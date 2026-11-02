//! An object's leave of the world and its release by the action that held it (slot 0x180); the
//! update's regeneration and hit point check, and a walk's pause.

use crate::GenieError;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Building;
use crate::objects::lifecycle::death_parts::{count_class, forget_resource};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::master_type::ANIMATED;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::Notify;
use crate::world::Seconds;
use crate::world::World;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// The object whose action lets its target go (the target's slot 0x180 takes it): its id, its
/// master's class and its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Releaser {
    pub(crate) id: ObjectId,
    pub(crate) class: Option<ClassId>,
    pub(crate) player: PlayerId,
}

/// Slot 0x180 on object `u` of master type `kind`, let go by `by`: the static object's
/// (`FUN_141197d00`, types 10 and 15) counts a worker less and a static one without workers
/// left, alive (state 2 with hit points, or 6), goes to sleep; the animated object's
/// (`FUN_141071ea0`, types 20 to 40) only counts the worker less. A combat object's (`FUN_141086e60`) counts the worker
/// less (`FUN_141071ea0`) and tells its unit AI that the releaser no longer acts on it (notify
/// 699, the releaser's master class as its first parameter; none from an object of a negative
/// id). A building's (`FUN_1411d6020`) first drops the releaser's first entry from its attacking
/// units (`+0xce0`, the rest keeping their order) when it lists it (`listed`, [`Unit::lists_taker`]).
/// `ai` is the object's unit AI, `layer` its building layer (none: no attacking units).
pub(crate) fn released(
    u: &mut Unit,
    (ai, layer): (Option<&mut crate::unit_ai::UnitAi>, Option<&mut Building>),
    (kind, listed): (MasterKind, bool),
    by: Releaser,
) -> Result<(), GenieError> {
    let state = u.life.state;
    match (released_fields(u, layer, (kind, listed), by), ai) {
        (Some(n), Some(ai)) => ai.post(n, state),
        _ => Ok(()),
    }
}

/// [`released`] on the object `u` itself and its building layer `layer`; returns the notify
/// 699 a combat object's unit AI is told (none for a releaser of a negative id), which
/// [`released`] posts.
pub(crate) fn released_fields(
    u: &mut Unit,
    layer: Option<&mut Building>,
    (kind, listed): (MasterKind, bool),
    by: Releaser,
) -> Option<Notify> {
    if listed
        && let Some(layer) = layer
        && let Some(i) = layer.attacking_units.iter().position(|&t| t == by.id)
    {
        let _ = layer.attacking_units.remove(i);
    }
    if crate::objects::WorkerCount::ZERO < u.workers {
        u.workers = u.workers.plus(-1);
    }
    // COMISS hp with 0, JNC: a NaN does not count as alive
    let alive = (u.life.state == ObjectState::Alive && HitPoints::ZERO <= u.life.hp)
        || u.life.state == ObjectState::Undead;
    // The first master type whose slot 0x180 is not the static object's (`FUN_141197d00`, types 10
    // and 15): the animated object's (`FUN_141071ea0`, types 20 to 40) only counts a worker less.
    if kind < ANIMATED && u.workers == crate::objects::WorkerCount::ZERO && alive {
        u.life.goto_sleep = true;
    }
    // a combat object's (types 50 and up) unit AI
    (COMBAT <= kind && !by.id.is_recyclable()).then(|| Notify {
        caller: Some(by.id),
        recipient: u.id,
        kind: crate::unit_ai::NotifyKind::NO_LONGER_ON_ME,
        p: [i32::from(ClassId::field(by.class)), 0, 0],
    })
}

impl World {
    /// The leave of object `gone`, out of player `p`'s live list already (its list's update that
    /// returned 1, `FUN_14116b5e0`): its player's remove (slot 0x2d8) takes it out of the tracker,
    /// it leaves the visible-resource lists ([`forget_resource`]), a combat object's destructor
    /// lowers its player's class counters (`FUN_141200450`), it leaves the world's places and its
    /// owner's watched list (`FUN_141091310`), a recyclable master's missile goes to the pool of
    /// type 60 (the world's `+0x1d8`), and it leaves the mesh. Every step runs; returns the
    /// world's leave of the place that the slice does not model.
    pub(crate) fn leave_world(
        &mut self,
        p: PlayerId,
        gone: &crate::world::parts::Object,
    ) -> Result<(), GenieError> {
        let (missile, gone) = (gone.parts.missile, &gone.unit);
        self.track_remove(p, gone.id, gone.life.master_id)?;
        let master = gone.master(
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            p,
        )?;
        forget_resource(&mut self.spatial, Some(master), gone.id, gone.position())?;
        if COMBAT <= master.kind {
            count_class(&mut self.players.player_mut(p)?.attributes, master, -1.0);
        }
        let recyclable = master.recyclable;
        let left = self.spatial.leave(gone.id);
        self.unwatch(p, gone.id)?;
        if let Some(m) = missile.filter(|_| recyclable) {
            self.recycling.missiles.push((gone.id, m.flying));
        }
        self.pathing.removed(gone.id);
        self.pathing.tiles_changed(self.spatial.take_changes());
        left
    }

    /// `TRIBE_Combat_Object::update`'s regeneration ([`Unit::regenerate`]) of the object in
    /// `slot` ([`World::live_slot`]), a combat object or building (types 70 and 80) by its own
    /// master, else its player's.
    ///
    /// # Errors
    ///
    /// A slot the player's list does not hold.
    pub(crate) fn regenerate_object(
        &mut self,
        slot: (PlayerId, ListIndex, ObjectId),
        dt_seconds: Seconds,
    ) -> Result<(), GenieError> {
        let Some((p, i)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(p, i)?;
        let master = unit.master(self.master_tables(), p)?;
        if !matches!(master.kind, COMBAT | BUILDING) {
            return Ok(());
        }
        let master = master.clone();
        let u = self.players.live_mut(p, i)?;
        u.regenerate(self.parts.combat_mut(u.id)?, &master, dt_seconds);
        Ok(())
    }

    /// `TRIBE_Combat_Object::update`'s hit point powerup check ([`Unit::clamp_hp_powerup`]) of the
    /// object in `slot` ([`World::live_slot`]), a combat object or building (types 70 and 80).
    ///
    /// # Errors
    ///
    /// A slot the player's list does not hold.
    pub(crate) fn clamp_object_powerup(
        &mut self,
        slot: (PlayerId, ListIndex, ObjectId),
    ) -> Result<(), GenieError> {
        let Some((p, i)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(p, i)?;
        let combat = matches!(
            unit.master(self.master_tables(), p)?.kind,
            COMBAT | BUILDING
        );
        if combat {
            let u = self.players.live_mut(p, i)?;
            u.clamp_hp_powerup(self.parts.combat_mut(u.id)?);
        }
        Ok(())
    }
}

impl World {
    /// Slot 0x5f0 (`FUN_141158b90`): the unit's walked level, if it has one, pauses for `pause`
    /// seconds; the world's view of it follows.
    pub(crate) fn pause(&mut self, id: ObjectId, pause: Seconds) {
        let Some(u) = self
            .players
            .iter_mut()
            .flat_map(|p| p.objects.iter_mut())
            .find(|u| u.id == id)
        else {
            return;
        };
        if let Some(level) = u.body.mover_mut().and_then(|m| m.paths.first_mut()) {
            level.pause_time = pause;
        }
    }
}

impl Unit {
    /// The unit as the releaser of its actions' targets (slot 0x180's argument).
    pub(crate) fn releaser(&self, c: &Step<'_>) -> Result<Releaser, GenieError> {
        Ok(Releaser {
            id: self.id,
            class: Some(self.master(c.w.master_tables(), c.player)?.class),
            player: c.player,
        })
    }
}
