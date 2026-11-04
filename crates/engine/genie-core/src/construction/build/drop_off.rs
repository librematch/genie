//! State 8: a builder that carries something walks to a drop site first ([`DropSearch`]).

use crate::GenieError;
use crate::construction::build::BuildState;
use crate::construction::build::{BuildAction, BuildReads, DROP_WAIT, DROPPED_ORDERS};
use crate::gathering::DropSiteId;
use crate::gathering::hunt::resource_kind;
use crate::map::Distance;
use crate::math::fmath::below_or_unordered;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::TargetMoved;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::release::Releaser;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::orders::nearest_table_site;
use crate::players::Amount;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::TaskOn;
use crate::unit_ai::task_finder::world::task_target_with;
use crate::world::OBJECT_MASTER;
use crate::world::Seconds;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::lets::{release, take};
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use std::mem::replace;

/// What the drop-off's search gives `set_state(8)` ([`BuildAction::set_drop`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropSearch {
    /// The builder carries nothing (`+0x310` at most 0): no search.
    Empty,
    /// The drop-site table has no drop site for its load (`FUN_141658e20`).
    Nothing,
    /// The drop site the table gives, and the range and sprite of the `MoveTo` to it: those of
    /// the task the builder's slot 0x820 finds on it (any action type, no switch: its `+0x30`
    /// and its carrying sprite `+0x58`), else the Build task's.
    Site {
        /// The drop site.
        id: DropSiteId,
        /// The `MoveTo`'s range.
        range: Distance,
        /// The found task's carrying sprite, None without the task or the sprite.
        sprite: Option<SpriteId>,
    },
}

impl BuildAction {
    /// `set_state(8)` (`0x140aae200` case 8) from the update (case 7 searching again): the
    /// drop-site table's nearest drop site for what the unit carries
    /// ([`crate::orders::nearest_table_site`] over its player's objects; the sleeping ones,
    /// static only, are never drop sites) and the task its slot 0x820 finds on it
    /// ([`task_search`] with any action type, no switch: its range, else the Build task's),
    /// unless it carries nothing (COMISS: a NaN load neither); then [`BuildAction::set_drop`].
    ///
    /// # Errors
    ///
    /// A builder without its master, and what the search and `set_drop` do not model.
    ///
    /// [`task_search`]: crate::unit_ai::task_finder::world::task_search
    pub(crate) fn drop_again(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        by: Releaser,
        slots: Slots,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let search = if Amount::ZERO < builder.life.amount {
            Self::drop_search(c, builder, self.task.range, slots, roles)?
        } else {
            DropSearch::Empty
        };
        self.set_drop(c, builder, search, by, roles)
    }

    /// The drop site the drop-off walks to ([`BuildAction::drop_again`]), with `task_range`
    /// the Build task's range for none found on it.
    fn drop_search(
        c: &Step<'_>,
        builder: &Unit,
        task_range: Distance,
        slots: Slots,
        roles: &Roles,
    ) -> Result<DropSearch, GenieError> {
        let view = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        let masters =
            c.w.model
                .masters
                .get(c.player.index())
                .ok_or(OBJECT_MASTER)?;
        let own = builder.master_in(OwnerMasters::new(&c.w.parts.own_master, Some(masters)))?;
        let at = builder.position().ground();
        let look = |id: ObjectId| view.own(id).map(|u| (u.life.state, u.position()));
        let found = nearest_table_site(
            (
                &c.w.model.drop_sites,
                c.w.players.player(c.player)?.civ,
                &c.w.players.player(c.player)?.tracker,
            ),
            (at, builder.life.attribute_kind, own.class),
            look,
            &c.w.model.tuning.order,
        );
        let Some((id, _)) = found else {
            return Ok(DropSearch::Nothing);
        };
        let (player, foreign) = (c.player, view.players());
        let find = |t: ObjectId| {
            if t == builder.id {
                return Some((player, builder));
            }
            view.find(t)
        };
        let every = MasterTables::new(&c.w.model.masters, &c.w.parts.own_master);
        let task = match (
            task_target_with(id.id(), 1, &find, &c.w.parts, every)?,
            task_target_with(id.id(), 2, &find, &c.w.parts, every)?,
        ) {
            (Some(t), Some(general)) => {
                use crate::unit_ai::task_finder::world::SideWorld;
                use crate::unit_ai::task_finder::world::task_search;
                let actor = Actor {
                    id: builder.id,
                    master: own,
                    player,
                    held: builder.life.held(),
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
                let dip = SideWorld {
                    player,
                    relations: &c.w.players.player(c.player)?.relations,
                    foreign,
                    drop_sites: &c.w.model.drop_sites,
                };
                // slot 0x820's point: x, y and z −1, no terrain
                let on = TaskOn {
                    target: Some(&t),
                    general: Some(&general),
                    terrain: None,
                };
                task_search(masters, &actor, &on, None, &dip, slots, roles)?
            }
            _ => None,
        };
        let range = task.map_or(task_range, |(t, _)| t.work_range);
        let sprite = task.and_then(|(t, _)| t.sprites.carrying);
        Ok(DropSearch::Site { id, range, sprite })
    }

    /// `set_state(8)` (`0x140aae200` case 8) with what the search found (`search`), the builder
    /// `by` letting its targets go: the prelude ([`BuildAction::leave`]: out of the builders,
    /// the `MoveTo`'s teardown stops the unit), then a builder that carries nothing walks to its
    /// foundation (`set_state(4)`, [`BuildAction::restart`]). Else the drop-site table's nearest
    /// drop site for its load (`FUN_141658e20`) becomes the second target at once
    /// (`FUN_140a7a190`: the old one let go, slot 0x180, the new one taken, slot 0x178); with
    /// none the unit tells its unit AI (notify 0x203, the Build's action value) and walks to its
    /// foundation (`set_state(4)`); with one a quiet `MoveTo` (`+0x61` 1) to it within the
    /// search's range starts (`FUN_140aa0df0` with its point the site's place and the found
    /// task's carrying sprite, else the Build task's moving one; [`MoveToReads::to_object`]):
    /// arrived within 0.01 of its footprint (`FUN_140aa2100`), in state 13 for a unit without
    /// speed.
    ///
    /// [`MoveToReads::to_object`]: crate::movement::move_to::MoveToReads::to_object
    ///
    /// # Errors
    ///
    /// A builder without movement state, a drop site or foundation without a place.
    pub(crate) fn set_drop(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        search: DropSearch,
        by: Releaser,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        self.leave(c, builder)?;
        self.state = BuildState::Drop { move_to: None };
        let found = match search {
            DropSearch::Empty => return self.restart(c, builder, roles),
            DropSearch::Nothing => None,
            DropSearch::Site { id, range, sprite } => Some((id, range, sprite)),
        };
        let new = found.map(|(id, _, _)| id);
        if self.site != new {
            let old = replace(&mut self.site, new);
            release(c, builder, old, by)?;
            take(c, builder, new, by, roles)?;
        }
        let Some((id, range, sprite)) = found else {
            builder.post_own(
                &mut c.w.parts.unit_ai,
                NotifyKind::NO_DROP_SITE,
                [AiAction::Build.value(), 0, 0],
            )?;
            return self.restart(c, builder, roles);
        };
        let move_reads = builder.move_to_reads(
            c.w.parts.motion.mover(builder.id),
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.model.graphics,
                c.w.parts.combat_of(builder.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let to = move_reads
            .walk_to(c.w, Some(id.id()))?
            .ok_or(GenieError::unsupported("a drop site without a place"))?;
        let given = sprite.or(self.task.moving);
        self.state = BuildState::Drop {
            move_to: move_reads.to_object(
                to,
                range,
                given,
                TargetMoved::AsksAgain,
                &c.w.model.tuning.move_to,
            )?,
        };
        Ok(())
    }

    /// State 8's update (`0x140ab0370` case 7). While its timer is below 0 (COMISS: a NaN runs
    /// it out) it runs by dt; at its end the drop-off searches again (`set_state(8)`,
    /// [`BuildAction::drop_again`]). Else its quiet `MoveTo` walks to the drop site
    /// ([`BuildAction::step_walk`]): its target let go (code 5), a failed walk (codes 3 and 4)
    /// and a drop site gone or not in state 2 search again. At its end (codes 1 and 2; the
    /// `MoveTo` stays in the list until the next `set_state`) a drop site that does not take the
    /// load (`FUN_140fdd190`) halts the Build (`set_state(2)`); one beyond `BuildTuning::drop_reach`
    /// (`FUN_14118f8c0`) makes it wait 3 seconds with the unit's animation stopped
    /// (`FUN_141161850`). Else the load goes to the player (slot 0xf8 with 1 and 1, the resource
    /// of what it carries, `FUN_140fdd4f0`, [`BuildAction::drop_load`]; its sound, slot 0x4c0,
    /// and the statistics are not held), the unit carries nothing (`FUN_14119d310`) and the
    /// Build goes on ([`BuildAction::after_deposit`]).
    ///
    /// # Errors
    ///
    /// What the walk, the search and the states do not model.
    pub(super) fn update_drop(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        me: Me,
        reads: &BuildReads,
        roles: &Roles,
        slots: Slots,
    ) -> Result<HeadCode, GenieError> {
        if below_or_unordered(self.timer.get(), 0.0) {
            self.timer += c.dt.seconds();
            if self.timer < Seconds::ZERO {
                return Ok(HeadCode::Keeps);
            }
            self.timer = Seconds::ZERO;
            self.drop_again(c, builder, reads.by, slots, roles)?;
            return Ok(HeadCode::Keeps);
        }
        match self.step_walk(c, builder, me, reads, roles)?.get() {
            1 | 2 => {}
            3..=5 => {
                self.drop_again(c, builder, reads.by, slots, roles)?;
                return Ok(HeadCode::Keeps);
            }
            // 0 (it walks or waits) and any code above 5 (none): the update returns 0
            _ => return Ok(HeadCode::Keeps),
        }
        let Some((ObjectState::Alive, drop)) = reads.deposit.filter(|_| self.site.is_some()) else {
            self.drop_again(c, builder, reads.by, slots, roles)?;
            return Ok(HeadCode::Keeps);
        };
        if !drop.takes {
            self.halt_in(c, builder, reads)?;
            return Ok(HeadCode::Keeps);
        }
        if c.w.model.tuning.build.drop_reach < drop.gap_squared.get() {
            self.timer = DROP_WAIT;
            c.w.parts.set_anim_speed(builder.id, Speed::ZERO);
            return Ok(HeadCode::Keeps);
        }
        let attributes = c.w.players.player(c.player)?.attributes.len();
        let kind = resource_kind(builder.life.attribute_kind, attributes, slots);
        let amount = builder.life.amount;
        Self::drop_load(c, builder, kind, amount)?;
        self.after_deposit(c, builder, reads, roles)
    }

    /// State 8 after the load went to the player: a Build with a foundation lets the drop site
    /// go (`FUN_140a7a190` with none: its slot 0x180) and walks back to it (`set_state(4)`).
    /// Without one its unit AI drops its orders on targets gone (`FUN_140c5de60`); with orders
    /// left it has no order and no action and the Build is done (`set_state(1)`, the update's
    /// 1); with none it waits (`set_state(3)`). A unit without a unit AI and a point of −1 is
    /// done (a computer player's) or halts; with a point it walks there (`set_state(11)`).
    fn after_deposit(
        &mut self,
        c: &mut Step<'_>,
        builder: &mut Unit,
        reads: &BuildReads,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        if self.target.is_some() {
            let site = self.site.take();
            if site.is_some() {
                release(c, builder, site, reads.by)?;
            }
            self.restart(c, builder, roles)?;
            return Ok(HeadCode::Keeps);
        }
        if let Some(ai) = c.w.parts.unit_ai.get_mut(builder.id) {
            let gone = &reads.gone_orders;
            ai.orders
                .retain(|o| !(DROPPED_ORDERS.contains(&o.order) && gone.contains(&o.target)));
            if !ai.orders.is_empty() {
                ai.end_order_and_action()?;
                // 1: the action is done.
                self.end(c, builder, BuildState::Done)?;
                return Ok(HeadCode::Deletes);
            }
            self.idle(c, builder, reads)?;
            return Ok(HeadCode::Keeps);
        }
        // UCOMISS against −1 (DAT_14330ec20): a NaN is not
        if self.point.x() == -1.0 || self.point.y() == -1.0 {
            if c.w.players.player(c.player)?.kind.is_computer() {
                self.end(c, builder, BuildState::Done)?;
            } else {
                self.halt_in(c, builder, reads)?;
            }
            return Ok(HeadCode::Keeps);
        }
        self.walk_to_point(c, builder, roles)?;
        Ok(HeadCode::Keeps)
    }
}
