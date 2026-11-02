//! The gates: the units in a gate, a player's gate list and passes, and the gates opened before
//! and closed after a player's update.

use crate::GenieError;
use crate::map::Rect;
use crate::map::spatial::Obstruction;
use crate::map::spatial::{RecordId, Spatial, WalkQuery};
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::layers::Building;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::player_master;
use crate::objects::masters::{Master, OwnerMasters};
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use crate::world::index::ObjectsRef;
use crate::world::slots::PlayerSlots;
use crate::world::{GATE_COUNT_CAP, GATE_GROW, GATE_QUERY_RECORDS, World};

/// `FUN_1411d4a30`, a building's slot 0x678 (any other object's answers no): whether the gate
/// `gate` (its building layer `layer`, master `master`, of player `owner`) lets a unit of
/// `player` pass. It does below state
/// 3, as a gate (class 39) whose master transforms (`+0x9ba`) and that is not locked (`+0xc7c`),
/// when its owner counts the player as an ally (`FUN_141663dc0`) and the player its owner
/// (`FUN_141663d90`), each with the ally flag.
#[must_use]
pub fn lets_pass(
    (gate, layer, master): (&Unit, &Building, &Master),
    owner: PlayerId,
    player: PlayerId,
    spatial: &Spatial,
) -> bool {
    let transforms = master
        .building
        .as_ref()
        .is_some_and(|b| b.transform_unit.is_some());
    master.kind == MasterKind::BUILDING
        && gate.life.state < ObjectState::Dying
        && master.class == class::GATE
        && transforms
        && !layer.gate.is_some_and(|g| g.locked)
        && spatial.allied_with(owner, player)
        && spatial.allied_with(player, owner)
}

/// The units in gate `gate` (`FUN_1411d0570`): the obstruction manager's query from its first
/// annex to its second (`+0xbc0`, `+0xbc8`) with flags 0x10, the gate and its annexes ignored
/// (`FUN_1410562e0`), every record and the query grown by [`GATE_GROW`] (`FUN_1410562d0`), any
/// kind and player ([`Spatial::walk_hits`]; its list holds 10 records, more are not modelled).
/// Of the objects it lists, those not of the classes 27, 39 and 52, nor of class 3 standing
/// still, and either moving or not Gaia's count, at most 25 (the game reads the object's top
/// speed, slot 0x208: not 0 and at least [`crate::world::GATE_EPSILON`] just when its master moves,
/// [`crate::map::spatial::Placed::moves`], for every speed the data gives); the second answer is how many of them are of a player the gate's counts an ally
/// (`FUN_141663d90` with the ally flag).
pub(crate) fn gate_units(
    spatial: &mut Spatial,
    gate: ObjectId,
    annexes: &[ObjectId],
    by_id: ObjectsRef<'_>,
) -> Result<(Vec<ObjectId>, usize), GenieError> {
    let [a, b] = annexes[..] else {
        return Err(GenieError::unsupported("a gate without its two annexes"));
    };
    let place = |id| {
        spatial
            .get(id)
            .map(|p| p.position.ground())
            .ok_or(GenieError::unsupported("a gate's annex without a place"))
    };
    let (from, to) = (place(a)?, place(b)?);
    let rect = Rect::spanning(from, to);
    let owner = spatial
        .get(gate)
        .map(|p| p.player)
        .ok_or(GenieError::unsupported("a gate without a place"))?;
    spatial.check_known_records(rect.grown(GATE_GROW))?;
    let hits = spatial.walk_hits(WalkQuery {
        start: from,
        end: to,
        rect,
        grow: GATE_GROW,
        ally_factor: 1.0,
        player: Some(owner),
        ignore: &[RecordId::from(gate), RecordId::from(a), RecordId::from(b)],
        objects: by_id,
    })?;
    if GATE_QUERY_RECORDS < hits.len() {
        return Err(GenieError::unsupported(
            "a gate's query of more than 10 records",
        ));
    }
    let (mut units, mut allied) = (Vec::new(), 0);
    for id in hits {
        if GATE_COUNT_CAP <= units.len() {
            break;
        }
        let Some((id, p)) = id.key().and_then(|o| Some((o, spatial.get(o)?))) else {
            continue;
        };
        let counts = !matches!(p.class, class::WALL | class::GATE | class::TOWER)
            && (p.class != class::BUILDING || p.moves)
            && (p.moves || p.player != PlayerId::GAIA);
        if counts {
            units.push(id);
            if spatial.allied_with(owner, p.player) {
                allied += 1;
            }
        }
    }
    Ok((units, allied))
}

impl World {
    /// Player `q`'s gate list (`+0x1360`, `FUN_141616200` adds to its front): its live objects
    /// of class 39 whose master transforms ([`crate::objects::masters::BuildingMaster::transform_unit`]),
    /// newest first.
    pub(crate) fn gate_list(&self, q: PlayerId) -> Result<Vec<ListIndex>, GenieError> {
        let masters = self.model.masters.of(q);
        let owner = self.players.player(q)?;
        let mut gates = Vec::new();
        for i in owner.objects.places().rev() {
            let m = owner
                .objects
                .checked(i)?
                .master_in(OwnerMasters::new(&self.parts.own_master, masters))?;
            if m.class == class::GATE
                && m.building
                    .as_ref()
                    .is_some_and(|b| b.transform_unit.is_some())
            {
                gates.push(i);
            }
        }
        Ok(gates)
    }

    /// A gate's slot 0x678 ([`lets_pass`]) for object `i` of player `q` and a unit of player `p`.
    pub(crate) fn gate_passes(
        &self,
        q: PlayerId,
        i: ListIndex,
        p: PlayerId,
    ) -> Result<bool, GenieError> {
        let gate = self.players.live(q, i)?;
        let m = gate.master(self.master_tables(), q)?;
        let layer = self.parts.building_of(gate.id);
        Ok(lets_pass((gate, layer, m), q, p, &self.spatial))
    }

    /// `BaseWorld::update` before player `p` updates (not Gaia): the gates of each player `q`
    /// that is `p` or counts `p` an ally and is counted one by it, unless `q`'s are open already
    /// (`open`, the update's flags), open for `p` (`q`'s slot 0x318, `FUN_14162faf0`): each gate
    /// `p` may pass ([`World::gate_passes`]) loses its record, its tiles' marks and its mesh shape
    /// (`FUN_141055b40`), so `p`'s units walk and plan through it.
    pub(crate) fn open_gates(&mut self, p: PlayerId, open: &mut [bool]) -> Result<(), GenieError> {
        if p == PlayerId::GAIA {
            return Ok(());
        }
        let players = self.players.len();
        for (q, is_open) in PlayerId::each(players).zip(open.iter_mut()).skip(1) {
            if *is_open
                || (q != p && !(self.spatial.allied_with(p, q) && self.spatial.allied_with(q, p)))
            {
                continue;
            }
            for i in self.gate_list(q)? {
                if !self.gate_passes(q, i, p)? {
                    continue;
                }
                let id = self.players.live(q, i)?.id;
                if self.spatial.get(id).is_some_and(|g| {
                    g.obstruction
                        .is_some_and(|o| o.kind == ObstructionKind::TYPE_10)
                }) {
                    return Err(GenieError::unsupported(
                        "a gate of obstruction kind 10 opens (FUN_140f65580)",
                    ));
                }
                self.spatial.drop_record(id);
                self.pathing.removed(id);
            }
            *is_open = true;
        }
        self.pathing.tiles_changed(self.spatial.take_changes());
        Ok(())
    }

    /// `BaseWorld::update` after player `p` updated (not Gaia): each open player `q`'s gates close
    /// (`q`'s slot 0x320, `FUN_141619480`) unless the next player counts `q` an ally and is
    /// counted one by it (they stay open for it): each gate `p` may pass gets its record, its
    /// tiles' marks and its mesh shape again (`FUN_141056460`), but for a part of obstruction
    /// kind 0 or 12 or of class 0 ([`crate::map::spatial::Obstruction::recorded`]).
    pub(crate) fn close_gates(&mut self, p: PlayerId, open: &mut [bool]) -> Result<(), GenieError> {
        if p == PlayerId::GAIA {
            return Ok(());
        }
        let players = self.players.len();
        let next = PlayerId::try_from(p.index() + 1)
            .ok()
            .filter(|n| n.index() < players);
        for (q, is_open) in PlayerId::each(players).zip(open.iter_mut()).skip(1) {
            if !*is_open {
                continue;
            }
            if next
                .is_some_and(|n| self.spatial.allied_with(n, q) && self.spatial.allied_with(q, n))
            {
                continue;
            }
            for i in self.gate_list(q)? {
                if !self.gate_passes(q, i, p)? {
                    continue;
                }
                let gate = &self.players.live(q, i)?;
                let (id, master) = (gate.id, gate.life.master_id);
                let Some(m) = player_master(&self.model.masters, q, master) else {
                    continue;
                };
                let record = Obstruction {
                    kind: m.obstruction_kind,
                    class: m.obstruction_class,
                };
                // a gate part of obstruction kind 0 (the gate's middle, master 91) gets none
                if record.recorded()
                    && self
                        .spatial
                        .get(id)
                        .is_some_and(|g| g.obstruction.is_none())
                {
                    self.spatial.add_record(id, record);
                    if let Some(placed) = self.spatial.get(id).copied() {
                        self.pathing.place_shape(id, &placed);
                    }
                }
            }
            *is_open = false;
        }
        self.pathing.tiles_changed(self.spatial.take_changes());
        Ok(())
    }
}
