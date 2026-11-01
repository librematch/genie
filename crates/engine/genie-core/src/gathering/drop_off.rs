//! A gatherer's walk after a drop and the drop sites it walks to.

use crate::GenieError;
use crate::gathering::DropSiteId;
use crate::gathering::drop_sites::{Seeker, drop_site_search};
use crate::gathering::gather::GatherState;
use crate::gathering::gather::{GatherRetarget, Walks};
use crate::map::Distance;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::movement::move_to::SpriteReads;
use crate::movement::{Me, Planning, footprint_gap};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionType;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::action_type;
use crate::objects::release::Releaser;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::unit_ai::OrderKind;
use crate::unit_ai::world_view::DropWorld;
use crate::world::Seconds;
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
    /// human's idles (`set_state(2)`).
    pub(crate) fn gather_after_drop(&mut self, c: &mut Step<'_>) -> Result<HeadCode, GenieError> {
        let by = self.releaser(c)?;
        if self.after_drop_orders(c)? {
            return Ok(HeadCode::Deletes);
        }
        let Some(found) = self.after_drop_search(c)? else {
            return self.after_drop_nothing(c, by);
        };
        self.after_drop_retarget(c, found, by)
            .map(|()| HeadCode::Keeps)
    }

    /// The unit AI's queued orders after the drop: the orders on targets gone (a target the world
    /// does not hold, or one its sight sees past state 5) leave (`FUN_140c5de60`); with orders
    /// left the AI ends its order and action and the Gather is done (its deletion lets the drop
    /// site go). True when it is.
    fn after_drop_orders(&mut self, c: &Step<'_>) -> Result<bool, GenieError> {
        let spatial: &Spatial = &c.w.spatial;
        let gone = |id: Option<ObjectId>| match spatial.get(id) {
            None => true,
            Some(_) => spatial
                .sight
                .as_ref()
                .and_then(|v| v.seer(id))
                .is_some_and(|s| ObjectState::Dead < s.state),
        };
        let Some(ai) = self.combat.ai.as_mut() else {
            return Ok(false);
        };
        ai.orders.retain(|o| {
            !(matches!(
                o.order,
                OrderKind::Gather | OrderKind::Hunt | OrderKind::OnTarget
            ) && gone(o.target))
        });
        if ai.orders.is_empty() {
            return Ok(false);
        }
        ai.end_order_and_action()?;
        Ok(true)
    }

    /// The unit AI's `ClosestResourceObject` after the drop (slot 0x80: no drop site, any
    /// workers, state 2, no point) for the gather group (slot 0x1b0, the civilian's) of its target
    /// class (the AI's `+0x58`, else the Gather's `mTargetType`); none for none, a class of no
    /// group, or a unit without a unit AI.
    fn after_drop_search(&self, c: &mut Step<'_>) -> Result<Option<ObjectId>, GenieError> {
        let gather = self
            .actions
            .list
            .gather()
            .ok_or(crate::gathering::gather::unit::NO_GATHER)?;
        let Some(ai) = self.combat.ai.as_ref() else {
            return Ok(None);
        };
        let class = ai.target_class.or(gather.target_class);
        if ai.ai_type != crate::unit_ai::CIVILIAN {
            return Err(GenieError::unsupported(
                "slot 0x1b0 of another unit AI than the civilian's",
            ));
        }
        let Some(kind) = class.and_then(crate::unit_ai::civilian_kind) else {
            return Ok(None);
        };
        self.resource_search(c, kind, crate::objects::WorkerCount::NO_CAP)
    }

    /// Nothing found after the drop: a Gather with its point walks there to search again
    /// (`set_state(11)` within the task's range; a farmer's 0xf3 is not modelled); without one
    /// (`LAB_140a8ed55`) a computer player's Gather is done (its code; its deletion lets the drop
    /// site go), a human's idles (`set_state(2)`: its targets and point cleared, its sprite
    /// [`Unit::idle_sprite`]'s) and lets its drop site go.
    fn after_drop_nothing(
        &mut self,
        c: &mut Step<'_>,
        by: Releaser,
    ) -> Result<HeadCode, GenieError> {
        let farm_waits = crate::gathering::gather::waits_for_a_farm(
            self.life.master_id,
            self.actions.list.gather().and_then(|g| g.target_class),
        );
        let walks = self.move_walks(&c.w.masters, c.player, &c.w.graphics)?;
        let gather = self
            .actions
            .list
            .gather_mut()
            .ok_or(crate::gathering::gather::unit::NO_GATHER)?;
        if gather.point.x() != -1.0 && gather.point.y() != -1.0 {
            if farm_waits {
                return Err(GenieError::unsupported(
                    "a farmer that found nothing after its drop-off (set_state 0xf3)",
                ));
            }
            // set_state(11) without a target: to the point within the task's range
            let mover = self
                .body
                .mover_mut()
                .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
            return gather
                .walk_back(Distance::ZERO, mover, &walks)
                .map(|()| HeadCode::Keeps);
        }
        if c.w.players.player(c.player)?.computer {
            return Ok(HeadCode::Deletes);
        }
        let carry_to = gather.carry_to;
        let shows = self.idle_sprite(c)?;
        let gather = self
            .actions
            .list
            .gather_mut()
            .ok_or(crate::gathering::gather::unit::NO_GATHER)?;
        gather.tear_down(self.body.mover_mut())?;
        gather.state = GatherState::Idle;
        gather.timer = Seconds::ZERO;
        gather.target = None;
        gather.carry_to = None;
        gather.point = Position::from([-1.0; 3]);
        if let Some(g) = shows {
            crate::objects::sprite::new_sprite(
                &mut self.sprite,
                self.body.move_stats_mut(),
                g,
                c.w.streams.get(Stream::Sprite),
            );
        }
        if carry_to.is_some() {
            crate::world::lets::release(c, self, carry_to, by)?;
        }
        Ok(HeadCode::Keeps)
    }

    /// The sprite an idle gatherer shows: its task's carrying sprite while it holds at least 1 of
    /// the task's resource, else its master's standing sprite (none the data file lacks).
    fn idle_sprite(
        &self,
        c: &Step<'_>,
    ) -> Result<Option<crate::objects::sprite::Graphic>, GenieError> {
        let gather = self
            .actions
            .list
            .gather()
            .ok_or(crate::gathering::gather::unit::NO_GATHER)?;
        let master = self.master(&c.w.masters, c.player)?;
        let standing = master
            .standing_sprite
            .and_then(|s| c.w.graphics.get(s).copied());
        let carrying = gather
            .task
            .sprites
            .carrying
            .and_then(|s| c.w.graphics.get(s).copied());
        Ok(match carrying {
            Some(g)
                if gather.task.resource.is_some()
                    && self.life.attribute_type == gather.task.resource
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
        c: &mut Step<'_>,
        found: ObjectId,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let (owner, resource) =
            self.found_resource(c, OthersRef::of(&c.w.players, c.player)?, found)?;
        let master = self.master(&c.w.masters, c.player)?;
        let (computer, farm_stops) = (
            c.w.players.player(c.player)?.computer,
            c.w.settings.walkable_farms,
        );
        let walks = self.move_walks(&c.w.masters, c.player, &c.w.graphics)?;
        let rate = crate::gathering::gather::StopRate {
            work_rate: self.combat.work_rate_powerup + master.work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity: crate::gathering::gather::productivity_of(
                self.actions.list.gather().and_then(|g| g.task.productivity),
                &c.w.players.player(c.player)?.attributes,
            ),
        };
        let (Some(gather), Some(mover)) = (self.actions.list.gather_mut(), self.body.mover_mut())
        else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        let old = [gather.target, gather.carry_to.map(DropSiteId::id)];
        let rng = c.w.streams.get(Stream::Action);
        let _ = gather.retarget(
            GatherRetarget {
                target: &resource,
                player: c.player,
                computer,
                walks,
                farm_stops,
                rate,
            },
            mover,
            rng,
        )?;
        let new = [gather.target, gather.carry_to.map(DropSiteId::id)];
        for (old, new) in old.into_iter().zip(new) {
            if old == new {
                continue;
            }
            if old.is_some() {
                crate::world::lets::release(c, self, old, by)?;
            }
            if new.is_none() {
                continue;
            }
            if owner == c.player.index() && new == Some(found) {
                // one of its own player's: allied, never listed
                let u =
                    c.w.players
                        .player_mut(c.player)?
                        .objects
                        .by_id_mut(found)
                        .ok_or(GenieError::unreachable(
                            "an object found above its player lost",
                        ))?;
                u.taken(false, by.id);
            } else {
                let by = self.releaser(c)?;
                crate::world::lets::take(c, self, new, by)?;
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
    ) -> Result<(usize, crate::gathering::gather::Resource), GenieError> {
        let (owner, t) = others.find(found).ok_or(GenieError::unsupported(
            "a gather's new target the slice does not hold",
        ))?;
        let master = self.master(&c.w.masters, c.player)?;
        let t_master = t.master(&c.w.masters, owner)?;
        Ok((
            owner.index(),
            crate::gathering::gather::Resource {
                id: found,
                position: t.position(),
                class: t_master.class,
                master: t_master.id,
                player: owner,
                workers: t.workers,
                drop_site: master.drop_sites.contains(&t.life.master_id),
                gap: footprint_gap(
                    self.position(),
                    master.radius,
                    t.position(),
                    t_master.radius,
                ),
                state: t.life.state,
                radius: t_master.radius,
            },
        ))
    }

    /// The Gather's `set_state(8)` (`0x140a8cd40` case 8): the current stop clears; a gatherer
    /// carrying nothing would search (`set_state` 3, not modelled); else its drop site
    /// ([`Unit::drop_site`]) becomes its second target (`FUN_140a7a190`: slot 0x178) and a quiet
    /// `MoveTo` walks there ([`crate::gathering::gather::GatherAction::set_drop`]). Without one the unit AI's
    /// part (notify 0x203) is not modelled.
    pub(crate) fn gather_drop_off(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let w = DropWorld {
            masters: c.w.masters.of(c.player),
            spatial: &mut c.w.spatial,
            zones: &mut c.w.zones,
            pathing: &mut c.w.pathing,
            object: c.w.streams.get(Stream::Object),
            world_time: c.w.world_time,
            computer: c.w.players.player(c.player)?.computer,
            groups: &c.w.groups,
        };
        let others = OthersRef::of(&c.w.players, c.player)?;
        if let Some(site) = self.gather_drop_off_in(w, others, &c.w.graphics)? {
            let by = self.releaser(c)?;
            crate::world::lets::take(c, self, site, by)?;
        }
        Ok(())
    }

    /// [`Unit::gather_drop_off`] in `w`, from the update or a command, with the sprites' table
    /// `graphics`: the drop site it took when it is a new second target (the caller's to take,
    /// slot 0x178).
    pub(crate) fn gather_drop_off_in(
        &mut self,
        mut c: DropWorld<'_>,
        others: OthersRef<'_>,
        graphics: &crate::objects::Graphics,
    ) -> Result<Option<ObjectId>, GenieError> {
        if self.life.amount <= Amount::ZERO {
            return Err(GenieError::unsupported(
                "a gather's drop-off carrying nothing (set_state 3)",
            ));
        }
        let gather = self
            .actions
            .list
            .gather()
            .ok_or(crate::gathering::gather::unit::NO_GATHER)?;
        let (carry_to, state) = (gather.carry_to, gather.state);
        let (site, gap) =
            self.drop_site(&mut c, others, carry_to, ActionState::new(state.get()))?;
        let at = c
            .spatial
            .get_known(site)?
            .map_or(Position::ORIGIN, |s| s.position);
        let sprites = SpriteReads::of(
            self.master_in(c.masters)?,
            self.combat.abilities.look.as_ref(),
            graphics,
        );
        let walks = Walks::of(sprites, self);
        let (Some(gather), Some(mover)) = (self.actions.list.gather_mut(), self.body.mover_mut())
        else {
            return Err(GenieError::unsupported(
                "a gatherer without its gather or movement state",
            ));
        };
        gather.carry_to = site;
        gather.set_drop(at, gap, mover, &walks)?;
        Ok(site.filter(|_| site != carry_to).map(DropSiteId::id))
    }

    /// [`Unit::hunt_drop_off`] in `w`, from the update or a command, with the sprites' table
    /// `graphics`: the drop site it took when it is a new second target (`FUN_140a7a190`, the caller's to take, slot 0x178).
    pub(crate) fn drop_off_in(
        &mut self,
        mut c: DropWorld<'_>,
        others: OthersRef<'_>,
        graphics: &crate::objects::Graphics,
    ) -> Result<Option<ObjectId>, GenieError> {
        if self.life.amount <= Amount::ZERO {
            return Err(GenieError::unsupported(
                "a hunt's drop-off carrying nothing (set_state 3)",
            ));
        }
        let hunt = self
            .actions
            .list
            .hunt()
            .ok_or(crate::gathering::hunt::NO_HUNT)?;
        let (carry_to, state) = (hunt.carry_to, hunt.state);
        let (site, gap) =
            self.drop_site(&mut c, others, carry_to, ActionState::new(state.get()))?;
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
            self.combat.abilities.look.as_ref(),
            graphics,
        );
        let walks = Walks::of(sprites, self);
        let (Some(hunt), Some(mover)) = (self.actions.list.hunt_mut(), self.body.mover_mut())
        else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        hunt.carry_to = site;
        hunt.set_drop(at, gap, mover, &walks)?;
        Ok(site.filter(|_| site != carry_to).map(DropSiteId::id))
    }

    /// The drop site a gatherer's `set_state(8)` walks to, its second target `carry_to` or, with
    /// none and a master with drop sites (`+0x7a0`), the closest of them (`FUN_1411671e0`: first
    /// one it reaches, [`Unit::drop_site_from`] outside other objects, then any); −1 for none.
    /// With the footprint gap to it (`FUN_14118f680`).
    pub(crate) fn drop_site(
        &self,
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        carry_to: Option<DropSiteId>,
        state: ActionState,
    ) -> Result<(Option<DropSiteId>, Distance), GenieError> {
        let master = self.master_in(c.masters)?;
        let mover = self
            .body
            .mover()
            .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let mut site = carry_to;
        if site.is_none() && !master.drop_sites.is_empty() {
            let masters = c.masters.ok_or(GenieError::unsupported(
                "a drop-off of a player without masters",
            ))?;
            let sites = &master.drop_sites;
            site = match self.drop_site_from(c, others, state, true)? {
                Some(s) => Some(s),
                None => drop_site_search(
                    others.own_iter(),
                    masters,
                    sites,
                    self.position(),
                    None,
                    (true, Some(action_type::UNPACK)),
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

    /// `FUN_1411671e0` from the unit (`state` its action's, which the planner reads) over its
    /// master's drop sites (`+0x7a0`): [`drop_site_search`] with the unit as the seeker, by path
    /// for a human player (its argument 9, `FUN_141659cf0` false), skipping objects inside others
    /// when `outside` (its argument 10) and those at work on `0x7d` or `0x7c` (its argument 8).
    pub(crate) fn drop_site_from(
        &self,
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        state: ActionState,
        outside: bool,
    ) -> Result<Option<DropSiteId>, GenieError> {
        let by_path = !c.computer;
        self.drop_site_with(
            c,
            others,
            state,
            (outside, by_path, Some(action_type::UNPACK)),
        )
    }

    /// [`Unit::drop_site_from`] with the search's arguments 10, 9 and 8: `outside`, `by_path`
    /// and the action type `at_work` it passes over (−1 none).
    pub(crate) fn drop_site_with(
        &self,
        c: &mut DropWorld<'_>,
        others: OthersRef<'_>,
        state: ActionState,
        (outside, by_path, at_work): (bool, bool, Option<ActionType>),
    ) -> Result<Option<DropSiteId>, GenieError> {
        let masters = c.masters.ok_or(GenieError::unsupported(
            "a drop-off of a player without masters",
        ))?;
        let master = self.master_in(Some(masters))?;
        let mover = self
            .body
            .mover()
            .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let terrain = c.spatial.terrain.as_ref().ok_or(GenieError::unsupported(
            "the map's zones without its terrain",
        ))?;
        let values = terrain
            .restriction(mover.stats.restriction)
            .ok_or(crate::pathing::zones::UNKNOWN_RESTRICTION)?;
        let zones = c.zones.of(c.spatial.map, &terrain.kinds, values)?;
        let me = Me {
            id: self.id,
            group: self.group.id,
            action_state: state,
        };
        let mut plan = Planning {
            pathing: &mut *c.pathing,
            now: c.world_time,
            computer: c.computer,
            object: &mut *c.object,
            groups: c.groups,
        };
        let seeker = Seeker {
            unit: self,
            me,
            mover,
            plan: &mut plan,
            spatial: c.spatial,
            zones,
            class: master.class,
            by_path,
        };
        drop_site_search(
            others.own_iter(),
            masters,
            &master.drop_sites,
            self.position(),
            Some(seeker),
            (outside, at_work),
        )
    }
}
