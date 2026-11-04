//! The builder's part of its update: what the Build action reads as it starts (its
//! foundation, the builder and its unit AI's orders), the action's update on the Build copied
//! out of the action list, and the work, which runs on the world with the builder lent: the
//! foundation's work, its starts and its completion.

use crate::construction::BuildProgress;
use crate::construction::build::BuildState;
use crate::gathering::DropSiteId;
use crate::map::Rect;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::players::Amount;
use crate::world::Seconds;
use std::collections::BTreeMap;

use crate::construction::FoundationId;
use crate::construction::build::world_side::NOT_HELD;
use crate::construction::build::{BuildAction, BuildReads, ClearLook, DROPPED_ORDERS};
use crate::gathering::hunt::DropOff;
use crate::map::spatial::Spatial;
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::layers::Action;
use crate::objects::lifecycle::death_parts::clear_look_of;
use crate::objects::masters::OwnerMasters;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAi;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::drops_at;
use crate::unit_ai::task_finder::world::state_of;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The Build's update ([`BuildAction::update`]) with what it reads as it starts
    /// ([`Unit::build_reads`]), on the Build copied out of the action list and put back after
    /// it; its code (the end's 1). Nothing without a Build.
    pub(crate) fn build_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
    ) -> GenieResult<HeadCode> {
        let Some(mut build) = actions.list.build().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.build_reads(step, build)?;
        let built = build.update(step, self, actions, me, &reads);
        if let Some(build_action) = actions.list.build_mut() {
            *build_action = build;
        }
        // its end's 1 deletes it; the destructor (FUN_140aadbf0) finds it out of the builders
        // (set_state left them) and lets go the foundation and the second target it still holds
        built
    }

    /// What the footprint clearing of the Build's `set_state(7)` reads (`FUN_1415c3620`) of the
    /// objects on the tiles about a desolid foundation, as the update starts: none for a builder
    /// in another state or at another foundation.
    fn clearing_looks(
        &self,
        step: &Step<'_>,
        others: OthersRef<'_>,
        action: BuildAction,
    ) -> GenieResult<BTreeMap<ObjectId, ClearLook>> {
        let mut looks = BTreeMap::new();
        if let Some(action_unit) = Some(action)
            .filter(|b| matches!(b.state, BuildState::Work | BuildState::Wait))
            .and_then(|build_action| others.own(build_action.target))
            .filter(|u| {
                step.w
                    .parts
                    .building_of(u.id)
                    .construction
                    .is_some_and(|construction| construction.desolid)
            })
        {
            let (fx, fy) = (action_unit.position().x(), action_unit.position().y());
            let reach = step.w.model.tuning.build.clearing_reach + 1.0;
            for (id, _) in step.w.spatial.objects_within(Rect::new(
                fx - reach,
                fy - reach,
                fx + reach,
                fy + reach,
            )) {
                let (owner, unit) = if id == self.id {
                    (step.player, self)
                } else if let Some(found) = others.find(id) {
                    found
                } else {
                    continue;
                };
                let clear_look = clear_look_of(
                    unit,
                    step.w.parts.building_of(id),
                    OwnerMasters::new(&step.w.parts.own_master, step.w.model.masters.of(owner)),
                )?;
                let _ = looks.insert(id, clear_look);
            }
        }
        Ok(looks)
    }

    /// What the Build's update reads of the world as it starts ([`BuildReads`]).
    pub(super) fn build_reads(
        &self,
        step: &Step<'_>,
        action: BuildAction,
    ) -> GenieResult<BuildReads> {
        let others = OthersRef::of(&step.w.players, &step.w.index, step.player)?;
        let target = action.target;
        let foundation = others.own(target);
        let site = foundation
            .filter(|u| step.w.parts.building_of(u.id).construction.is_some())
            .map(|u| FoundationId::new(u.id));
        let built = foundation.map(|u| u.life.master_id);
        let stack_made = foundation.and_then(|u| step.w.parts.building_of(u.id).stack_made);
        let build_targets: Vec<(Option<ObjectId>, Option<ObjectState>)> = step
            .w
            .parts
            .unit_ai
            .get(self.id)
            .map_or_else(Vec::new, |ai| {
                ai.orders
                    .iter()
                    .filter(|o| o.order == OrderKind::Build)
                    .map(|queued_order| {
                        let state = others.own(queued_order.target).map(|unit| unit.life.state);
                        (queued_order.target, state)
                    })
                    .collect()
            });
        let (deposit, gone_orders) = if matches!(action.state, BuildState::Drop { .. }) {
            (
                self.build_deposit(step, others, action.site)?,
                Self::gone_drop_orders(&step.w.spatial, step.w.parts.unit_ai.get(self.id)),
            )
        } else {
            (None, Vec::new())
        };
        let master = self.master(step.w.master_tables(), step.player);
        // FUN_140aadc50 drops only a kind the player has (FUN_140efe5b0)
        let attributes = step.w.players.player(step.player)?.attributes.len();
        let has_kind = (self.life.attribute_kind)
            .is_some_and(|attribute_id| attribute_id.index() < attributes);
        Ok(BuildReads {
            site,
            stack_made,
            build_targets,
            deposit,
            gone_orders,
            looks: self.clearing_looks(step, others, action)?,
            class: master.as_ref().ok().map(|m| m.class),
            drop_site: drop_site_master(step, built)? && has_kind,
            by: self.releaser(step)?,
        })
    }

    /// State 8: what the drop-off reads of its drop site `site`: its state, whether it takes
    /// what the builder carries and the gap between their footprints; none without the site.
    fn build_deposit(
        &self,
        step: &Step<'_>,
        others: OthersRef<'_>,
        site: Option<DropSiteId>,
    ) -> GenieResult<Option<(ObjectState, DropOff)>> {
        let Some(placed) = step.w.spatial.get(site).filter(|_| site.is_some()) else {
            return Ok(None);
        };
        let me_at = step
            .w
            .spatial
            .get_known(self.id)?
            .ok_or(GenieError::unsupported("a builder without a place"))?;
        let master = self.master(step.w.master_tables(), step.player)?;
        let actor = Actor {
            id: self.id,
            master,
            player: step.player,
            held: self.life.held(),
            attribute_95: step
                .w
                .players
                .player(step.player)?
                .attributes
                .get(95)
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: step.w.players.player(step.player)?.civ,
        };
        Ok(state_of(site.map(DropSiteId::id), others).map(|state| {
            let drop = DropOff {
                takes: drops_at(&step.w.model.drop_sites, placed.master, &actor),
                gap_squared: footprint_gap_squared(
                    me_at.position,
                    me_at.radius,
                    placed.position,
                    placed.radius,
                ),
            };
            (state, drop)
        }))
    }

    /// State 8: the targets of the unit AI `ai`'s orders that a drop-off drops, gone from the
    /// world or past state 5 as the player sees them.
    fn gone_drop_orders(spatial: &Spatial, ai: Option<&UnitAi>) -> Vec<Option<ObjectId>> {
        let gone = |t: Option<ObjectId>| match spatial.get(t) {
            None => true,
            Some(_) => spatial
                .sight
                .as_ref()
                .and_then(|visibility| visibility.seer(t))
                .is_some_and(|seer| ObjectState::Dead < seer.state),
        };
        ai.map_or_else(Vec::new, |ai| {
            ai.orders
                .iter()
                .filter(|o| DROPPED_ORDERS.contains(&o.order))
                .map(|queued_order| queued_order.target)
                .filter(|&object_id| gone(object_id))
                .collect()
        })
    }
}

impl BuildAction {
    /// Runs `f` on the world with the builder lent back to its slot ([`Step::lend_at`]: the
    /// world's routines that scan the objects meet it, as the game's do; `f` gets its player and
    /// slot), its Build as this update left it so far; after `f` the Build is the one `f` left
    /// (none when `f` replaced the builder's actions: a work command).
    ///
    /// # Errors
    ///
    /// `f`'s and the lend's.
    pub(super) fn lend<R>(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        f: impl FnOnce(&mut World, PlayerId, ListIndex) -> GenieResult<R>,
    ) -> GenieResult<R> {
        if let Some(b) = actions.list.build_mut() {
            *b = *self;
        }
        let answer = step.lend_at((&mut *builder, &mut *actions), f);
        if let Some(build_action) = actions.list.build() {
            *self = *build_action;
        }
        answer
    }

    /// `FUN_1411cc430`'s `rate` seconds of work on the foundation, the builder lent to the
    /// world ([`World::work_building`]): its annexes' and its own construction's start with
    /// its owner's watch, and the completion of the work that reaches its build time
    /// (`set_state(2)`, [`World::complete_building`]), each where the routine makes it.
    /// [`BuildProgress::Ongoing`] while its construction goes on (the routine's 0).
    ///
    /// # Errors
    ///
    /// What the work, the starts and the completion do not model.
    pub(super) fn work_on(
        &mut self,
        step: &mut Step<'_>,
        builder: &mut Unit,
        actions: &mut Action,
        rate: Seconds,
    ) -> GenieResult<BuildProgress> {
        let target = self.target.ok_or(NOT_HELD)?;
        self.lend(step, builder, actions, |world, _, _| {
            world.work_building(target, rate)
        })
    }
}

/// `FUN_140fde180`: the foundation's master `built` is a drop site (the masters 562, 584, 621,
/// 68, 1808, 50, 1187 and 199, or one the drop-site table has for every civ or the builder's).
fn drop_site_master(step: &Step<'_>, built: Option<MasterId>) -> GenieResult<bool> {
    let Some(master_id) = built else {
        return Ok(false);
    };
    if step
        .w
        .model
        .roles
        .of_master(master_id)
        .contains(MasterRoles::DROP_SITE)
    {
        return Ok(true);
    }
    let civ = step.w.players.player(step.player)?.civ;
    Ok(step.w.model.drop_sites.iter().any(|drop_site| {
        drop_site.building == master_id
            && (drop_site.civs.is_empty() || drop_site.civs.contains(&civ))
    }))
}
