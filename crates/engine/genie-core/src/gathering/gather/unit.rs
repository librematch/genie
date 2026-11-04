//! A builder's and a gatherer's drop-off and a gatherer's update and search.

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
use crate::objects::masters::PlayersMastersExt;
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
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The Gather's update (slot 6, `0x140a8e1d0`). It first checks its targets: a target gone
    /// from the world's table goes (slot 0x68 with none), one past state 6 is let go (slot 0x180)
    /// and goes; a second target gone goes (`FUN_140a7a190` with none), one past state 6 is let
    /// go and goes. Then state 8 walks the
    /// load to its drop site ([`crate::gathering::gather::GatherAction::update_drop`]): the load goes to the
    /// player, the drop site goes (`FUN_140a7a190` with none) and the gather walks back to its
    /// target (`set_state(4)`); without a target the unit AI's search is not modelled. The other
    /// states ([`crate::gathering::gather::GatherAction::update`]) may ask for the drop-off
    /// ([`Unit::gather_drop_off`]). `gather` is the head's copy.
    pub(crate) fn gather_update(
        &mut self,
        mut gather: GatherAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let by = self.releaser(step)?;
        let checked = self.gather_check_targets(&mut gather, step, by);
        // the check reaches no action of the unit's: the copy goes back, before an error too
        actions.list.set_gather(Some(gather));
        checked?;
        let reads = self.gather_reads(step, &gather, roles)?;
        if matches!(gather.state, GatherState::Drop { .. }) {
            return self.gather_drop_update(actions, step, me, gather, &reads, by);
        }
        self.gather_work_update(actions, step, me, gather, &reads, by)
    }

    /// The Gather's update's check of its targets: a target the world does not hold goes, one
    /// past state 6 is let go and goes (one the slice holds but cannot read is not modelled); a
    /// second target ([`second_target_checked`]) alike.
    fn gather_check_targets(
        &mut self,
        gather: &mut GatherAction,
        step: &mut Step<'_>,
        by: Releaser,
    ) -> GenieResult<()> {
        let target = gather.target;
        if target.is_some() {
            let state = readable(step, target)?.map(|(_, unit, _)| unit.life.state);
            let gone = match state {
                Some(object_state) => ObjectState::Undead < object_state,
                None if step.world.spatial.get(target).is_some() => {
                    return Err(GenieError::unsupported(
                        "a gather's target the slice cannot read",
                    ));
                }
                None => true,
            };
            if gone {
                if state.is_some() {
                    release(step, self, target, by)?;
                }
                gather.target = None;
            }
        }
        let site = gather.carry_to;
        let held = Presence::of_found(step.world.spatial.get(site));
        let (kept, let_go) = second_target_checked(
            site,
            held,
            state_of(
                site.map(DropSiteId::id),
                OthersRef::of(&step.world.players, &step.world.index, step.player)?,
            ),
        );
        gather.carry_to = kept;
        if let Some(role_id) = let_go {
            release(step, self, role_id, by)?;
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
    pub(crate) fn move_walks<'graphics>(
        &self,
        mover: Option<&Mover>,
        every: MasterTables<'_>,
        player: PlayerId,
        (graphics, look): (&'graphics Graphics, Option<&Look>),
        roles: &Roles,
    ) -> GenieResult<Walks<'graphics>> {
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
        step: &Step<'_>,
        gather: &GatherAction,
        roles: &Roles,
    ) -> GenieResult<GatherReads> {
        let master = self.master(step.world.master_tables(), step.player)?;
        let graphic_of = |id: Option<SpriteId>| {
            id.and_then(|sprite_id| step.world.model.graphics.get(sprite_id).copied())
        };
        let task = gather.task;
        let attributes = &step.world.players.player(step.player)?.attributes;
        Ok(GatherReads {
            work_rate: step.world.parts.combat_of(self.id).work_rate_powerup
                + master.action_part().work_rate,
            capacity: Amount::of_master(master.capacity),
            productivity: (task.productivity)
                .and_then(|attribute_id| attributes.get(attribute_id.index()))
                .copied()
                .filter(|&amount| Amount::ZERO < amount)
                .unwrap_or(Amount::new(1.0)),
            sprites: HuntSprites {
                moving: graphic_of(task.sprites.moving),
                working: graphic_of(task.sprites.working),
                carrying: graphic_of(task.sprites.carrying),
                standing: graphic_of(master.standing_sprite),
            },
            class: master.class,
            defaults: MoveDefaults::of(
                master,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
                roles,
            ),
            master: self.life.master_id,
            stance: step
                .world
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
            farm_mode: step.world.settings.farm_mode,
            player_kind: step.world.players.player(step.player)?.kind,
            stop_productivity: productivity_of(
                task.productivity,
                attributes,
                &step.world.model.tuning.gather,
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
        step: &mut Step<'_>,
        me: Me,
        mut gather: GatherAction,
        reads: &GatherReads,
        by: Releaser,
    ) -> GenieResult<HeadCode> {
        let roles = &step.world.model.roles.clone();
        let (position, radius) = step
            .world
            .spatial
            .get_known(self.id)?
            .map(|placed| (placed.position, placed.radius))
            .ok_or(GenieError::unsupported("a gatherer without a place"))?;
        let site = self.gather_site(step, gather.carry_to, (position, radius))?;
        let before = gather.targets();
        let dropped = gather.update_drop(step, self, me, reads, site, roles);
        actions.put_gather(gather);
        match dropped? {
            Dropped::Going => Ok(HeadCode::Keeps),
            Dropped::Load(kind, amount) => {
                self.gather_load_dropped((gather, actions), step, by, (kind, amount), radius, roles)
            }
            // set_state(8) again: the drop site kept or searched
            Dropped::Again => self
                .gather_drop_off(&mut gather, actions, step, roles)
                .map(|()| HeadCode::Keeps),
            // set_state(2): the target, then the drop site go (slot 0x68, FUN_140a7a190)
            Dropped::Idle => {
                for id in before.into_iter().flatten() {
                    release(step, self, id, by)?;
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
        step: &Step<'_>,
        site: Option<DropSiteId>,
        (position, radius): (Position, Footprint),
    ) -> GenieResult<Option<DropOff>> {
        let Some(placed) = step.world.spatial.get_known(site)? else {
            return Ok(None);
        };
        let master = self.master(step.world.master_tables(), step.player)?;
        let actor = Actor {
            id: self.id,
            master,
            player: step.player,
            held: self.life.held(),
            attribute_95: step
                .world
                .players
                .player(step.player)?
                .attributes
                .get(95)
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: step.world.players.player(step.player)?.civ,
        };
        Ok(Some(DropOff {
            takes: drops_at(&step.world.model.drop_sites, placed.master, &actor),
            gap_squared: footprint_gap_squared(position, radius, placed.position, placed.radius),
        }))
    }

    /// A load dropped at the drop site (`load`: its kind and amount) goes to the player (its slot
    /// 0xf8 with 1 and 1, [`give_load`], as the hunt's drop-off). Without a target the search
    /// follows ([`Unit::gather_after_drop`]); else the drop site goes (`FUN_140a7a190` with none)
    /// and the gather walks back to its target (`set_state(4)`, the gap from the gatherer's
    /// footprint, `radius` its radii). `gather` is the head as the drop left it.
    fn gather_load_dropped(
        &mut self,
        (mut gather, actions): (GatherAction, &mut Action),
        step: &mut Step<'_>,
        by: Releaser,
        (kind, amount): (AttributeId, Amount),
        radius: Footprint,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        step.give_load(kind, amount)?;
        if gather.target.is_none() {
            return self.gather_after_drop(gather, actions, step, roles);
        }
        release(step, self, gather.carry_to, by)?;
        // the release and the walk's reads reach no action of the unit's: the copy goes back
        // after the walk, before an error too
        let walked = self.gather_walk_back(&mut gather, step, radius, roles);
        actions.list.set_gather(Some(gather));
        walked
    }

    /// The walk back to the target after the drop (`set_state(4)`): the drop site is none, and
    /// a quiet `MoveTo` walks to the target from the gatherer's footprint (`radius` its radii).
    fn gather_walk_back(
        &self,
        gather: &mut GatherAction,
        step: &mut Step<'_>,
        radius: Footprint,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        // the unit's load is gone (slot 0xf8 after the drop): its walking sprite is the moving one
        let walks = self.move_walks(
            step.world.parts.mover_of(self.id),
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
            (
                &step.world.model.graphics,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        gather.carry_to = None;
        let placed = step
            .world
            .spatial
            .get_known(gather.target)?
            .ok_or(GenieError::unsupported("a gather's target without a place"))?;
        let gap = footprint_gap(
            self.place.position(),
            radius,
            placed.position,
            placed.radius,
        );
        let state = step.world.object_state(gather.target);
        let mover = step
            .world
            .parts
            .mover_mut(self.id)
            .ok_or(GenieError::unsupported("a gatherer without movement state"))?;
        let rng = step.world.streams.get_mut(Stream::Action);
        let to = WalkTo {
            id: gather.target.map(GatherTargetId::id),
            at: placed.position,
            gap,
            state,
        };
        gather
            .set_walk(
                to,
                placed.radius,
                mover,
                rng,
                &walks,
                &step.world.model.tuning,
            )
            .map(|()| HeadCode::Keeps)
    }

    /// The Gather's other states ([`crate::gathering::gather::GatherAction::update`]) on the
    /// target the caller holds, read as the update starts and written back after it where the
    /// work ran (a kill it made filed: [`crate::combat::victims::file_kill`]); then what follows
    /// ([`Unit::gather_next`]).
    fn gather_work_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        mut gather: GatherAction,
        reads: &GatherReads,
        by: Releaser,
    ) -> GenieResult<HeadCode> {
        let roles = &step.world.model.roles.clone();
        let mut victim = victim(step, gather.target, roles)?;
        let old = (gather.target, gather.carry_to);
        let next = gather.update(step, self, me, victim.as_mut(), reads, roles);
        actions.put_gather(gather);
        let next = next?;
        // what the work changed on its target, where it ran; the kill changes the unit's
        // actions only by livestock's change to Gaia, and livestock gathers nothing: the game
        // goes on with the Gather itself
        if let Some(victim) = victim.as_mut()
            && let Some(kill_left) = write_back(step, victim)?
        {
            file_kill(step, (self, actions), kill_left, roles)?;
        }
        self.gather_next((&mut gather, actions), step, next, old, by, roles)
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
        (gather, actions): (&mut GatherAction, &mut Action),
        step: &mut Step<'_>,
        next: Next,
        (old_target, old_second): (Option<GatherTargetId>, Option<DropSiteId>),
        by: Releaser,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        use crate::gathering::gather::Next;
        if next == Next::Done {
            return Ok(HeadCode::Deletes);
        }
        for (old, new) in [
            (
                old_target.map(GatherTargetId::id),
                gather.target.map(GatherTargetId::id),
            ),
            (
                old_second.map(DropSiteId::id),
                gather.carry_to.map(DropSiteId::id),
            ),
        ] {
            if let Some(old) = old.filter(|_| old != new) {
                release(step, self, old, by)?;
            }
        }
        match next {
            Next::Drop => self.gather_drop_off(gather, actions, step, roles)?,
            Next::Search => self.gather_search(gather, actions, step)?,
            Next::Build => {
                // a build command on no foundation does nothing
                if let Some(target) = gather.target {
                    step.lend_at(
                        (&mut *self, &mut *actions),
                        |world, player_id, list_index| {
                            // the farm the gatherer works is the foundation it rebuilds
                            world
                                .build_command(
                                    player_id,
                                    list_index,
                                    FoundationId::new(target.id()),
                                )
                                .map(drop)
                        },
                    )?;
                }
            }
            Next::Reseed { end } => {
                step.lend_at(
                    (&mut *self, &mut *actions),
                    |world, player_id, list_index| {
                        world.reseed_after_walk(player_id, list_index, end)
                    },
                )?;
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
        gather: &GatherAction,
        actions: &Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let by_id = ObjectsRef::new(
            &step.world.players,
            &step.world.index,
            &step.world.model.masters,
            &step.world.parts,
            &step.world.model.roles,
            &step.world.model.tuning,
        );
        let others = OthersRef::of(&step.world.players, &step.world.index, step.player)?;
        let (carry_to, class, state) = (gather.carry_to, gather.target_class, gather.state);
        let master = self.master(step.world.master_tables(), step.player)?;
        let site = if let Some(carry) = carry_to {
            carry.id().get()
        } else if !master.action_part().drop_sites.is_empty() {
            let mut drop_world = DropWorld {
                masters: OwnerMasters::new(
                    &step.world.parts.own_master,
                    step.world.model.masters.of(step.player),
                ),
                spatial: &mut step.world.spatial,
                zones: &mut step.world.zones,
                pathing: &mut step.world.pathing,
                object: step.world.streams.get_mut(Stream::Object),
                world_time: step.world.world_time,
                player_kind: step.world.players.player(step.player)?.kind,
                groups: &step.world.groups,
            };
            // the notify's record holds the drop site's id, 0 for none
            self.drop_site_from(
                (actions, step.world.parts.mover_of(self.id)),
                &mut drop_world,
                others,
                ActionState::new(state.get()),
                SitePlace::Anywhere,
                by_id,
            )?
            .map_or(0, |role_id| role_id.id().get())
        } else {
            -1
        };
        self.post_own(
            &mut step.world.parts.unit_ai,
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
) -> GenieResult<()> {
    let own = relations.get(player).unwrap_or(&NO_RELATIONS);
    let allied = PlayerId::each(own.len()).skip(1).any(|player_id| {
        player_id != player && allied(own, player, player_id, AllyTest::WithRelation4)
    });
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
        if let Some(gather_action) = self.list.gather_mut() {
            *gather_action = gather;
        }
    }
}
