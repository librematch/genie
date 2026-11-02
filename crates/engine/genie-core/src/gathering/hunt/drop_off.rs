//! State 8: the drop-off at the drop site; the target let go, the teardown and a case 6
//! without a target.

use crate::GenieError;
use crate::combat::attack::set_walker_anim;
use crate::gathering::DropSiteId;
use crate::gathering::gather::Walks;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::walks::QUIET_CODE;
use crate::gathering::hunt::{
    DropOff, HuntAction, Lead, PointWalker, asked_drop_off, resource_kind,
};
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::movement::move_to::GiveUp;
use crate::movement::move_to::WalkTo;
use crate::movement::{Me, Mover};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::attribute::AttributeId;
use crate::unit_ai::world_view::DropWorld;
use crate::world::Seconds;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// What a drop-off's update did ([`HuntAction::update_drop`],
/// [`crate::gathering::gather::GatherAction::update_drop`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dropped {
    /// It walks or waits.
    Going,
    /// The load went to the player: its resource and amount, which the caller adds.
    Load(AttributeId, Amount),
    /// Its wait ended: it tries again (`set_state(8)`, the caller's).
    Again,
    /// The drop site does not take the load: the action idles (`set_state(2)`; the caller lets
    /// the targets go).
    Idle,
}

/// The wait of a hunt's drop-off that cannot go on (`0xc0400000`).
pub(super) const DROP_WAIT: Seconds = Seconds::new(-3.0);
/// A hunt's drop-off reaches its drop site within this squared footprint gap (`DAT_14330eb8c`).
pub(super) const DROP_REACH: f32 = crate::math::image::hunt::DROP_REACH;

impl HuntAction {
    /// `set_state(8)` (`0x140ac60e0` case 8) with a load and its drop site (`carry_to`, at `site`,
    /// `gap` from the unit; without one the drop site search `FUN_1411671e0` would find it, not
    /// modelled): the sub-actions go and a quiet `MoveTo` to the drop site within the task's
    /// range starts ([`HuntAction::move_to_object`]).
    pub(crate) fn set_drop(
        &mut self,
        site: Position,
        gap: Distance,
        mover: &mut Mover,
        walks: &Walks<'_>,
    ) -> Result<(), GenieError> {
        self.tear_down(Some(mover))?;
        self.state = HuntState::Drop { move_to: None };
        if self.carry_to.is_none() {
            return Err(GenieError::unsupported(
                "a drop-off without its drop site (its search)",
            ));
        }
        let range = self.task.range;
        let to = WalkTo {
            id: self.carry_to.map(DropSiteId::id),
            at: site,
            gap,
            state: None,
        };
        let move_to = self.move_to_object(to, range, mover, walks)?;
        self.state = HuntState::Drop { move_to };
        Ok(())
    }

    /// State 8's update (`0x140ac7a00` case 7) of `unit`'s hunt on the step, the hunt copied out
    /// of its list: while its timer is below 0 the hunt waits (the timer runs by dt); at its end
    /// the drop site goes (`FUN_140a7a190` with none) and the hunt tries again (`set_state(8)`,
    /// [`Dropped::Again`], the caller's). Else the `MoveTo` to the drop site walks
    /// ([`HuntAction::update_list`]: the units in its way go to the unit AI at once), waits
    /// after the motion step gave up (a human player's villager once, `who`) or fails; a failed
    /// one or one whose target was let go (codes 3 to 5) makes the hunt wait 3 seconds and stops the unit's animation (`FUN_141161850`). Once it
    /// ended (its update in state 1 returns 1: the list deletes it and its destructor stops the
    /// unit) the load goes to the player at a drop site that takes it within a tile (`site`; one
    /// that does not, `set_state` 2, is not modelled; a farther one makes the hunt wait 3 seconds
    /// too): the resource of what the unit carries (`FUN_140fdd4f0`: food for 15 to 17, nothing
    /// outside 0 to 3, 9 and 56, then 0 beyond the player's attributes, `FUN_140efe5b0`), which
    /// the caller adds; the unit carries nothing after (`FUN_14119d310` with -1). A hunt with a
    /// target walks back to it (`set_state` 4), one without searches for a new one (the
    /// caller's).
    ///
    /// # Errors
    ///
    /// What the drop-off does not model, and the walk's.
    pub(crate) fn update_drop(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        site: Option<DropOff>,
        who: PointWalker,
    ) -> Result<Dropped, GenieError> {
        if self.timer < Seconds::ZERO {
            self.timer += c.dt.seconds();
            if self.timer < Seconds::ZERO {
                return Ok(Dropped::Going);
            }
            self.timer = Seconds::ZERO;
            return Ok(Dropped::Again);
        }
        let gives_up = GiveUp {
            waits: !who.player_type.is_computer() && who.villager,
            stance: who.stance,
        };
        match self.update_list(c, unit, me, gives_up)?.get() {
            0 => return Ok(Dropped::Going),
            // failed or its target let go: the hunt waits and the unit's animation stops
            3..=5 => {
                self.timer = DROP_WAIT;
                set_walker_anim(c.w.parts.motion_mut(unit.id), Speed::new(0.0));
                return Ok(Dropped::Going);
            }
            1 | 2 => self.tear_down(c.w.parts.mover_mut(unit.id))?,
            code => return Err(QUIET_CODE.with("code", code)),
        }
        let site = site.ok_or(GenieError::unsupported(
            "a drop-off whose drop site is gone",
        ))?;
        if !site.takes {
            return Err(GenieError::unsupported(
                "a drop site that does not take the load (set_state 2)",
            ));
        }
        if DROP_REACH < site.gap_squared.get() {
            self.timer = DROP_WAIT;
            set_walker_anim(c.w.parts.motion_mut(unit.id), Speed::new(0.0));
            return Ok(Dropped::Going);
        }
        let attributes = c.w.players.player(c.player)?.attributes.len();
        let kind = resource_kind(unit.life.attribute_type, attributes);
        let amount = unit.life.amount;
        unit.life.attribute_type = None;
        unit.life.amount = Amount::ZERO;
        Ok(Dropped::Load(kind, amount))
    }

    /// Slot 0x78 (`FUN_140a78060`, the base action's; `UNIT_ORDER`'s drop-off) of a unit carrying
    /// some of `held_kind` (`+0x310` above 0, which the caller checked): [`asked_drop_off`] on the
    /// task's resource.
    #[must_use]
    pub fn drop_off_asked(&self, held_kind: Option<AttributeId>, attributes: usize) -> bool {
        asked_drop_off(self.task.resource, held_kind, attributes)
    }

    /// Slot 0x68 with none (`FUN_140ac7980`, [`HuntAction::set_target`]): the target goes; the
    /// old target, which the caller releases (slot 0x180).
    pub const fn let_go(&mut self) -> Option<crate::gathering::HuntTargetId> {
        let target = self.target;
        self.set_target(None);
        target
    }

    /// Slot 0x68 (`FUN_140ac7980`): the base's change of target (`FUN_140a7a230`, whose release
    /// and take are the caller's), then the lead clears ([`Lead::NEW`]: `moved_sheep` and
    /// `sheep_still_time` 0, the start −1).
    pub const fn set_target(&mut self, target: Option<crate::gathering::HuntTargetId>) {
        self.target = target;
        self.lead = Lead::NEW;
    }

    /// The teardown (`FUN_140a73fc0`): the sub-actions go; the caller releases the target (slot
    /// 0x180).
    pub fn tear_down(&mut self, mut mover: Option<&mut Mover>) -> Result<(), GenieError> {
        if let Some(mut a) = self.state.take_attack() {
            a.tear_down(mover.as_deref_mut())?;
        }
        if self.state.take_move_to().is_some()
            && let Some(m) = mover
        {
            m.stop()?;
        }
        Ok(())
    }

    /// Case 6 without a target (`0x140ac7a00` at `0x140ac9029`): a load is carried off
    /// (`set_state(8)`, true, the caller's); with none, or one that is not a number (`COMISS`
    /// with `JBE`), the hunt searches (`set_state(3)`: [`HuntState::Search`] here, the rest the caller's).
    pub(super) fn ends_without_target(&mut self, held: Amount) -> bool {
        if crate::math::fmath::at_most_or_unordered(held.get(), 0.0) {
            self.state = HuntState::Search;
            false
        } else {
            true
        }
    }
}

impl Unit {
    /// The hunt's `set_state(8)` after its gathering (`0x140ac60e0` case 8): a unit that carries
    /// nothing goes back to state 3 (not modelled). Without a drop site, its master's drop sites
    /// (`+0x7a0`; none when the first is −1) are searched among its player's objects, from the
    /// unit by path ([`drop_site_search`] with the unit), else by distance from its place (without
    /// it); none found is not modelled (notify 0x203, `set_state` 2). The drop site found is the
    /// second target (`FUN_140a7a190`, which takes it: slot 0x178 after the update) and a quiet
    /// `MoveTo` to it starts ([`HuntAction::set_drop`]).
    ///
    /// [`HuntAction::set_drop`]: crate::gathering::hunt::HuntAction::set_drop
    /// [`drop_site_search`]: crate::gathering::drop_sites::drop_site_search
    pub(crate) fn hunt_drop_off(
        &mut self,
        actions: &mut crate::objects::layers::Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let (mover, by_id) =
            c.w.parts
                .lend_mover(self.id, (&c.w.players, &c.w.index, &c.w.masters));
        let w = DropWorld {
            masters: by_id.masters().of(c.player),
            spatial: &mut c.w.spatial,
            zones: &mut c.w.zones,
            pathing: &mut c.w.pathing,
            object: c.w.streams.get(Stream::Object),
            world_time: c.w.world_time,
            player_type: c.w.players.player(c.player)?.kind,
            groups: &c.w.groups,
        };
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        if let Some(site) = self.drop_off_in(actions, mover, w, others, &c.w.graphics, by_id)? {
            let by = self.releaser(c)?;
            crate::world::lets::take(c, self, site, by)?;
        }
        Ok(())
    }
}
