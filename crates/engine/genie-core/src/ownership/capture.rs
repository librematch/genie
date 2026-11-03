//! Captures and takeovers: an animal or artifact a player takes over, its stop, the unit AI's
//! ownership change, a takeover by sight, a unit's ownership change, and the scan for more
//! livestock after a capture.

use crate::GenieError;
use crate::map::Position;
use crate::map::Tile;
use crate::math::fmath::below_or_unordered;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::unit::Unit;
use crate::orders::object_gone;
use crate::ownership::CaptureKind;
use crate::ownership::change::change_hands;
use crate::ownership::change::dead_master_of;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::StopOrder;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::lets::{release, torn_down};
use crate::world::others::OthersRef;
use crate::world::parts::Part;
use crate::world::step::Step;

impl Unit {
    /// `FUN_140aab570`: player `new` takes the animal over. Its stop (slot 0x148,
    /// `FUN_14108c050` -> `FUN_14106fa60`): its Artifact action's slot 0x38 (the unit AI of a
    /// class other than livestock would stop too, which is not modelled), then the ownership
    /// change (slot 0x70, [`Unit::change_owner`]); a livestock whose owner is then not Gaia has
    /// its new owner take Gaia's livestock in state 2 on the tiles it sees within 3 of the
    /// animal's tile, x then y, each the same way. Returns whether the animal changed hands and
    /// the others taken; the lists' moves, the storage and the class counters are the caller's.
    /// The new owner's computer AI part (`FUN_140aab130`), the local player's sound and
    /// statistics are not held.
    pub(crate) fn capture(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        new: PlayerId,
        roles: &Roles,
        slots: Slots,
    ) -> Result<(bool, Vec<ObjectId>), GenieError> {
        let class = self.placed_class(c)?;
        self.capture_stop(actions, c, class, roles)?;
        let changed = self.change_owner(actions, c, new, roles)?;
        // the scan reads the owner after the change: the old one when `new` lacks the master
        let mut taken = Vec::new();
        // The livestock class (`-0xe72` raw).
        // The siege class (`-0xe9f` raw), whose ownership change drops the units it carries
        // (`FUN_14120f800`).
        if roles.of_class(class).contains(ClassRoles::LIVESTOCK)
            && (if changed { new } else { c.player }) != PlayerId::GAIA
        {
            scan_livestock(c, self.position(), new, &mut taken, roles, slots)?;
        }
        Ok((changed, taken))
    }

    /// The master class the world files the captured object under.
    pub(crate) fn placed_class(&self, c: &Step<'_>) -> Result<ClassId, GenieError> {
        c.w.spatial
            .get(self.id)
            .map(|p| p.class)
            .ok_or(GenieError::unsupported(
                "a captured object the world does not hold",
            ))
    }

    /// The stop (slot 0x148, `FUN_14108c050`) of an animal or artifact of master class `class`
    /// from inside its update: the action object's stop (`FUN_14106fa60`: `+0x88a` 0, the top
    /// action's slot 0x38: the Artifact's, [`Artifact::stop`], keeps the action list: a walk's
    /// `MoveTo` goes and stops the unit, the target is let go, and the unit shows its standing
    /// sprite; without an Artifact the list goes, an animal or an artifact holding no Hunt,
    /// Gather, Build or Attack, whose slot 0x38 would keep it); then a unit AI of any class but
    /// livestock, not on action 0x2c2, stops (slot 0xb0 with 0: the order ends, the unit's
    /// stopAction) and forgets its targets (`FUN_140c4dcb0`), unless the unit's top action
    /// (slot 0x438) is a trade (0x6f) with a load of a kind (COMISS 0, amount; JNC: an amount
    /// that is not a number keeps it).
    ///
    /// [`Artifact::stop`]: crate::ownership::artifact::Artifact::stop
    pub(crate) fn capture_stop(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        class: ClassId,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let by = self.releaser(c)?;
        if let Some(artifact) = actions.list.artifact_mut() {
            let (gone, released) = artifact.stop();
            if gone.stops {
                c.w.parts
                    .mover_mut(self.id)
                    .ok_or(GenieError::unsupported(
                        "a walking animal without movement state",
                    ))?
                    .stop()?;
            }
            for t in [gone.released, released].into_iter().flatten() {
                release(c, self, t, by)?;
            }
            self.show_standing(
                c.w.parts.motion.layer_mut(self.id),
                OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                &c.w.model.graphics,
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            )?;
        } else {
            // any other top action's slot 0x38 is the base's false for what an animal or an
            // artifact holds (a `MoveTo` of its own), and the list goes
            let (mut releases, mut leaves) = (Vec::new(), Vec::new());
            self.tear_down_actions(
                (actions, c.w.parts.unread.get(self.id)),
                c.w.parts.motion.layer_mut(self.id),
                &mut releases,
                &mut leaves,
                by,
            )?;
            torn_down(c, self, (leaves, releases))?;
        }
        let trading = actions.top_action_kind() == Some(TRADE_ACTION)
            && below_or_unordered(0.0, self.life.amount.get())
            && self.life.attribute_type.is_some();
        if !roles.of_class(class).contains(ClassRoles::LIVESTOCK)
            && !trading
            && c.w
                .parts
                .unit_ai
                .get(self.id)
                .is_some_and(|ai| ai.current_action != Some(AiAction::HeroPower))
        {
            let place = (self.place.position().ground(), Some(class));
            // slot 0xb0 with 0, its stopAction a call on the unit at once
            self.run_calls(actions, c, |ai, side, rng| {
                ai.stop_with(StopOrder::End, Some(place), side, rng, roles)?;
                ai.forget_targets();
                Ok(())
            })?;
        }
        Ok(())
    }

    /// The unit's slot 0x70 in its own update: as its unit AI asks it ([`Effect::ChangeOwner`],
    /// the artifact's processIdle `FUN_140c1e410`), or a hit's kill gives it, a player's
    /// livestock, to Gaia ([`crate::combat::victims::file_kill`]): [`Unit::change_owner`]; once
    /// it changed hands the caller
    /// moves it to the new owner's list after the update ([`Outcome::Captured`]), and a building's
    /// whole change is the caller's even without the new owner's master.
    ///
    /// [`Effect::ChangeOwner`]: crate::unit_ai::Effect::ChangeOwner
    ///
    /// [`Outcome::Captured`]: crate::objects::unit::Outcome::Captured
    pub(crate) fn ai_change_owner(
        &mut self,
        actions: &mut Action,
        new: PlayerId,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let changed = self.change_owner(actions, c, new, roles)?;
        if changed || c.building_handover {
            c.taken_over = Some((new, changed));
        }
        Ok(())
    }

    /// `RGE_Combat_Object::update`'s takeover by sight of the unit by player `new`, which has its
    /// master: slot 0x70 ([`Unit::change_owner`]; a building's is the caller's after the update),
    /// then `+0x924` goes 0. The local player's sound and its capture notice (`FUN_1414c3380`)
    /// are not simulation. Not modelled: a siege weapon's (class 13) drop of the units it carries
    /// (`FUN_14120f800` with `FUN_141192d10`).
    pub(crate) fn take_by_sight(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        new: PlayerId,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        if roles.of_class(master.class).contains(ClassRoles::SIEGE) && !self.garrison.is_empty() {
            return Err(HANDS_LEFT);
        }
        let _ = self.change_owner(actions, c, new, roles)?;
        c.w.parts.combat_mut(self.id)?.capture_flag = CaptureKind::NONE;
        if let Some(s) = c.w.spatial.sight.as_mut().and_then(|v| v.seer_mut(self.id)) {
            s.capture = CaptureKind::NONE;
        }
        Ok(())
    }

    /// The ownership change (slot 0x70, [`change_hands`]) of the unit, from its owner to `new`:
    /// false when `new` has not its master. A building's (`FUN_1411cd860`) is the caller's after
    /// the update, whole ([`Captured::building`], [`crate::world::World::building_changes_hands`]),
    /// whether or not `new` has its master.
    pub(crate) fn change_owner(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        new: PlayerId,
        roles: &Roles,
    ) -> Result<bool, GenieError> {
        let master = self.master(c.w.master_tables(), c.player);
        if master.is_ok_and(|m| m.kind == MasterKind::BUILDING) {
            c.building_handover = true;
            return Ok(c
                .w
                .model
                .masters
                .get(new.index())
                .is_some_and(|m| m.get(self.life.master_id).is_some()));
        }
        let had = c.w.parts.dead_master.contains(self.id);
        let tables = MasterTables::new(&c.w.model.masters, &c.w.parts.own_master);
        let dead = dead_master_of(self, c.player.index(), tables, had);
        let id = self.id;
        let changed = change_hands(
            (self, actions, c.w.parts.unit_ai.get_mut(id)),
            (c.player.index(), new),
            tables,
            &mut c.w.spatial,
            c.w.world_time,
            roles,
            c.w.model.tuning.visibility,
        )?;
        let Some(own) = changed else {
            return Ok(false);
        };
        c.w.set_part(self.id, Part::OwnMaster(own))?;
        if let Some(d) = dead {
            c.w.set_part(self.id, Part::DeadMaster(Box::new(d)))?;
        }
        Ok(true)
    }
}

/// The ownership changes the slice does not model: a building's (its slot 0x70,
/// `FUN_1411cd860`: its annexes', its production queue's and researches', its garrison's, its
/// object and technology on capture, then the combat object's), which a takeover by sight and
/// the artifact's ask for one reach, and a siege weapon's with units inside (`FUN_14120f800`'s
/// class 13 branch: `FUN_141192d10`, each unit's leave, slot 0x1a8, with the siege weapon's slot
/// 0x1b0, and the rest destroyed, slot 0xd0).
pub(crate) const HANDS_LEFT: GenieError = GenieError::unsupported(
    "an ownership change of a building or of a siege weapon with units inside",
);

/// The trade action's type (slot 0x438), whose load keeps a captured unit's unit AI going
/// (`FUN_14108c050`).
pub(crate) const TRADE_ACTION: ActionKind = ActionKind::TRADE;

/// `FUN_140aab570`'s scan after a livestock's capture: Gaia's livestock in state 2 whose tile
/// the new owner sees (`FUN_141663500`), within 3 tiles of `at` (x outer, y inner); each is taken
/// in turn, with its own scan. Two on one tile would need the tile's object list order.
pub(crate) fn scan_livestock(
    c: &mut Step<'_>,
    at: Position,
    new: PlayerId,
    taken: &mut Vec<ObjectId>,
    roles: &Roles,
    slots: Slots,
) -> Result<(), GenieError> {
    let (tx, ty) = (at.x() as i32, at.y() as i32);
    let (w, h) = (c.w.spatial.map.width(), c.w.spatial.map.height());
    for x in (tx - 3).max(0)..=(tx + 3).min(w - 1) {
        for y in (ty - 3).max(0)..=(ty + 3).min(h - 1) {
            if !c.w.spatial.sees_tile(new, Tile::new(x, y)) {
                continue;
            }
            match livestock_on(c, Tile::new(x, y), taken, roles)?[..] {
                [] => {}
                [id] => {
                    if let Some(position) = take_livestock(c, id, new, slots, roles)? {
                        taken.push(id);
                        scan_livestock(c, position, new, taken, roles, slots)?;
                    }
                }
                _ => {
                    return Err(GenieError::unsupported(
                        "two livestock to take on one tile (its object list's order)",
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The scan's finds on tile (`x`, `y`): Gaia's livestock in state 2 of the updating object's
/// player's list whose centre is on the tile, not yet `taken`.
fn livestock_on(
    c: &Step<'_>,
    tile: Tile,
    taken: &[ObjectId],
    roles: &Roles,
) -> Result<Vec<ObjectId>, GenieError> {
    let (x, y) = (tile.x(), tile.y());
    Ok(OthersRef::of(&c.w.players, &c.w.index, c.player)?
        .own_iter()
        .filter(|u| {
            let (ux, uy) = (u.position().x(), u.position().y());
            u.life.state == ObjectState::Alive
                && ux as i32 == x
                && uy as i32 == y
                && !taken.contains(&u.id)
                && c.w.spatial.get(u.id).is_some_and(|p| {
                    roles.of_class(p.class).contains(ClassRoles::LIVESTOCK)
                        && p.player == PlayerId::GAIA
                })
        })
        .map(|u| u.id)
        .collect())
}

/// The scan takes livestock `id` for `new`: out of its slot while its stop reaches the view, as
/// the updating object, its capture stop and its ownership change. Its place when `new` took it;
/// None when `new` lacks the master and the animal stays Gaia's, which scans nothing.
fn take_livestock(
    c: &mut Step<'_>,
    id: ObjectId,
    new: PlayerId,
    slots: Slots,
    roles: &Roles,
) -> Result<Option<Position>, GenieError> {
    let mut unit =
        c.w.take_out_own(c.player, id)?
            .ok_or_else(|| object_gone(id))?;
    // its action layer goes out of the table with it, as the updating object's
    let mut actions = c.w.parts.take_actions(id);
    let done = (|| {
        unit.capture_stop(&mut actions, c, slots.class.livestock, roles)?;
        unit.change_owner(&mut actions, c, new, roles)
    })();
    let position = unit.position();
    c.w.parts.put_actions(id, actions);
    let _ = c.w.put_back_own(c.player, unit)?;
    Ok(done?.then_some(position))
}

/// What a takeover in an object's update did ([`Outcome::Captured`]): player `new` took the object
/// (`main`; without the master it stays) and then `also`, other objects of its player, in order.
/// Those taken move to the end of the new owner's list; their storage and class counters move
/// with them.
///
/// [`Outcome::Captured`]: crate::objects::unit::Outcome::Captured
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Captured {
    pub(crate) new: PlayerId,
    pub(crate) main: bool,
    pub(crate) also: Vec<ObjectId>,
    /// The object is a building: its slot 0x70 (`FUN_1411cd860`) is the caller's, whole
    /// ([`crate::world::World::building_changes_hands`]); `main` says whether the new owner has
    /// its master.
    pub(crate) building: bool,
}
