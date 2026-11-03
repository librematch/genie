//! A builder's and a gatherer's drop-off and a gatherer's update and search.

use crate::GenieError;
use crate::combat::abilities::Look;
use crate::combat::victims::file_kill;
use crate::combat::victims::{readable, victim, write_back};
use crate::construction::FoundationId;
use crate::gathering::DropSiteId;
use crate::gathering::GatherTargetId;
use crate::gathering::drop_sites::SitePlace;
use crate::gathering::gather::Walks;
use crate::gathering::gather::{
    FARM_FOOD_SOLD, GatherAction, GatherReads, GatherState, Next, productivity_of,
};
use crate::gathering::hunt::DropOff;
use crate::gathering::hunt::Dropped;
use crate::gathering::hunt::HuntSprites;
use crate::map::Footprint;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::move_to::WalkTo;
use crate::movement::move_to::{MoveDefaults, SpriteReads};
use crate::movement::{Me, Mover, footprint_gap, footprint_gap_squared};
use crate::objects::ActionState;
use crate::objects::Graphics;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::Action;
use crate::objects::masters::ClassId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::release::Releaser;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Presence;
use crate::objects::unit::{Unit, second_target_checked};
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::players::{ByPlayer, NO_RELATIONS, Relations};
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::world::state_of;
use crate::unit_ai::task_finder::{allied, drops_at};
use crate::unit_ai::world_view::DropWorld;
use crate::world::index::ObjectsRef;
use crate::world::layer::AttributeRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

/// The Gather's update, or what follows it, without the Gather its caller found.
pub(crate) const NO_GATHER: GenieError =
    GenieError::unreachable("a Gather's update without its Gather");

impl Unit {
    /// The Gather's update (slot 6, `0x140a8e1d0`). It first checks its targets: a target gone
    /// from the world's table goes (slot 0x68 with none), one past state 6 is let go (slot 0x180)
    /// and goes; a second target gone goes (`FUN_140a7a190` with none), one past state 6 is let
    /// go and goes. Then state 8 walks the
    /// load to its drop site ([`crate::gathering::gather::GatherAction::update_drop`]): the load goes to the
    /// player, the drop site goes (`FUN_140a7a190` with none) and the gather walks back to its
    /// target (`set_state(4)`); without a target the unit AI's search is not modelled. The other
    /// states ([`crate::gathering::gather::GatherAction::update`]) may ask for the drop-off
    /// ([`Unit::gather_drop_off`]).
    pub(crate) fn gather_update(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let by = self.releaser(c)?;
        self.gather_check_targets(actions, c, by)?;
        let gather = *actions.list.gather().ok_or(NO_GATHER)?;
        let reads = self.gather_reads(c, &gather, roles)?;
        if matches!(gather.state, GatherState::Drop { .. }) {
            return self.gather_drop_update(actions, c, me, gather, &reads, by);
        }
        self.gather_work_update(actions, c, me, gather, &reads, by)
    }

    /// The Gather's update's check of its targets: a target the world does not hold goes, one
    /// past state 6 is let go and goes (one the slice holds but cannot read is not modelled); a
    /// second target ([`second_target_checked`]) alike.
    fn gather_check_targets(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        by: Releaser,
    ) -> Result<(), GenieError> {
        let target = actions.list.gather().ok_or(NO_GATHER)?.target;
        if target.is_some() {
            let state = readable(c, target)?.map(|(_, u, _)| u.life.state);
            let gone = match state {
                Some(s) => ObjectState::Undead < s,
                None if c.w.spatial.get(target).is_some() => {
                    return Err(GenieError::unsupported(
                        "a gather's target the slice cannot read",
                    ));
                }
                None => true,
            };
            if gone {
                if state.is_some() {
                    release(c, self, target, by)?;
                }
                actions.list.gather_mut().ok_or(NO_GATHER)?.target = None;
            }
        }
        let site = actions.list.gather().ok_or(NO_GATHER)?.carry_to;
        let held = Presence::of_found(c.w.spatial.get(site));
        let (kept, let_go) = second_target_checked(
            site,
            held,
            state_of(
                site.map(DropSiteId::id),
                OthersRef::of(&c.w.players, &c.w.index, c.player)?,
            ),
        );
        actions.list.gather_mut().ok_or(NO_GATHER)?.carry_to = kept;
        if let Some(t) = let_go {
            release(c, self, t, by)?;
        }
        Ok(())
    }

    /// What the Gather's `MoveTo` constructors read of this gatherer with its mover `mover`
    /// (`player`'s, whose masters are `every`): the sprites' table `graphics` with the defaults
    /// of its master and terrain look `look`, and what it carries now.
    ///
    /// # Errors
    ///
    /// A gatherer whose master the table lacks.
    pub(crate) fn move_walks<'g>(
        &self,
        mover: Option<&Mover>,
        every: MasterTables<'_>,
        player: PlayerId,
        (graphics, look): (&'g Graphics, Option<&Look>),
        roles: &Roles,
    ) -> Result<Walks<'g>, GenieError> {
        let sprites = SpriteReads::of(self.master(every, player)?, look, graphics, roles);
        Ok(Walks::of(sprites, (self, mover)))
    }

    /// What the Gather's update reads of the gatherer and its player as it starts
    /// ([`crate::gathering::gather::GatherReads`]): the task's sprites and the master's standing
    /// one, the master's work rate (`+0x798`) plus the powerup (slot 0x6e8), its capacity and
    /// class, the task's productivity (1 for none or not above 0), the stance, and the player's
    /// farm settings.
    ///
    /// # Errors
    ///
    /// A gatherer whose master the slice lacks.
    fn gather_reads(
        &self,
        c: &Step<'_>,
        gather: &GatherAction,
        roles: &Roles,
    ) -> Result<GatherReads, GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        let g = |id: Option<SpriteId>| id.and_then(|s| c.w.model.graphics.get(s).copied());
        let task = gather.task;
        let attributes = &c.w.players.player(c.player)?.attributes;
        Ok(GatherReads {
            work_rate: c.w.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity: (task.productivity)
                .and_then(|i| attributes.get(i.index()))
                .copied()
                .filter(|&v| Amount::ZERO < v)
                .unwrap_or(Amount::new(1.0)),
            sprites: HuntSprites {
                moving: g(task.sprites.moving),
                working: g(task.sprites.working),
                carrying: g(task.sprites.carrying),
                standing: g(master.standing_sprite),
            },
            class: master.class,
            defaults: MoveDefaults::of(
                master,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
            master: self.life.master_id,
            stance: c
                .w
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
            farm_mode: c.w.settings.farm_mode,
            player_kind: c.w.players.player(c.player)?.kind,
            stop_productivity: productivity_of(
                task.productivity,
                attributes,
                &c.w.model.tuning.gather,
            ),
            food_sold: attributes
                .get(FARM_FOOD_SOLD)
                .copied()
                .unwrap_or(Amount::ZERO),
        })
    }

    /// The Gather's state 8 ([`crate::gathering::gather::GatherAction::update_drop`]): the walk
    /// of the load to its drop site (the site's take and the gap to it, [`Unit::gather_site`]).
    /// A load dropped goes to the player ([`Unit::gather_load_dropped`]); a drop site lost sets
    /// the drop-off again ([`Unit::gather_drop_off`]); an idle gather lets its targets go.
    fn gather_drop_update(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        mut gather: GatherAction,
        reads: &GatherReads,
        by: Releaser,
    ) -> Result<HeadCode, GenieError> {
        let roles = &c.w.model.roles.clone();
        let (position, radius) =
            c.w.spatial
                .get_known(self.id)?
                .map(|p| (p.position, p.radius))
                .ok_or(GenieError::unsupported("a gatherer without a place"))?;
        let site = self.gather_site(c, gather.carry_to, (position, radius))?;
        let before = gather.targets();
        let dropped = gather.update_drop(c, self, me, reads, site, roles);
        actions.put_gather(gather);
        match dropped? {
            Dropped::Going => Ok(HeadCode::Keeps),
            Dropped::Load(kind, amount) => {
                self.gather_load_dropped(actions, c, by, (kind, amount), radius, roles)
            }
            // set_state(8) again: the drop site kept or searched
            Dropped::Again => self
                .gather_drop_off(actions, c, roles)
                .map(|()| HeadCode::Keeps),
            // set_state(2): the target, then the drop site go (slot 0x68, FUN_140a7a190)
            Dropped::Idle => {
                for id in before.into_iter().flatten() {
                    release(c, self, id, by)?;
                }
                Ok(HeadCode::Keeps)
            }
        }
    }

    /// The drop site `site` as the drop-off reads it (none the world does not place): whether it
    /// takes what the gatherer carries (`FUN_140fdd190`) and the squared gap between the two
    /// footprints (`me`: the gatherer's place and radii).
    fn gather_site(
        &self,
        c: &Step<'_>,
        site: Option<DropSiteId>,
        (position, radius): (Position, Footprint),
    ) -> Result<Option<DropOff>, GenieError> {
        let Some(s) = c.w.spatial.get_known(site)? else {
            return Ok(None);
        };
        let master = self.master(c.w.master_tables(), c.player)?;
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
        Ok(Some(DropOff {
            takes: drops_at(&c.w.model.drop_sites, s.master, &actor),
            gap_squared: footprint_gap_squared(position, radius, s.position, s.radius),
        }))
    }

    /// A load dropped at the drop site (`load`: its kind and amount) goes to the player (its slot
    /// 0xf8 with 1 and 1, [`give_load`], as the hunt's drop-off). Without a target the search
    /// follows ([`Unit::gather_after_drop`]); else the drop site goes (`FUN_140a7a190` with none)
    /// and the gather walks back to its target (`set_state(4)`, the gap from the gatherer's
    /// footprint, `radius` its radii).
    fn gather_load_dropped(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        by: Releaser,
        (kind, amount): (AttributeId, Amount),
        radius: Footprint,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        c.give_load(kind, amount)?;
        let gather = actions.list.gather().ok_or(NO_GATHER)?;
        if gather.target.is_none() {
            return self.gather_after_drop(actions, c, roles);
        }
        release(c, self, gather.carry_to, by)?;
        // the unit's load is gone (slot 0xf8 after the drop): its walking sprite is the moving one
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
        let gather = actions.list.gather_mut().ok_or(NO_GATHER)?;
        gather.carry_to = None;
        let t =
            c.w.spatial
                .get_known(gather.target)?
                .ok_or(GenieError::unsupported("a gather's target without a place"))?;
        let gap = footprint_gap(self.place.position(), radius, t.position, t.radius);
        let state = c.w.object_state(gather.target);
        let mover =
            c.w.parts
                .mover_mut(self.id)
                .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let rng = c.w.streams.get(Stream::Action);
        let to = WalkTo {
            id: gather.target.map(GatherTargetId::id),
            at: t.position,
            gap,
            state,
        };
        gather
            .set_walk(to, t.radius, mover, rng, &walks, &c.w.model.tuning)
            .map(|()| HeadCode::Keeps)
    }

    /// The Gather's other states ([`crate::gathering::gather::GatherAction::update`]) on the
    /// target the caller holds, read as the update starts and written back after it where the
    /// work ran (a kill it made filed: [`crate::combat::victims::file_kill`]); then what follows
    /// ([`Unit::gather_next`]).
    fn gather_work_update(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        mut gather: GatherAction,
        reads: &GatherReads,
        by: Releaser,
    ) -> Result<HeadCode, GenieError> {
        let roles = &c.w.model.roles.clone();
        let mut victim = victim(c, gather.target, roles)?;
        let old = (gather.target, gather.carry_to);
        let next = gather.update(c, self, me, victim.as_mut(), reads, roles);
        actions.put_gather(gather);
        let next = next?;
        // what the work changed on its target, where it ran
        if let Some(v) = victim.as_mut()
            && let Some(k) = write_back(c, v)?
        {
            file_kill(c, (self, actions), k, roles)?;
        }
        self.gather_next(actions, c, next, old, by, roles)
    }

    /// What follows the Gather's update (`next`; `old`: its targets before): an end is its code
    /// (state 1's update returns 1, whose deletion lets both targets go); else each target it
    /// changed is let go (slot 0x68, `FUN_140a7a190` with none), then the drop-off, the search,
    /// the unit's build command on its target (`set_state(6)`'s slot 0x380, forced:
    /// [`crate::world::World::build_command`]) or the reseed (state 0xf2's walk ended,
    /// `FUN_140a8aa60`: [`crate::world::World::reseed_after_walk`]). The last two run inside the
    /// update, on the unit lent back to its slot ([`Step::lend_at`]), and replace or end the
    /// Gather; the update then answers 0, as after them.
    pub(crate) fn gather_next(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        next: Next,
        (old_target, old_second): (Option<GatherTargetId>, Option<DropSiteId>),
        by: Releaser,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        use crate::gathering::gather::Next;
        if next == Next::Done {
            return Ok(HeadCode::Deletes);
        }
        if let Some(g) = actions.list.gather() {
            for (old, new) in [
                (
                    old_target.map(GatherTargetId::id),
                    g.target.map(GatherTargetId::id),
                ),
                (
                    old_second.map(DropSiteId::id),
                    g.carry_to.map(DropSiteId::id),
                ),
            ] {
                if let Some(old) = old.filter(|_| old != new) {
                    release(c, self, old, by)?;
                }
            }
        }
        match next {
            Next::Drop => self.gather_drop_off(actions, c, roles)?,
            Next::Search => self.gather_search(actions, c)?,
            Next::Build => {
                // a build command on no foundation does nothing
                if let Some(target) = actions.list.gather().and_then(|g| g.target) {
                    c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                        // the farm the gatherer works is the foundation it rebuilds
                        w.build_command(p, i, FoundationId::new(target.id()))
                            .map(drop)
                    })?;
                }
            }
            Next::Reseed { end } => {
                c.lend_at((&mut *self, &mut *actions), |w, p, i| {
                    w.reseed_after_walk(p, i, end)
                })?;
            }
            Next::Stay | Next::Done => {}
        }
        Ok(HeadCode::Keeps)
    }

    /// The Gather's search in state 3 (`0x140a8cd40` case 3 and its repeat, `0x140a8e1d0` case
    /// 2): the unit's notify 0x202 to its unit AI (slot 0x328) with the gather action (`0x261`),
    /// the Gather's target type (`+0x68`) and a drop site: the second target's, else for a master
    /// with drop sites (`+0x7a0`) the one `FUN_1411671e0` finds from the unit (inside others too:
    /// [`Unit::drop_site_from`]), 0 when it finds none; −1 for a master without.
    pub(crate) fn gather_search(
        &self,
        actions: &Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let by_id = ObjectsRef::new(
            &c.w.players,
            &c.w.index,
            &c.w.model.masters,
            &c.w.parts,
            &c.w.model.roles,
            &c.w.model.tuning,
        );
        let others = OthersRef::of(&c.w.players, &c.w.index, c.player)?;
        let gather = actions.list.gather().ok_or(NO_GATHER)?;
        let (carry_to, class, state) = (gather.carry_to, gather.target_class, gather.state);
        let master = self.master(c.w.master_tables(), c.player)?;
        let site = if let Some(carry) = carry_to {
            carry.id().get()
        } else if !master.action_part().drop_sites.is_empty() {
            let mut w = DropWorld {
                masters: OwnerMasters::new(&c.w.parts.own_master, c.w.model.masters.of(c.player)),
                spatial: &mut c.w.spatial,
                zones: &mut c.w.zones,
                pathing: &mut c.w.pathing,
                object: c.w.streams.get(Stream::Object),
                world_time: c.w.world_time,
                player_kind: c.w.players.player(c.player)?.kind,
                groups: &c.w.groups,
            };
            // the notify's record holds the drop site's id, 0 for none
            self.drop_site_from(
                (actions, c.w.parts.mover_of(self.id)),
                &mut w,
                others,
                ActionState::new(state.get()),
                SitePlace::Anywhere,
                by_id,
            )?
            .map_or(0, |s| s.id().get())
        } else {
            -1
        };
        self.post_own(
            &mut c.w.parts.unit_ai,
            NotifyKind::REQUIRES_SEARCH,
            [
                AiAction::Gather.value(),
                i32::from(ClassId::field(class)),
                site,
            ],
        )
    }
}

/// The player's slot 0xf8 (`FUN_141616820`) with 1 and 1 on a unit's load: the resource and its
/// gathered total (166 to 168; gold's 49 and 169) in `adds`, which the caller gives the player; the
/// statistics (slot 0xb8) and the local player's tracker (`FUN_14097daf0`) are not held. With 1
/// the resources would go to the allies too when the player shares (`sharing`, `+0x1426`,
/// `FUN_14165d040`): a sharing player with allies is not modelled.
pub(crate) fn give_load(
    (relations, sharing): (&ByPlayer<Relations>, bool),
    player: PlayerId,
    adds: &mut Vec<(AttributeId, Amount)>,
    kind: AttributeId,
    amount: Amount,
    roles: &Roles,
    slots: Slots,
) -> Result<(), GenieError> {
    let own = relations.get(player).unwrap_or(&NO_RELATIONS);
    let allied = PlayerId::each(own.len())
        .skip(1)
        .any(|q| q != player && allied(own, player, q, AllyTest::WithRelation4));
    if sharing && allied {
        return Err(GenieError::unsupported(
            "a load given to a player with allies (+0x1426)",
        ));
    }
    adds.push((kind, amount));
    if roles.of_attribute(kind).contains(AttributeRoles::GOLD) {
        adds.push((slots.attribute.gold_counter, amount));
        adds.push((slots.attribute.gold_gathered, amount));
    } else if kind.is_resource() {
        // food, wood and stone gathered: 166 and on
        adds.push((slots.attribute.gathered.offset(kind.index())?, amount));
    }
    Ok(())
}

impl Action {
    /// The Gather's copy `gather` back in the action list, as its update left it.
    fn put_gather(&mut self, gather: GatherAction) {
        if let Some(g) = self.list.gather_mut() {
            *g = gather;
        }
    }
}
