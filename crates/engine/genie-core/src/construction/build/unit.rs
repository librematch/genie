//! The builder's part of its update: what the Build action reads as it starts (its
//! foundation, the builder and its unit AI's orders), the action's update on the Build copied
//! out of the action list, and the work, which runs on the world with the builder lent: the
//! foundation's work, its starts and its completion.

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

use crate::GenieError;
use crate::construction::build::{BuildAction, BuildReads, ClearLook, DROPPED_ORDERS};
use crate::gathering::hunt::DropOff;
use crate::map::spatial::Spatial;
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::lifecycle::death_parts::clear_look_of;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::PlayerId;
use crate::unit_ai::OrderKind;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::world::state_of;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::world::{DROP_SITE_MASTERS, World};

impl Unit {
    /// The Build's update ([`BuildAction::update`]) with what it reads as it starts
    /// ([`Unit::build_reads`]), on the Build copied out of the action list and put back after
    /// it; its code (the end's 1). Nothing without a Build.
    pub(crate) fn build_part(&mut self, c: &mut Step<'_>, me: Me) -> Result<HeadCode, GenieError> {
        let Some(mut build) = self.actions.build().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.build_reads(c, build)?;
        let built = build.update(c, self, me, &reads);
        if let Some(b) = self.actions.build_mut() {
            *b = build;
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
        c: &Step<'_>,
        others: OthersRef<'_>,
        action: BuildAction,
    ) -> Result<BTreeMap<ObjectId, ClearLook>, GenieError> {
        let mut looks = BTreeMap::new();
        if let Some(f) = Some(action)
            .filter(|b| matches!(b.state, BuildState::Work | BuildState::Wait))
            .and_then(|b| others.own(b.target))
            .filter(|u| u.construction.is_some_and(|k| k.desolid))
        {
            let (fx, fy) = (f.position().x(), f.position().y());
            let reach = crate::construction::build::CLEARING_REACH + 1.0;
            for (id, _) in c.w.spatial.objects_within(Rect::new(
                fx - reach,
                fy - reach,
                fx + reach,
                fy + reach,
            )) {
                let (owner, u) = if id == self.id {
                    (c.player, self)
                } else if let Some(found) = others.find(id) {
                    found
                } else {
                    continue;
                };
                let l = clear_look_of(u, c.w.masters.of(owner))?;
                let _ = looks.insert(id, l);
            }
        }
        Ok(looks)
    }

    /// What the Build's update reads of the world as it starts ([`BuildReads`]).
    pub(super) fn build_reads(
        &self,
        c: &Step<'_>,
        action: BuildAction,
    ) -> Result<BuildReads, GenieError> {
        let others = OthersRef::of(&c.w.players, c.player)?;
        let target = action.target;
        let foundation = others.own(target);
        let site = foundation
            .filter(|u| u.construction.is_some())
            .map(|u| crate::construction::FoundationId::new(u.id));
        let built = foundation.map(|u| u.life.master_id);
        let stack_made = foundation.and_then(|u| u.stack_made);
        let build_targets: Vec<(Option<ObjectId>, Option<ObjectState>)> =
            self.combat.ai.as_ref().map_or_else(Vec::new, |ai| {
                ai.orders
                    .iter()
                    .filter(|o| o.order == OrderKind::Build)
                    .map(|o| {
                        let state = others.own(o.target).map(|u| u.life.state);
                        (o.target, state)
                    })
                    .collect()
            });
        let (deposit, gone_orders) = if action.state == BuildState::Drop {
            (
                self.build_deposit(c, others, action.site)?,
                self.gone_drop_orders(&c.w.spatial),
            )
        } else {
            (None, Vec::new())
        };
        let master = self.master(&c.w.masters, c.player);
        // FUN_140aadc50 drops only a kind the player has (FUN_140efe5b0)
        let attributes = c.w.players.player(c.player)?.attributes.len();
        let has_kind = (self.life.attribute_type).is_some_and(|k| k.index() < attributes);
        Ok(BuildReads {
            site,
            built,
            stack_made,
            build_targets,
            deposit,
            gone_orders,
            looks: self.clearing_looks(c, others, action)?,
            class: master.as_ref().ok().map(|m| m.class),
            drop_site: drop_site_master(c, built)? && has_kind,
            by: self.releaser(c)?,
        })
    }

    /// State 8: what the drop-off reads of its drop site `site`: its state, whether it takes
    /// what the builder carries and the gap between their footprints; none without the site.
    fn build_deposit(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
        site: Option<DropSiteId>,
    ) -> Result<Option<(ObjectState, DropOff)>, GenieError> {
        let Some(s) = c.w.spatial.get(site).filter(|_| site.is_some()) else {
            return Ok(None);
        };
        let me_at =
            c.w.spatial
                .get_known(self.id)?
                .ok_or(GenieError::unsupported("a builder without a place"))?;
        let master = self.master(&c.w.masters, c.player)?;
        let actor = Actor {
            id: self.id,
            master,
            player: c.player,
            held: self.life.held(),
            attribute_95: c
                .w
                .players
                .player(c.player)?
                .attributes
                .get(95)
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: c.w.players.player(c.player)?.civ,
        };
        Ok(state_of(site.map(DropSiteId::id), others).map(|state| {
            let drop = DropOff {
                takes: crate::unit_ai::task_finder::drops_at(&c.w.drop_sites, s.master, &actor),
                gap_squared: footprint_gap_squared(
                    me_at.position,
                    me_at.radius,
                    s.position,
                    s.radius,
                ),
            };
            (state, drop)
        }))
    }

    /// State 8: the targets of the unit AI's orders that a drop-off drops, gone from the world
    /// or past state 5 as the player sees them.
    fn gone_drop_orders(&self, spatial: &Spatial) -> Vec<Option<ObjectId>> {
        let gone = |t: Option<ObjectId>| match spatial.get(t) {
            None => true,
            Some(_) => spatial
                .sight
                .as_ref()
                .and_then(|v| v.seer(t))
                .is_some_and(|s| ObjectState::Dead < s.state),
        };
        self.combat.ai.as_ref().map_or_else(Vec::new, |ai| {
            ai.orders
                .iter()
                .filter(|o| DROPPED_ORDERS.contains(&o.order))
                .map(|o| o.target)
                .filter(|&t| gone(t))
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
        c: &mut Step<'_>,
        builder: &mut Unit,
        f: impl FnOnce(&mut World, PlayerId, ListIndex) -> Result<R, GenieError>,
    ) -> Result<R, GenieError> {
        if let Some(b) = builder.actions.build_mut() {
            *b = *self;
        }
        let answer = c.lend_at(builder, f);
        if let Some(b) = builder.actions.build() {
            *self = *b;
        }
        answer
    }

    /// `FUN_1411cc430`'s `rate` seconds of work on the foundation, the builder lent to the
    /// world ([`World::work_building`]): its annexes' and its own construction's start with
    /// its owner's watch, and the completion of the work that reaches its build time
    /// (`set_state(2)`, [`World::complete_building`]), each where the routine makes it. True
    /// while its construction goes on (the routine's 0).
    ///
    /// # Errors
    ///
    /// What the work, the starts and the completion do not model.
    pub(super) fn work_on(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        rate: Seconds,
    ) -> Result<bool, GenieError> {
        let target = self
            .target
            .ok_or(crate::construction::build::world_side::NOT_HELD)?;
        self.lend(c, builder, |w, _, _| w.work_building(target, rate))
    }
}

/// `FUN_140fde180`: the foundation's master `built` is a drop site (the masters 562, 584, 621,
/// 68, 1808, 50, 1187 and 199, or one the drop-site table has for every civ or the builder's).
fn drop_site_master(c: &Step<'_>, built: Option<MasterId>) -> Result<bool, GenieError> {
    let Some(m) = built else {
        return Ok(false);
    };
    if DROP_SITE_MASTERS.contains(&m) {
        return Ok(true);
    }
    let civ = c.w.players.player(c.player)?.civ;
    Ok(c.w
        .drop_sites
        .iter()
        .any(|d| d.building == m && (d.civs.is_empty() || d.civs.contains(&civ))))
}
