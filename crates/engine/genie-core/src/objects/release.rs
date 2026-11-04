//! An object's leave of the world and its release by the action that held it (slot 0x180); the
//! update's regeneration and hit point check, and a walk's pause.

use crate::GenieResult;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::layers::Building;
use crate::objects::lifecycle::death_parts::{count_class, forget_resource};
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::Notify;
use crate::unit_ai::{NotifyKind, UnitAi};
use crate::world::Seconds;
use crate::world::World;
use crate::world::parts::Object;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;

/// The object whose action lets its target go (the target's slot 0x180 takes it): its id, its
/// master's class and its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Releaser {
    pub(crate) id: ObjectId,
    pub(crate) class: Option<ClassId>,
    pub(crate) player: PlayerId,
}

/// Slot 0x180 on object `unit` of master type `kind`, let go by `by`: the static object's
/// (`FUN_141197d00`, types 10 and 15) counts a worker less and a static one without workers
/// left, alive (state 2 with hit points, or 6), goes to sleep; the animated object's
/// (`FUN_141071ea0`, types 20 to 40) only counts the worker less. A combat object's (`FUN_141086e60`) counts the worker
/// less (`FUN_141071ea0`) and tells its unit AI that the releaser no longer acts on it (notify
/// 699, the releaser's master class as its first parameter; none from an object of a negative
/// id). A building's (`FUN_1411d6020`) first drops the releaser's first entry from its attacking
/// units (`+0xce0`, the rest keeping their order) when it lists it (`listed`, [`Unit::lists_taker`]).
/// `ai` is the object's unit AI, `layer` its building layer (none: no attacking units).
pub(crate) fn released(
    unit: &mut Unit,
    (ai, layer): (Option<&mut UnitAi>, Option<&mut Building>),
    (kind, listed): (MasterKind, bool),
    by: Releaser,
) -> GenieResult<()> {
    let state = unit.life.state;
    match (released_fields(unit, layer, (kind, listed), by), ai) {
        (Some(notify), Some(ai)) => ai.post(notify, state),
        _ => Ok(()),
    }
}

/// [`released`] on the object `unit` itself and its building layer `layer`; returns the notify
/// 699 a combat object's unit AI is told (none for a releaser of a negative id), which
/// [`released`] posts.
pub(crate) fn released_fields(
    unit: &mut Unit,
    layer: Option<&mut Building>,
    (kind, listed): (MasterKind, bool),
    by: Releaser,
) -> Option<Notify> {
    if listed
        && let Some(layer) = layer
        && let Some(index) = layer
            .attacking_units
            .iter()
            .position(|&object_id| object_id == by.id)
    {
        let _ = layer.attacking_units.remove(index);
    }
    if WorkerCount::ZERO < unit.workers {
        unit.workers = unit.workers.plus(-1);
    }
    // COMISS hp with 0, JNC: a NaN does not count as alive
    let alive = (unit.life.state == ObjectState::Alive && HitPoints::ZERO <= unit.life.hp)
        || unit.life.state == ObjectState::Undead;
    // The first master type whose slot 0x180 is not the static object's (`FUN_141197d00`, types 10
    // and 15): the animated object's (`FUN_141071ea0`, types 20 to 40) only counts a worker less.
    if kind < MasterKind::ANIMATED && unit.workers == WorkerCount::ZERO && alive {
        unit.life.goto_sleep = true;
    }
    // a combat object's (types 50 and up) unit AI
    (MasterKind::COMBAT <= kind && !by.id.is_recyclable()).then(|| Notify {
        caller: Some(by.id),
        recipient: unit.id,
        kind: NotifyKind::NO_LONGER_ON_ME,
        params: [i32::from(ClassId::field(by.class)), 0, 0],
    })
}

impl World {
    /// The leave of object `gone`, out of player `player_id`'s live list already (its list's update
    /// that returned 1, `FUN_14116b5e0`): its player's remove (slot 0x2d8) takes it out of the
    /// tracker, it leaves the visible-resource lists ([`forget_resource`]), a combat object's
    /// destructor lowers its player's class counters (`FUN_141200450`), it leaves the world's
    /// places and its owner's watched list (`FUN_141091310`), a recyclable master's missile goes to
    /// the pool of type 60 (the world's `+0x1d8`), and it leaves the mesh. Every step runs; returns
    /// the world's leave of the place that the slice does not model.
    pub(crate) fn leave_world(&mut self, player_id: PlayerId, gone: &Object) -> GenieResult<()> {
        let (missile, gone) = (gone.parts.missile, &gone.unit);
        self.track_remove(player_id, gone.id, gone.life.master_id)?;
        let master = gone.master(
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player_id,
        )?;
        forget_resource(&mut self.spatial, Some(master), gone.id, gone.position())?;
        if MasterKind::COMBAT <= master.kind {
            count_class(
                &mut self.players.player_mut(player_id)?.attributes,
                master,
                -1.0,
                &self.model.roles,
            );
        }
        let recyclable = master.recyclable;
        let left = self.spatial.leave(gone.id);
        self.unwatch(player_id, gone.id)?;
        if let Some(missile) = missile.filter(|_| recyclable.recycled()) {
            self.recycling.missiles.push((gone.id, missile.flying));
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
    ) -> GenieResult<()> {
        let Some((player_id, list_index)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(player_id, list_index)?;
        let master = unit.master(self.master_tables(), player_id)?;
        if !matches!(master.kind, MasterKind::COMBAT | MasterKind::BUILDING) {
            return Ok(());
        }
        let master = master.clone();
        let unit = self.players.live_mut(player_id, list_index)?;
        unit.regenerate(
            self.parts.combat_mut(unit.id)?,
            &master,
            dt_seconds,
            &self.model.tuning.turn,
        );
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
    ) -> GenieResult<()> {
        let Some((player_id, list_index)) = self.live_slot(slot) else {
            return Ok(());
        };
        let unit = self.players.live(player_id, list_index)?;
        let combat = matches!(
            unit.master(self.master_tables(), player_id)?.kind,
            MasterKind::COMBAT | MasterKind::BUILDING
        );
        if combat {
            let unit = self.players.live_mut(player_id, list_index)?;
            unit.clamp_hp_powerup(self.parts.combat_mut(unit.id)?, &self.model.tuning.turn);
        }
        Ok(())
    }
}

impl World {
    /// Slot 0x5f0 (`FUN_141158b90`): the unit's walked level, if it has one, pauses for `pause`
    /// seconds; the world's view of it follows.
    pub(crate) fn pause(&mut self, id: ObjectId, pause: Seconds) {
        let Some(unit) = self
            .players
            .iter_mut()
            .flat_map(|player| player.objects.iter_mut())
            .find(|unit| unit.id == id)
        else {
            return;
        };
        if let Some(level) = self
            .parts
            .mover_mut(unit.id)
            .and_then(|mover| mover.paths.first_mut())
        {
            level.pause_time = pause;
        }
    }
}

impl Unit {
    /// The unit as the releaser of its actions' targets (slot 0x180's argument).
    pub(crate) fn releaser(&self, step: &Step<'_>) -> GenieResult<Releaser> {
        Ok(Releaser {
            id: self.id,
            class: Some(self.master(step.world.master_tables(), step.player)?.class),
            player: step.player,
        })
    }
}
