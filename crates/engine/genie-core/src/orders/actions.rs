//! Taking and releasing targets, the order record, actions cleared, transformations and
//! gates.

use crate::construction::gates::gate_units;
use crate::map::Distance;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::release::Releaser;
use crate::orders::ActionsOnChange;
use crate::orders::TROPHIES;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderRecord;
use crate::unit_ai::best_target::category;
use crate::world::Millis;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// Slot 0x178 on the object an action of `by` takes as its target, outside an update
    /// ([`crate::world::lets::Lets::take`] on the whole world): a building's (`FUN_1411dba70`)
    /// first lists an unallied taker of a foundation with a stack unit ([`Unit::taken`]); then
    /// the base take (`FUN_14119d3e0`): its worker count (`+0x644`) rises, to at most 550; a
    /// sleeping object (static, never a building) wakes: it leaves its player's sleeping list
    /// (slot 0x2d8, the last object taking its place) for the end of the live list (slot
    /// 0x2d0). A live object flagged to sleep only drops the flags.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::Lets::take`]'s.
    ///
    /// [`Unit::taken`]: crate::objects::unit::Unit::taken
    pub(crate) fn take(&mut self, id: ObjectId, by: Releaser) -> GenieResult<()> {
        let roles = self.model.roles.clone();
        self.with_lets(by.player, |lets, others| {
            lets.take(others, None, id, by, &roles)
        })?
    }

    /// Slot 0x180 on the object an action of `by` lets go, outside an update
    /// ([`crate::world::lets::Lets::release`] on the whole world,
    /// [`crate::objects::release::released`]): a building's (`FUN_1411d6020`) drops an unallied
    /// releaser from the attacking units of a foundation with a stack unit
    /// ([`Unit::lists_taker`]); its worker count falls; a static object (type below 30,
    /// `FUN_141197d00`) that no one works on any more and is alive (state 2 with hit points at
    /// least 0) or in state 6 is flagged to sleep (`+0x641`, `+0x643`); a combat object
    /// (`FUN_141086e60`) tells its unit AI (notify 699). A sleeping object only counts a worker
    /// less.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::Lets::release`]'s.
    ///
    /// [`Unit::lists_taker`]: crate::objects::unit::Unit::lists_taker
    pub(crate) fn release(&mut self, id: ObjectId, by: Releaser) -> GenieResult<()> {
        self.with_lets(by.player, |lets, others| lets.release(others, None, id, by))?
    }

    /// Object `idx` of player `player` as the releaser of its actions' targets.
    pub(crate) fn releaser(&self, player: PlayerId, idx: ListIndex) -> GenieResult<Releaser> {
        let unit = self.players.live(player, idx)?;
        Ok(Releaser {
            id: unit.id,
            class: Some(unit.master(self.master_tables(), player)?.class),
            player,
        })
    }

    /// `FUN_140c42080` after the notify of a new action (`FUN_14106e4e0`, the combat object's
    /// `FUN_14121c890`, the attack order of `FUN_14161b970`) of object `idx` of player `player`:
    /// with a target the world holds (`FUN_1415417a0`), its unit AI remembers `order` and
    /// `action` on it ([`crate::unit_ai::UnitAi::remember_order`]) at the world's time, with the
    /// unit's and the target's slot 0x520 ([`Unit::moved_vector`]: x, height, y) and the target's
    /// attack category by its master
    /// ([`crate::unit_ai::best_target::category`]: its own, else its owner's). An order on a point (no
    /// target) records nothing.
    ///
    /// [`Unit::moved_vector`]: crate::objects::unit::Unit::moved_vector
    pub(crate) fn remember_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (order, action): (Option<OrderKind>, Option<AiAction>),
    ) -> GenieResult<()> {
        let live = &self.players.live(player, idx)?;
        let Some(target) = self
            .parts
            .unit_ai
            .get(live.id)
            .and_then(|unit_ai| unit_ai.current_target)
            .filter(|object_id| !object_id.is_recyclable())
        else {
            return Ok(());
        };
        let position = live.moved_vector(self.parts.mover_of(live.id));
        let Some((player_id, unit)) = self.object(target) else {
            return Ok(());
        };
        let class = unit.master(self.master_tables(), player_id)?.class;
        let record = OrderRecord {
            order,
            action,
            time: self.world_time,
            position,
            target: Some(target),
            category: category(unit.life.master_id, class, &self.model.roles),
            target_position: unit.moved_vector(self.parts.mover_of(unit.id)),
        };
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player, idx)?.id)
        {
            ai.remember_order(record);
        }
        Ok(())
    }

    /// The action list's clearing (`FUN_140a98620`) of object `idx` of player `player`: each
    /// action's teardown ([`Unit::clear_own_actions`]), then what they leave to the world: a
    /// counted builder leaves its building (`FUN_140aadbf0` before its base teardown,
    /// [`World::leave_builders`]), then the targets go (slot 0x180, [`World::release`]). A
    /// building's production is a list of its own, which stays.
    ///
    /// [`Unit::clear_own_actions`]: crate::objects::unit::Unit::clear_own_actions
    pub(crate) fn clear_actions(&mut self, player: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let by = self.releaser(player, idx)?;
        let (mut releases, mut builder_leaves) = (Vec::new(), Vec::new());
        let top = self.top_speed(player, self.players.live(player, idx)?)?;
        let unit = self.players.live_mut(player, idx)?;
        unit.clear_own_actions(
            self.parts.actions.of_mut(unit.id)?,
            (self.parts.motion.layer_mut(unit.id), top),
            &mut releases,
            &mut builder_leaves,
            by,
        )?;
        for id in builder_leaves {
            self.leave_builders(Some(id))?;
        }
        for (id, by) in releases {
            self.release(id, by)?;
        }
        Ok(())
    }

    /// `TRANSFORM_OBJECT` (`RGE_Command::do_command` case 0x29): the object `id`, the player's
    /// (another player's is refused, the cheat flag `+0xb12` aside), of the trophy master
    /// (1649) takes one of the trophies 1650 to 1653 the player has (`FUN_141659d20`): slot 0x90
    /// with 0, the building's ([`World::transform_building`]).
    pub(crate) fn transform_object(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> GenieResult<()> {
        let Some(Place::Live(player_id, idx)) = self.place(id) else {
            return Ok(());
        };
        if player_id != player || self.master(player, master).is_err() {
            return Ok(());
        }
        let unit = &self.players.live(player_id, idx)?;
        if !self
            .model
            .roles
            .of_master(unit.life.master_id)
            .contains(MasterRoles::TRANSFORMED_TROPHY)
            || !TROPHIES.contains(&master.get())
        {
            return Ok(());
        }
        self.transform_building(player_id, idx, master, ActionsOnChange::Clear)
    }

    /// The player's gate toggle (`FUN_14162f8c0`, from its slots 0x340 and 0x338): a gate of its
    /// player's masters, without masters of its own ([`crate::world::Gate::other_masters`]),
    /// changes to its master's `transform_unit` (slot 0x90, [`World::transform_building`]). A
    /// gate that shut (the new master's height above 0) kills the units in it (slot 0xd8 on
    /// those its count finds, [`World::shut_in_gate`]) and takes the world time plus 1 as its
    /// last move time (`+0x870`).
    pub(crate) fn toggle_gate(&mut self, player: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        if self
            .parts
            .building_of(unit.id)
            .gate
            .is_some_and(|gate| gate.other_masters)
        {
            return Err(GenieError::unsupported(
                "a gate's toggle to a master of its own or its original owner's",
            ));
        }
        let Some(new) = unit
            .master(self.master_tables(), player)?
            .building
            .as_ref()
            .and_then(|building_master| building_master.transform_unit)
        else {
            return Ok(());
        };
        self.transform_building(player, idx, new, ActionsOnChange::Clear)?;
        let height = self.master(player, new)?.height;
        let now = self.world_time;
        if Distance::ZERO < height {
            let id = self.players.live(player, idx)?.id;
            let annexes: Vec<ObjectId> = self.objects().annexes(id).ids().collect();
            let (units, _) = gate_units(
                &mut self.spatial,
                id,
                &annexes,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                    &self.model.tuning,
                    self.treaty.as_ref(),
                ),
            )?;
            for object_id in units {
                self.shut_in_gate(object_id)?;
            }
            self.parts
                .mover_mut(self.players.live(player, idx)?.id)
                .ok_or(GenieError::unsupported(
                    "a shut gate without its moving part",
                ))?
                .last_move = now.after(Millis::new(1));
        }
        Ok(())
    }

    /// Slot 0xd8 of object `id` in a gate that shut (`FUN_14162f8c0`): from type 70 on
    /// (`FUN_141211340`) class 42 is spared, and livestock (class 58) while it holds some
    /// (`COMISS`, `JA`: a NaN load holds none); else, and below type 70, `FUN_14118f580`: an
    /// object asleep wakes to the end of its player's live list (slots 0x2d8, 0x2d0), one flagged
    /// to sleep drops the flag, and its hit points go to 0. Its place in its player's
    /// special-object list (`+0x30c` bit 0, `FUN_141677380`) is not held.
    pub(super) fn shut_in_gate(&mut self, id: ObjectId) -> GenieResult<()> {
        /// The first master type whose slot 0xd8 is `FUN_141211340` (`TRIBE_Combat_Object`).
        const TRIBE_COMBAT: MasterKind = MasterKind::COMBAT;

        let Some(place) = self.place(id) else {
            return Ok(());
        };
        let (place_player_id, place_unit) = (place.player(), self.players.at(place)?);
        let master = place_unit.master(self.master_tables(), place_player_id)?;
        let spared = TRIBE_COMBAT <= master.kind
            && (self.model.roles.of_class(master.class).contains(ClassRoles::ALWAYS_SPARED)
                // The class `FUN_140c89760` lets scout without the master's flag when a game setting allows it
                // (raw `0x3a`).
                // The town centre (master 109, `0x6109` raw), to which livestock's `MoveTo` goes as to a point
                // (`FUN_140aa0df0`).
                || (self.model.roles.of_class(master.class).contains(ClassRoles::LIVESTOCK) && Amount::ZERO < place_unit.life.amount));
        if spared {
            return Ok(());
        }
        let unit = match place {
            Place::Live(owner, list_index) => self.players.live_mut(owner, list_index)?,
            Place::Sleeping(player_id, slot) => {
                let woken = self.swap_remove_asleep(player_id, slot)?;
                let (id, master) = (woken.unit.id, woken.unit.life.master_id);
                let list_index = self.push_live(player_id, woken)?;
                // the wake's remove and add (slots 0x2d8, 0x2d0) file it last in the tracker
                self.track_relist(player_id, id, master)?;
                self.players.live_mut(player_id, list_index)?
            }
        };
        unit.life.goto_sleep = false;
        unit.life.hp = HitPoints::ZERO;
        Ok(())
    }
}
