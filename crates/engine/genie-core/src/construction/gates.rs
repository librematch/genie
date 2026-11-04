//! The gates: the units in a gate, a player's gate list and passes, and the gates opened before
//! and closed after a player's update.

use crate::map::Distance;
use crate::map::Rect;
use crate::map::spatial::Obstruction;
use crate::map::spatial::{RecordId, Spatial, WalkQuery};
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::layers::Building;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::player_master;
use crate::objects::masters::{Master, OwnerMasters};
use crate::objects::unit::Unit;
use crate::objects::{ObjectId, ObjectKey};
use crate::players::PlayerId;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlotsExt;
use crate::world::{GATE_COUNT_CAP, GATE_QUERY_RECORDS, World};
use crate::{GenieError, GenieResult};

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
    roles: &Roles,
) -> bool {
    let transforms = master
        .building
        .as_ref()
        .is_some_and(|building_master| building_master.transform_unit.is_some());
    master.kind == MasterKind::BUILDING
        && gate.life.state < ObjectState::Dying
        && roles.of_class(master.class).contains(ClassRoles::GATE)
        && transforms
        && !layer.gate.is_some_and(|gate| gate.locked)
        && spatial.allied_with(owner, player)
        && spatial.allied_with(player, owner)
}

/// The units in gate `gate` (`FUN_1411d0570`): the obstruction manager's query from its first
/// annex to its second (`+0xbc0`, `+0xbc8`) with flags 0x10, the gate and its annexes ignored
/// (`FUN_1410562e0`), every record and the query grown by `TurnTuning::gate_grow` (`FUN_1410562d0`), any
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
) -> GenieResult<(Vec<ObjectId>, usize)> {
    let turn_tuning = &by_id.tuning().turn;
    let roles = by_id.roles();
    let [first, second] = annexes[..] else {
        return Err(GenieError::unsupported("a gate without its two annexes"));
    };
    let place = |id| {
        spatial
            .get(id)
            .map(|placed| placed.position.ground())
            .ok_or(GenieError::unsupported("a gate's annex without a place"))
    };
    let (from, to) = (place(first)?, place(second)?);
    let rect = Rect::spanning(from, to);
    let owner = spatial
        .get(gate)
        .map(|placed| placed.player)
        .ok_or(GenieError::unsupported("a gate without a place"))?;
    spatial.check_known_records(rect.grown(Distance::new(turn_tuning.gate_grow)))?;
    let hits = spatial.walk_hits(WalkQuery {
        start: from,
        end: to,
        rect,
        grow: Distance::new(turn_tuning.gate_grow),
        ally_factor: 1.0,
        player: Some(owner),
        ignore: &[
            RecordId::from(gate),
            RecordId::from(first),
            RecordId::from(second),
        ],
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
        let Some((id, placed)) = id
            .key()
            .and_then(|object_id| Some((object_id, spatial.get(object_id)?)))
        else {
            continue;
        };
        let counts = !roles
            .of_class(placed.class)
            .intersects(ClassRoles::WALL | ClassRoles::GATE | ClassRoles::TOWER)
            && (!roles.of_class(placed.class).contains(ClassRoles::BUILDING) || placed.moves)
            && (placed.moves || placed.player != PlayerId::GAIA);
        if counts {
            units.push(id);
            if spatial.allied_with(owner, placed.player) {
                allied += 1;
            }
        }
    }
    Ok((units, allied))
}

impl World {
    /// Player `player_id`'s gate list (`+0x1360`, `FUN_141616200` adds to its front): its live
    /// objects of class 39 whose master transforms
    /// ([`crate::objects::masters::BuildingMaster::transform_unit`]), newest first.
    pub(crate) fn gate_list(&self, player_id: PlayerId) -> GenieResult<Vec<ListIndex>> {
        let masters = self.model.masters.of(player_id);
        let owner = self.players.player(player_id)?;
        let mut gates = Vec::new();
        for list_index in owner.objects.places().rev() {
            let master = owner
                .objects
                .checked(list_index)?
                .master_in(OwnerMasters::new(&self.parts.own_master, masters))?;
            if self
                .model
                .roles
                .of_class(master.class)
                .contains(ClassRoles::GATE)
                && master
                    .building
                    .as_ref()
                    .is_some_and(|building_master| building_master.transform_unit.is_some())
            {
                gates.push(list_index);
            }
        }
        Ok(gates)
    }

    /// A gate's slot 0x678 ([`lets_pass`]) for object `list_index` of player `gate_owner` and a
    /// unit of player `unit_owner`.
    pub(crate) fn gate_passes(
        &self,
        gate_owner: PlayerId,
        list_index: ListIndex,
        unit_owner: PlayerId,
    ) -> GenieResult<bool> {
        let gate = self.players.live(gate_owner, list_index)?;
        let master = gate.master(self.master_tables(), gate_owner)?;
        let layer = self.parts.building_of(gate.id);
        Ok(lets_pass(
            (gate, layer, master),
            gate_owner,
            unit_owner,
            &self.spatial,
            &self.model.roles,
        ))
    }

    /// `BaseWorld::update` before player `gate_owner` updates (not Gaia): the gates of each player
    /// `player_id` that is `gate_owner` or counts `gate_owner` an ally and is counted one by it,
    /// unless `player_id`'s are open already (`open`, the update's flags), open for `gate_owner`
    /// (`player_id`'s slot 0x318, `FUN_14162faf0`): each gate `gate_owner` may pass
    /// ([`World::gate_passes`]) loses its record, its tiles' marks and its mesh shape
    /// (`FUN_141055b40`), so `gate_owner`'s units walk and plan through it.
    pub(crate) fn open_gates(
        &mut self,
        gate_owner: PlayerId,
        open: &mut [bool],
    ) -> GenieResult<()> {
        if gate_owner == PlayerId::GAIA {
            return Ok(());
        }
        let players = self.players.len();
        for (player_id, is_open) in PlayerId::each(players).zip(open.iter_mut()).skip(1) {
            if *is_open
                || (player_id != gate_owner
                    && !(self.spatial.allied_with(gate_owner, player_id)
                        && self.spatial.allied_with(player_id, gate_owner)))
            {
                continue;
            }
            for list_index in self.gate_list(player_id)? {
                if !self.gate_passes(player_id, list_index, gate_owner)? {
                    continue;
                }
                let id = self.players.live(player_id, list_index)?.id;
                if self.spatial.get(id).is_some_and(|placed| {
                    placed
                        .obstruction
                        .is_some_and(|obstruction| obstruction.kind == ObstructionKind::KIND_10)
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

    /// `BaseWorld::update` after player `gate_owner` updated (not Gaia): each open player `other`'s
    /// gates close (`other`'s slot 0x320, `FUN_141619480`) unless the next player counts `other` an
    /// ally and is counted one by it (they stay open for it): each gate `gate_owner` may pass gets
    /// its record, its tiles' marks and its mesh shape again (`FUN_141056460`), but for a part of
    /// obstruction kind 0 or 12 or of class 0 ([`crate::map::spatial::Obstruction::recorded`]).
    pub(crate) fn close_gates(
        &mut self,
        gate_owner: PlayerId,
        open: &mut [bool],
    ) -> GenieResult<()> {
        if gate_owner == PlayerId::GAIA {
            return Ok(());
        }
        let players = self.players.len();
        let next = PlayerId::try_from(gate_owner.index() + 1)
            .ok()
            .filter(|player_id| player_id.index() < players);
        for (other, is_open) in PlayerId::each(players).zip(open.iter_mut()).skip(1) {
            if !*is_open {
                continue;
            }
            if next.is_some_and(|player_id| {
                self.spatial.allied_with(player_id, other)
                    && self.spatial.allied_with(other, player_id)
            }) {
                continue;
            }
            for list_index in self.gate_list(other)? {
                if !self.gate_passes(other, list_index, gate_owner)? {
                    continue;
                }
                let gate = &self.players.live(other, list_index)?;
                let (id, master) = (gate.id, gate.life.master_id);
                let Some(master) = player_master(&self.model.masters, other, master) else {
                    continue;
                };
                let record = Obstruction {
                    kind: master.obstruction_kind,
                    class: master.obstruction_class,
                };
                // a gate part of obstruction kind 0 (the gate's middle, master 91) gets none
                if record.recorded()
                    && self
                        .spatial
                        .get(id)
                        .is_some_and(|placed| placed.obstruction.is_none())
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
