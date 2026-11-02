//! What the unit AI's view asks of its unit: its picker, a helper's tests and a master switch.

use crate::GenieError;
use crate::map::Position;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::release::Releaser;
use crate::objects::unit::Unit;
use crate::ownership::CaptureKind;
use crate::players::PlayerId;
use crate::unit_ai::world_view::owner_view;
use crate::unit_ai::world_view::{UnitView, WorldPicker};
use crate::world::slots::PlayerSlots;

impl UnitView<'_, '_> {
    /// The world as `bestUnitToAttack` and the attack tests read it for the unit
    /// ([`WorldPicker`]), with `seeker` and without a terrain restriction.
    pub(super) fn picker(
        &self,
        seeker: Option<crate::unit_ai::Seeker>,
    ) -> Result<WorldPicker<'_>, GenieError> {
        Ok(WorldPicker {
            attacker: &self.attacker,
            seeker,
            me: self.unit.id,
            player: self.player,
            others: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?,
            foreign: owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .players(),
            every: crate::objects::masters::MasterTables::new(
                &self.c.w.masters,
                &self.c.w.parts.own_master,
            ),
            parts: &self.c.w.parts,
            by_id: crate::world::index::ObjectsRef::new(
                &self.c.w.players,
                &self.c.w.index,
                &self.c.w.masters,
                &self.c.w.parts,
            ),
            restriction: None,
            unit: &*self.unit,
            attributes: &self.c.w.players.player(self.player)?.attributes,
            relations: &self.c.w.players.player(self.player)?.relations,
            ai_settings: self.ai_settings,
            now: self.c.w.world_time,
            mode_680: self.c.w.mode_680,
        })
    }

    /// The attack test of `may_attack` on a target the view holds: its class and place when
    /// `seeker` can attack it.
    pub(super) fn attack_check(
        &self,
        target: ObjectId,
        seeker: &crate::unit_ai::Seeker,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        let picker = self.picker(Some(*seeker))?;
        let (t, _) = picker.target(&self.c.w.spatial, target)?;
        Ok(
            crate::unit_ai::can_attack(&self.attacker, seeker, &t)?
                .then_some((t.class, t.position)),
        )
    }

    /// Whether the unit's view of the world (live objects, owner-first) holds object `id`.
    pub(super) fn view_holds(&self, id: ObjectId) -> Result<bool, GenieError> {
        Ok(owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?
        .get(id)
        .is_some())
    }

    /// `AskForHelp`'s tests of a helper `u` (of player `owner`, master `m`) that passed its
    /// class, owner and reach tests, against `attacker`, all reads (the game asks its attack
    /// check before its owner's test; neither changes what the other reads): a helper in no group
    /// helps only when idle (no action value, no action, no order, an empty queue and no target:
    /// "He has a `UnitAIaction`"), one in a group only when the group patrols (`FUN_141017c30` or
    /// `FUN_141017c40`); then its top action's target within its range (slot 400, slot 0x1e8:
    /// the footprints' squared gap between the least and the weapon range squared) keeps it, but
    /// a building whose master has no attack for a patrolling group's member; then its unit AI's
    /// attack check (slot 0xa0 with 0, [`crate::unit_ai::can_attack`]) on the attacker. True for
    /// a helper that answers.
    pub(crate) fn helper_free(
        &self,
        owner: PlayerId,
        u: &Unit,
        m: &crate::objects::masters::Master,
        attacker: ObjectId,
    ) -> Result<bool, GenieError> {
        let others = owner_view(
            &self.c.w.players,
            &self.c.w.index,
            self.c.player,
            self.new_owner,
        )?;
        let find = |id: ObjectId| others.find(id);
        let Some(ai) = self.c.w.parts.unit_ai.get(u.id) else {
            return Ok(false);
        };
        let patrolling = match u.group.id.and_then(|id| self.c.w.groups.get(id)) {
            None => {
                let busy = u.top_action_kind().is_some() || u.actions.unmodelled_action.is_some();
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
            Some(g) if g.patrols() => true,
            Some(_) => return Ok(false),
        };
        if let Some((tq, t)) = u.top_target().and_then(find) {
            let tm = t.master(self.c.w.master_tables(), tq)?;
            let gap = footprint_gap_squared(u.position(), m.radius, t.position(), tm.radius);
            let in_range = gap <= m.weapon_range.squared() && m.min_range.squared() <= gap;
            let spared = patrolling && tm.kind == BUILDING && tm.weapons.is_empty();
            if in_range && !spared {
                return Ok(false);
            }
        }
        let computer = if owner == self.player {
            self.computer
        } else {
            owner_view(
                &self.c.w.players,
                &self.c.w.index,
                self.c.player,
                self.new_owner,
            )?
            .players()
            .player(owner)
            .is_some_and(|q| q.computer)
        };
        let capture = self
            .c
            .w
            .spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(u.id))
            .map_or(CaptureKind::NONE, |s| s.capture);
        let me = u.attacker_of(
            ai.ai_type,
            Some(m),
            (owner, computer),
            self.attacker.scenario,
            (capture, self.c.w.parts.combat_of(u.id).firepower),
        );
        let target_kind = match ai.current_target.and_then(find) {
            Some((q, o)) => Some(o.master(self.c.w.master_tables(), q)?.kind),
            None => None,
        };
        let picker = self.picker(None)?;
        let (target, _) = picker.target(&self.c.w.spatial, attacker)?;
        crate::unit_ai::can_attack(&me, &ai.seeker_with(target_kind), &target)
    }
}

impl UnitView<'_, '_> {
    /// Slot 0x90 with 0 from `own` to master `next` (a task's switch, a villager's swap): the
    /// action list clears (the old actions' teardowns), then the master change at once, the
    /// unit lent back to its slot ([`World::change_master`]: the master's unit part, what it
    /// gives and takes of the owner's attributes and the tracker's promotion).
    ///
    /// # Errors
    ///
    /// What the teardowns and the switch do not model.
    pub(crate) fn switch_unit_master(
        &mut self,
        own: &crate::objects::masters::Master,
        next: MasterId,
    ) -> Result<(), GenieError> {
        let by = Releaser {
            id: self.unit.id,
            class: Some(own.class),
            player: self.player,
        };
        let (mut releases, mut leaves) = (Vec::new(), Vec::new());
        self.unit
            .tear_down_actions(&mut releases, &mut leaves, by)?;
        self.torn_down((leaves, releases))?;
        self.c
            .lend_at(self.unit, |w, p, i| w.change_master(p, i, next))
    }
}

impl UnitView<'_, '_> {
    /// Slot 0x178 on object `id`, which the unit's action of `by` takes
    /// ([`crate::world::lets::take`]); the unit itself, out of its slot, is taken where it is.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::take`]'s.
    pub(crate) fn take(&mut self, id: ObjectId, by: Releaser) -> Result<(), GenieError> {
        crate::world::lets::take(self.c, self.unit, id, by)
    }

    /// Slot 0x180 on object `id`, which the unit's action of `by` lets go
    /// ([`crate::world::lets::release`]); the unit itself, out of its slot, is let go where it
    /// is, its AI told when the side's call returns.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::release`]'s.
    pub(crate) fn release(&mut self, id: ObjectId, by: Releaser) -> Result<(), GenieError> {
        crate::world::lets::release(self.c, self.unit, id, by)
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
    ) -> Result<(), GenieError> {
        crate::world::lets::torn_down(self.c, self.unit, (leaves, releases))
    }
}
