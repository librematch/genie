//! What the unit AI's view asks of its unit: its picker, a helper's tests and a master switch.

use crate::GenieResult;
use crate::map::Position;
use crate::movement::SpeedReads;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{Master, MasterTables};
use crate::objects::release::Releaser;
use crate::objects::unit::Unit;
use crate::orders::ActionsOnChange;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{UnitView, WorldPicker};
use crate::unit_ai::{Seeker, can_attack};
use crate::world::layer::Roles;
use crate::world::lets::{release as lets_release, take as lets_take, torn_down as lets_torn_down};
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::PlayerSlotsExt;

/// A helper that `AskForHelp` tests ([`UnitView::helper_free`]): one of player `owner`'s units
/// with its action layer and its master.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Helper<'world> {
    /// The helper's player.
    pub(crate) owner: PlayerId,
    /// The helper.
    pub(crate) unit: &'world Unit,
    /// Its action layer.
    pub(crate) actions: &'world Action,
    /// Its master.
    pub(crate) master: &'world Master,
}

impl UnitView<'_, '_> {
    /// The world as `bestUnitToAttack` and the attack tests read it for the unit
    /// ([`WorldPicker`]), with `seeker`, its running unit AI's target `ai_target` and without a
    /// terrain restriction.
    pub(super) fn picker(
        &self,
        seeker: Option<Seeker>,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<WorldPicker<'_>> {
        Ok(WorldPicker {
            attacker: &self.attacker,
            seeker,
            me: self.unit.id,
            ai_target,
            player: self.player,
            others: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?,
            foreign: owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players(),
            every: MasterTables::new(
                &self.step.world.model.masters,
                &self.step.world.parts.own_master,
            ),
            parts: &self.step.world.parts,
            speeds: SpeedReads::of(
                &self.step.world.players,
                &self.step.world.index,
                &self.step.world.parts,
                &self.step.world.groups,
                &self.step.world.model,
            ),
            by_id: ObjectsRef::new(
                &self.step.world.players,
                &self.step.world.index,
                &self.step.world.model.masters,
                &self.step.world.parts,
                &self.step.world.model.roles,
                &self.step.world.model.tuning,
                self.step.world.treaty.as_ref(),
            ),
            restriction: None,
            unit: &*self.unit,
            actions: &*self.actions,
            attributes: &self.step.world.players.player(self.player)?.attributes,
            relations: &self.step.world.players.player(self.player)?.relations,
            ai_settings: self.ai_settings,
            now: self.step.world.world_time,
            mode_680: self.step.world.mode_680,
        })
    }

    /// The attack test of `may_attack` on a target the view holds: its class and place when
    /// `seeker` can attack it.
    pub(super) fn attack_check(
        &self,
        target: ObjectId,
        seeker: &Seeker,
        roles: &Roles,
    ) -> GenieResult<Option<(ClassId, Position)>> {
        let picker = self.picker(Some(*seeker), seeker.target)?;
        let (target, _) = picker.target(&self.step.world.spatial, target)?;
        Ok(can_attack(
            &self.attacker,
            seeker,
            &target,
            roles,
            &self.step.world.model.tuning.unit_ai,
        )?
        .then_some((target.class, target.position)))
    }

    /// Whether the unit's view of the world (live objects, owner-first) holds object `id`.
    pub(super) fn view_holds(&self, id: ObjectId) -> GenieResult<bool> {
        Ok(owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?
        .get(id)
        .is_some())
    }

    /// `AskForHelp`'s tests of a helper `helper` (of player `owner`, master `master`) that passed
    /// its class, owner and reach tests, against `attacker`, all reads (the game asks its attack
    /// check before its owner's test; neither changes what the other reads): a helper in no group
    /// helps only when idle (no action value, no action, no order, an empty queue and no target:
    /// "He has a `UnitAIaction`"), one in a group only when the group patrols (`FUN_141017c30` or
    /// `FUN_141017c40`); then its top action's target within its range (slot 400, slot 0x1e8: the
    /// footprints' squared gap between the least and the weapon range squared) keeps it, but a
    /// building whose master has no attack for a patrolling group's member; then its unit AI's
    /// attack check (slot 0xa0 with 0, [`crate::unit_ai::can_attack`]) on the attacker. True for a
    /// helper that answers. The asking unit's unit AI runs with the target `ai_target`.
    pub(crate) fn helper_free(
        &self,
        helper: Helper<'_>,
        attacker: ObjectId,
        ai_target: Option<ObjectId>,
        roles: &Roles,
    ) -> GenieResult<bool> {
        let Helper {
            owner,
            unit: helper,
            actions,
            master,
        } = helper;
        let others = owner_view(
            &self.step.world.players,
            &self.step.world.index,
            self.step.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| others.find(id);
        let Some(ai) = self.step.world.parts.unit_ai.get(helper.id) else {
            return Ok(false);
        };
        let patrolling = match helper
            .group
            .id
            .and_then(|id| self.step.world.groups.get(id))
        {
            None => {
                let busy = actions.top_action_kind().is_some()
                    || self.step.world.parts.unread.contains(helper.id);
                if ai.current_action.is_some()
                    || busy
                    || ai.current_order.is_some()
                    || !ai.orders.is_empty()
                    || ai.current_target.is_some()
                {
                    return Ok(false);
                }
                false
            }
            Some(group) if group.patrols() => true,
            Some(_) => return Ok(false),
        };
        if let Some((tq, unit)) = actions.top_target().and_then(find) {
            let tm = unit.master(self.step.world.master_tables(), tq)?;
            let gap =
                footprint_gap_squared(helper.position(), master.radius, unit.position(), tm.radius);
            let in_range = gap <= master.combat_part().weapon_range.squared()
                && master.combat_part().min_range.squared() <= gap;
            let spared = patrolling
                && tm.kind == MasterKind::BUILDING
                && tm.combat_part().weapons.is_empty();
            if in_range && !spared {
                return Ok(false);
            }
        }
        let player_kind = if owner == self.player {
            self.player_kind
        } else {
            owner_view(
                &self.step.world.players,
                &self.step.world.index,
                self.step.player,
                self.new_owner,
            )?
            .players()
            .player(owner)
            .map_or(PlayerKind::Phantom, |player| player.kind)
        };
        let capture = self.step.world.spatial.capture_of(helper.id);
        let me = helper.attacker_of(
            actions,
            ai.ai_kind,
            Some(master),
            (owner, player_kind),
            self.step.world.settings.game_mode,
            (
                capture,
                self.step.world.parts.combat_of(helper.id).firepower,
                self.step
                    .world
                    .top_speed_with(owner, helper, ai.current_target)?,
            ),
        );
        let target_master_kind = match ai.current_target.and_then(find) {
            Some((player_id, unit)) => Some(
                unit.master(self.step.world.master_tables(), player_id)?
                    .kind,
            ),
            None => None,
        };
        let picker = self.picker(None, ai_target)?;
        let (target, _) = picker.target(&self.step.world.spatial, attacker)?;
        can_attack(
            &me,
            &ai.seeker_with(target_master_kind),
            &target,
            roles,
            &self.step.world.model.tuning.unit_ai,
        )
    }
}

impl UnitView<'_, '_> {
    /// Slot 0x90 with 0 from `own` to master `next` (a task's switch, a villager's swap): the
    /// action list clears (the old actions' teardowns), then the master change at once, the unit
    /// lent back to its slot ([`World::change_master`](crate::world::World::change_master): the
    /// master's unit part, what it gives and takes of the owner's attributes and the tracker's
    /// promotion). The teardowns read the unit's top speed with the running unit AI's target
    /// `ai_target`.
    ///
    /// # Errors
    ///
    /// What the teardowns and the switch do not model.
    pub(crate) fn switch_unit_master(
        &mut self,
        own: &Master,
        next: MasterId,
        ai_target: Option<ObjectId>,
    ) -> GenieResult<()> {
        let by = Releaser {
            id: self.unit.id,
            class: Some(own.class),
            player: self.player,
        };
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        let top = self
            .step
            .world
            .top_speed_with(self.player, self.unit, ai_target)?;
        self.unit.tear_down_actions(
            (self.actions, self.step.world.parts.unread.get(self.unit.id)),
            (self.step.world.parts.motion.layer_mut(self.unit.id), top),
            &mut releases,
            &mut leaves,
            by,
        )?;
        self.torn_down((leaves, releases))?;
        self.step.lend_at(
            (&mut *self.unit, &mut *self.actions),
            |world, player_id, list_index| {
                world.change_master(player_id, list_index, next, ActionsOnChange::Clear)
            },
        )
    }
}

impl UnitView<'_, '_> {
    /// Slot 0x178 on object `id`, which the unit's action of `by` takes
    /// ([`crate::world::lets::take`]); the unit itself, out of its slot, is taken where it is.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::take`]'s.
    pub(crate) fn take(&mut self, id: ObjectId, by: Releaser, roles: &Roles) -> GenieResult<()> {
        lets_take(self.step, self.unit, id, by, roles)
    }

    /// Slot 0x180 on object `id`, which the unit's action of `by` lets go
    /// ([`crate::world::lets::release`]); the unit itself, out of its slot, is let go where it
    /// is, its AI told when the side's call returns.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::release`]'s.
    pub(crate) fn release(&mut self, id: ObjectId, by: Releaser) -> GenieResult<()> {
        lets_release(self.step, self.unit, id, by)
    }

    /// What the unit's action list's teardown left the world, at once
    /// ([`crate::world::lets::torn_down`]): the buildings its counted builders leave, then the
    /// targets it lets go.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::torn_down`]'s.
    pub(crate) fn torn_down(
        &mut self,
        (leaves, releases): (Vec<ObjectId>, Vec<(ObjectId, Releaser)>),
    ) -> GenieResult<()> {
        lets_torn_down(self.step, self.unit, (leaves, releases))
    }
}
