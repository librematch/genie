//! A gatherer's walk after a drop and the drop sites it walks to.

use crate::GenieError;
use crate::gathering::ActionEnd;
use crate::gathering::DropSiteId;
use crate::gathering::drop_sites::DropMeasure;
use crate::gathering::drop_sites::SitePlace;
use crate::gathering::drop_sites::SiteSearch;
use crate::gathering::drop_sites::{Seeker, drop_site_search};
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{GatherAction, GatherRetarget, Walks};
use crate::gathering::gather::{Resource, StopRate, productivity_of, waits_for_farm};
use crate::gathering::hunt::HuntAction;
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::movement::move_to::SpriteReads;
use crate::movement::{Me, Mover, Planning, footprint_gap};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ActionKind;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::release::Releaser;
use crate::objects::sprite::{Graphic, new_sprite};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::{Graphics, WorkerCount};
use crate::pathing::zones::UNKNOWN_RESTRICTION;
use crate::players::Amount;
use crate::unit_ai::OrderKind;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::civilian_kind;
use crate::unit_ai::world_view::DropWorld;
use crate::world::Seconds;
use crate::world::index::ObjectsRef;
use crate::world::layer::Roles;
use crate::world::lets::{release, take};
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The Gather's state 8 after the deposit without a target (`0x140a8e1d0` case 7; its drop
    /// site stays): the unit AI drops the queued orders on targets gone (`FUN_140c5de60`) and,
    /// with orders left (`FUN_140c564a0`), ends its order and action (`FUN_140c64b60`,
    /// `FUN_140c64b50`) and the Gather is done (`set_state(1)`: its update returns 1, the list
    /// removes it). Else the gather group of its target class (the AI's `+0x58`, else the
    /// Gather's `mTargetType`; slot 0x1b0, the civilian's) names the unit AI's
    /// `ClosestResourceObject` (slot 0x80: no drop site, any workers, state 2, no point), and the
    /// Gather takes what it finds (slot 0x48, [`crate::gathering::gather::GatherAction::retarget`]). Without
    /// one a Gather with its point would walk there to search again (`set_state(11)`, not
    /// modelled, or 0xf3 for a farmer); without a point a computer player's Gather is done, a
    /// human's idles (`set_state(2)`). `gather` is the head as the drop left it.
    pub(crate) fn gather_after_drop(
        &mut self,
        gather: GatherAction,
        actions: &mut Action,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let by = self.releaser(c)?;
        if self.after_drop_orders(c)? == ActionEnd::Ends {
            return Ok(HeadCode::Deletes);
        }
        let Some(found) = self.after_drop_search(&gather, actions, c)? else {
            return self.after_drop_nothing((gather, actions), c, by, roles);
        };
        self.after_drop_retarget(actions, c, found, by, roles)
            .map(|()| HeadCode::Keeps)
    }

    /// The unit AI's queued orders after the drop: the orders on targets gone (a target the world
    /// does not hold, or one its sight sees past state 5) leave (`FUN_140c5de60`); with orders
    /// left the AI ends its order and action and the Gather is done (its deletion lets the drop
    /// site go). [`ActionEnd::Ends`] when it is.
    fn after_drop_orders(&self, c: &mut Step<'_>) -> Result<ActionEnd, GenieError> {
        let spatial: &Spatial = &c.w.spatial;
        let gone = |id: Option<ObjectId>| match spatial.get(id) {
            None => true,
            Some(_) => spatial
                .sight
                .as_ref()
                .and_then(|v| v.seer(id))
                .is_some_and(|s| ObjectState::Dead < s.state),
        };
        let Some(ai) = c.w.parts.unit_ai.get_mut(self.id) else {
            return Ok(ActionEnd::GoesOn);
        };
        ai.orders.retain(|o| {
            !(matches!(
                o.order,
                OrderKind::Gather | OrderKind::Hunt | OrderKind::OnTarget
            ) && gone(o.target))
        });
        if ai.orders.is_empty() {
            return Ok(ActionEnd::GoesOn);
        }
        ai.end_order_and_action()?;
        Ok(ActionEnd::Ends)
    }

    /// The unit AI's `ClosestResourceObject` after the drop (slot 0x80: no drop site, any
    /// workers, state 2, no point) for the gather group (slot 0x1b0, the civilian's) of its target
    /// class (the AI's `+0x58`, else the Gather's `mTargetType`); none for none, a class of no
    /// group, or a unit without a unit AI.
    fn after_drop_search(
        &self,
        gather: &GatherAction,
        actions: &Action,
        c: &mut Step<'_>,
    ) -> Result<Option<ObjectId>, GenieError> {
        let Some(ai) = c.w.parts.unit_ai.get(self.id) else {
            return Ok(None);
        };
        let class = ai.target_class.or(gather.target_class);
        if ai.ai_kind != UnitAiKind::Civilian {
            return Err(GenieError::unsupported(
                "slot 0x1b0 of another unit AI than the civilian's",
            ));
        }
        let Some(kind) = class.and_then(|class| civilian_kind(class, &c.w.model.roles)) else {
            return Ok(None);
        };
        self.resource_search(actions, c, kind, WorkerCount::NO_CAP)
    }

    /// Nothing found after the drop: a Gather with its point walks there to search again
    /// (`set_state(11)` within the task's range; a farmer's 0xf3 is not modelled); without one
    /// (`LAB_140a8ed55`) a computer player's Gather is done (its code; its deletion lets the drop
    /// site go), a human's idles (`set_state(2)`: its targets and point cleared, its sprite
    /// [`Unit::idle_sprite`]'s) and lets its drop site go.
    fn after_drop_nothing(
        &mut self,
        (mut gather, actions): (GatherAction, &mut Action),
        c: &mut Step<'_>,
        by: Releaser,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let farm_waits = waits_for_farm(self.life.master_id, gather.target_class, roles);
        let walks = self.move_walks(
            c.w.parts.mover_of(self.id),
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.model.graphics,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        // the search and the reads reach no action of the unit's: the copy is the head until it
        // goes back, before an error too
        if gather.point.x() != -1.0 && gather.point.y() != -1.0 {
            if farm_waits {
                return Err(GenieError::unsupported(
                    "a farmer that found nothing after its drop-off (set_state 0xf3)",
                ));
            }
            // set_state(11) without a target: to the point within the task's range
            let mover =
                c.w.parts
                    .mover_mut(self.id)
                    .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
            let walked = gather.walk_back(Distance::ZERO, mover, &walks, &c.w.model.tuning.move_to);
            actions.list.set_gather(Some(gather));
            return walked.map(|()| HeadCode::Keeps);
        }
        if c.w.players.player(c.player)?.kind.is_computer() {
            return Ok(HeadCode::Deletes);
        }
        let carry_to = gather.carry_to;
        let shows = self.idle_sprite(&gather, c)?;
        let torn = gather.tear_down(c.w.parts.mover_mut(self.id));
        if torn.is_ok() {
            gather.state = GatherState::Idle;
            gather.timer = Seconds::ZERO;
            gather.target = None;
            gather.carry_to = None;
            gather.point = Position::from([-1.0; 3]);
        }
        actions.list.set_gather(Some(gather));
        torn?;
        if let Some(g) = shows {
            new_sprite(
                &mut self.sprite,
                c.w.parts.move_stats_mut(self.id),
                g,
                c.w.streams.get(Stream::Sprite),
                c.w.model.tuning.sprite,
            );
        }
        if carry_to.is_some() {
            release(c, self, carry_to, by)?;
        }
        Ok(HeadCode::Keeps)
    }

    /// The sprite an idle gatherer shows: its task's carrying sprite while it holds at least 1 of
    /// the task's resource, else its master's standing sprite (none the data file lacks).
    fn idle_sprite(
        &self,
        gather: &GatherAction,
        c: &Step<'_>,
    ) -> Result<Option<Graphic>, GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        let standing = master
            .standing_sprite
            .and_then(|s| c.w.model.graphics.get(s).copied());
        let carrying = gather
            .task
            .sprites
            .carrying
            .and_then(|s| c.w.model.graphics.get(s).copied());
        Ok(match carrying {
            Some(g)
                if gather.task.resource.is_some()
                    && self.life.attribute_kind == gather.task.resource
                    && 1 <= self.life.amount.get() as i32 =>
            {
                Some(g)
            }
            _ => standing,
        })
    }

    /// The Gather takes what the search found (slot 0x48,
    /// [`crate::gathering::gather::GatherAction::retarget`]) with its stop rate; each target it
    /// changed is let go and the new one taken (one of its own player's directly, never listed).
    fn after_drop_retarget(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        found: ObjectId,
        by: Releaser,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let (owner, resource) =
            self.found_resource(c, OthersRef::of(&c.w.players, &c.w.index, c.player)?, found)?;
        let master = self.master(c.w.master_tables(), c.player)?;
        let (player_kind, farm_mode) = (c.w.players.player(c.player)?.kind, c.w.settings.farm_mode);
        let walks = self.move_walks(
            c.w.parts.mover_of(self.id),
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.model.graphics,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let rate = StopRate {
            work_rate: c.w.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity: productivity_of(
                actions.list.gather().and_then(|g| g.task.productivity),
                &c.w.players.player(c.player)?.attributes,
                &c.w.model.tuning.gather,
            ),
        };
        let (Some(gather), Some(mover)) = (actions.list.gather_mut(), c.w.parts.mover_mut(self.id))
        else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = gather.targets();
        let rng = c.w.streams.get(Stream::Action);
        gather.retarget(
            GatherRetarget {
                target: &resource,
                player: c.player,
                player_kind,
                walks,
                farm_mode,
                rate,
            },
            mover,
            rng,
            roles,
            &c.w.model.tuning,
        )?;
        let new = gather.targets();
        for (old, new) in old.into_iter().zip(new) {
            if old == new {
                continue;
            }
            if old.is_some() {
                release(c, self, old, by)?;
            }
            if new.is_none() {
                continue;
            }
            if owner == c.player.index() && new == Some(found) {
                // one of its own player's: allied, never listed
                let _ = c.w.players.player(c.player)?;
                let u =
                    c.w.live_object_mut(c.player, found)
                        .ok_or(GenieError::unreachable(
                            "an object found above its player lost",
                        ))?;
                u.taken(None, by.id);
            } else {
                let by = self.releaser(c)?;
                take(c, self, new, by, roles)?;
            }
        }
        Ok(())
    }

    /// What the Gather's retarget reads of the object `found` (its own player's, or another's
    /// the slice holds): its owner, place, class, master, workers and radii, whether it is one of
    /// the gatherer's drop sites, and the footprint gap to it.
    fn found_resource(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
        found: ObjectId,
    ) -> Result<(usize, Resource), GenieError> {
        let (owner, t) = others.find(found).ok_or(GenieError::unsupported(
            "a gather's new target the slice does not hold",
        ))?;
        let master = self.master(c.w.master_tables(), c.player)?;
        let target_master = t.master(c.w.master_tables(), owner)?;
        Ok((
            owner.index(),
            Resource {
                id: found,
                position: t.position(),
                class: target_master.class,
                master: target_master.id,
                player: owner,
                workers: t.workers,
                drop_site: master.action_part().drop_sites.contains(&t.life.master_id),
                gap: footprint_gap(
                    self.position(),
                    master.radius,
                    t.position(),
                    target_master.radius,
                ),
                state: t.life.state,
                radius: target_master.radius,
            },
        ))
    }

    /// The Gather's `set_state(8)` (`0x140a8cd40` case 8): the current stop clears; a gatherer
    /// carrying nothing would search (`set_state` 3, not modelled); else its drop site
    /// ([`Unit::drop_site`]) becomes its second target (`FUN_140a7a190`: slot 0x178) and a quiet
    /// `MoveTo` walks there ([`crate::gathering::gather::GatherAction::set_drop`]). Without one the unit AI's
    /// part (notify 0x203) is not modelled.
    pub(crate) fn gather_drop_off(
        &mut self,
        gather: &mut GatherAction,
        actions: &mut Action,
        c: &mut Step<'_>,
        roles: &Roles,
    ) -> Result<(), GenieError> {
        let (mover, by_id) =
            c.w.parts
                .lend_mover(self.id, (&c.w.players, &c.w.index, &c.w.model));
        let w = DropWorld {
            masters: by_id.masters().of(c.player),
            spatial: &mut c.w.spatial,
            zones: &mut c.w.zones,
            pathing: &mut c.w.pathing,
            object: c.w.streams.get(Stream::Object),
            world_time: c.w.world_time,
            player_kind: c.w.players.player(c.player)?.kind,
            groups: &c.w.groups,
        };
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        if let Some(site) = self.gather_drop_off_in(
            (gather, actions),
            mover,
            w,
            others,
            &c.w.model.graphics,
            by_id,
        )? {
            let by = self.releaser(c)?;
            take(c, self, site, by, roles)?;
        }
        Ok(())
    }

    /// [`Unit::gather_drop_off`] in `w` with the unit's `mover`, from the update or a command,
    /// with the sprites' table `graphics`: the drop site it took when it is a new second target (the caller's to take,
    /// slot 0x178). `gather` is the head's copy, which the list holds; it goes back after the
    /// walk's start, before an error too.
    pub(crate) fn gather_drop_off_in(
        &self,
        (gather, actions): (&mut GatherAction, &mut Action),
        mover: Option<&mut Mover>,
        mut c: DropWorld<'_>,
        others: OthersRef<'_>,
        graphics: &Graphics,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<ObjectId>, GenieError> {
        let move_to_tuning = &by_id.tuning().move_to;
        let roles = by_id.roles();
        if self.life.amount <= Amount::ZERO {
            return Err(GenieError::unsupported(
                "a gather's drop-off carrying nothing (set_state 3)",
            ));
        }
        let (carry_to, state) = (gather.carry_to, gather.state);
        let (site, gap) = self.drop_site(
            (actions, mover.as_deref()),
            &mut c,
            others,
            carry_to,
            ActionState::new(state.get()),
            by_id,
        )?;
        let at = c
            .spatial
            .get_known(site)?
            .map_or(Position::ORIGIN, |s| s.position);
        let sprites = SpriteReads::of(
            self.master_in(c.masters)?,
            by_id.combat_of(self.id).abilities.look.as_ref(),
            graphics,
            roles,
        );
        let walks = Walks::of(sprites, (self, mover.as_deref()));
        let Some(mover) = mover else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        gather.carry_to = site;
        let walked = gather.set_drop(at, gap, mover, &walks, move_to_tuning);
        actions.list.set_gather(Some(*gather));
        walked?;
        Ok(site.filter(|_| site != carry_to).map(DropSiteId::id))
    }

    /// [`Unit::hunt_drop_off`] in `w` with the unit's `mover`, from the update or a command,
    /// with the sprites' table `graphics`: the drop site it took when it is a new second target
    /// (`FUN_140a7a190`, the caller's to take, slot 0x178). `hunt` is the head's copy, which the
    /// list holds; it goes back after the walk's start, before an error too.
    pub(crate) fn drop_off_in(
        &self,
        (hunt, actions): (&mut HuntAction, &mut Action),
        mover: Option<&mut Mover>,
        mut c: DropWorld<'_>,
        others: OthersRef<'_>,
        graphics: &Graphics,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<ObjectId>, GenieError> {
        let move_to_tuning = &by_id.tuning().move_to;
        let roles = by_id.roles();
        if self.life.amount <= Amount::ZERO {
            return Err(GenieError::unsupported(
                "a hunt's drop-off carrying nothing (set_state 3)",
            ));
        }
        let (carry_to, state) = (hunt.carry_to, hunt.state);
        let (site, gap) = self.drop_site(
            (actions, mover.as_deref()),
            &mut c,
            others,
            carry_to,
            ActionState::new(state.get()),
            by_id,
        )?;
        if site.is_none() {
            return Err(GenieError::unsupported(
                "a hunt without a drop site (notify 0x203, set_state 2)",
            ));
        }
        let at = c
            .spatial
            .get_known(site)?
            .map_or(Position::ORIGIN, |s| s.position);
        let sprites = SpriteReads::of(
            self.master_in(c.masters)?,
            by_id.combat_of(self.id).abilities.look.as_ref(),
            graphics,
            roles,
        );
        let walks = Walks::of(sprites, (self, mover.as_deref()));
        let Some(mover) = mover else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        hunt.carry_to = site;
        let walked = hunt.set_drop(at, gap, mover, &walks, move_to_tuning);
        actions.list.set_hunt(Some(*hunt));
        walked?;
        Ok(site.filter(|_| site != carry_to).map(DropSiteId::id))
    }

    /// The drop site a gatherer's (with its mover `mover`) `set_state(8)` walks to, its second
    /// target `carry_to` or, with
    /// none and a master with drop sites (`+0x7a0`), the closest of them (`FUN_1411671e0`: first
    /// one it reaches, [`Unit::drop_site_from`] outside other objects, then any); −1 for none.
    /// With the footprint gap to it (`FUN_14118f680`).
    pub(crate) fn drop_site(
        &self,
        (actions, mover): (&Action, Option<&Mover>),
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        carry_to: Option<DropSiteId>,
        state: ActionState,
        by_id: ObjectsRef<'_>,
    ) -> Result<(Option<DropSiteId>, Distance), GenieError> {
        let master = self.master_in(c.masters)?;
        let mover = mover.ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let mut site = carry_to;
        if site.is_none() && !master.action_part().drop_sites.is_empty() {
            let masters = c.masters.table.ok_or(GenieError::unsupported(
                "a drop-off of a player without masters",
            ))?;
            let sites = &master.action_part().drop_sites;
            site = match self.drop_site_from(
                (actions, Some(mover)),
                c,
                others,
                state,
                SitePlace::Outside,
                by_id,
            )? {
                Some(s) => Some(s),
                None => drop_site_search(
                    others.own_iter(),
                    OwnerMasters::new(c.masters.own, Some(masters)),
                    sites,
                    self.position(),
                    None,
                    (SitePlace::Outside, Some(ActionKind::UNPACK)),
                    by_id,
                )?,
            };
        }
        let Some(site) = site else {
            return Ok((None, Distance::ZERO));
        };
        let ground = mover.stats.radius.ground();
        let s = c
            .spatial
            .get_known(site)?
            .ok_or(GenieError::unsupported("a drop site without a place"))?;
        Ok((
            Some(site),
            footprint_gap(self.position(), ground, s.position, s.radius),
        ))
    }

    /// `FUN_1411671e0` from the unit with its `mover` (`state` its action's, which the planner
    /// reads) over its master's drop sites (`+0x7a0`): [`drop_site_search`] with the unit as the
    /// seeker, by path for a human player (its argument 9, `FUN_141659cf0` false), skipping objects
    /// inside others when `place` is [`SitePlace::Outside`] (its argument 10) and those at work on
    /// `0x7d` or `0x7c` (its argument 8).
    pub(crate) fn drop_site_from(
        &self,
        (actions, mover): (&Action, Option<&Mover>),
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        state: ActionState,
        place: SitePlace,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<DropSiteId>, GenieError> {
        let nearest = if c.player_kind.is_computer() {
            DropMeasure::ByDistance
        } else {
            DropMeasure::ByPath
        };
        self.drop_site_with(
            (actions, mover),
            c,
            others,
            state,
            SiteSearch {
                place,
                nearest,
                at_work: Some(ActionKind::UNPACK),
            },
            by_id,
        )
    }

    /// [`Unit::drop_site_from`] with the search's arguments 10, 9 and 8 (`search`).
    pub(crate) fn drop_site_with(
        &self,
        (actions, mover): (&Action, Option<&Mover>),
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        state: ActionState,
        search: SiteSearch,
        by_id: ObjectsRef<'_>,
    ) -> Result<Option<DropSiteId>, GenieError> {
        let masters = c.masters.table.ok_or(GenieError::unsupported(
            "a drop-off of a player without masters",
        ))?;
        let master = self.master_in(c.masters)?;
        let mover = mover.ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let terrain = c.spatial.terrain.as_ref().ok_or(GenieError::unsupported(
            "the map's zones without its terrain",
        ))?;
        let values = terrain
            .restriction(mover.stats.restriction)
            .ok_or(UNKNOWN_RESTRICTION)?;
        let zones = c.zones.of(c.spatial.map, &terrain.kinds, values)?;
        let me = Me {
            action_state: state,
            ..self.me(actions)
        };
        let mut plan = Planning {
            pathing: &mut *c.pathing,
            now: c.world_time,
            player_kind: c.player_kind,
            object: &mut *c.object,
            groups: c.groups,
        };
        let seeker = Seeker {
            unit: self,
            actions,
            unread: by_id.unread(self.id),
            me,
            mover,
            plan: &mut plan,
            spatial: c.spatial,
            zones,
            class: master.class,
            nearest: search.nearest,
        };
        drop_site_search(
            others.own_iter(),
            OwnerMasters::new(c.masters.own, Some(masters)),
            &master.action_part().drop_sites,
            self.position(),
            Some(seeker),
            (search.place, search.at_work),
            by_id,
        )
    }
}
