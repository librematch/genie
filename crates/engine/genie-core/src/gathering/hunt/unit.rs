//! A hunter's search and its walk after a drop.

use crate::combat::attack::BeginLayer;
use crate::combat::attack::{arms, engagement};
use crate::combat::victims::victim;
use crate::gathering::ActionEnd;
use crate::gathering::hunt::HuntChase;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::{HuntAction, HuntTask, Quarry, stand_sprite};
use crate::map::Distance;
use crate::map::spatial::{SearchQuery, Spatial};
use crate::math::rng::Stream;
use crate::movement::MoveStart;
use crate::movement::Speed;
use crate::movement::{Me, Planning};
use crate::objects::ActionState;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Action;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::objects::sprite::new_sprite;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::{ObjectKey, WorkerCount};
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::OrderKind;
use crate::unit_ai::Search;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::world_view::DropWorld;
use crate::unit_ai::{CIVILIAN_CARED, Memory, Pick, civilian_kind, civilian_search_radius};
use crate::world::HUNTER_UNMOVING;
use crate::world::SEARCH_AGAIN;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

mod found;

impl Unit {
    /// The Hunt's `set_state(3)` (`0x140ac60e0` case 3) after its target went empty when it faced
    /// it: the target goes (slot 0x68 with none: its slot 0x180), the unit AI is told
    /// ([`Unit::hunt_search_notify`]), the unit takes the sprite it stands with (slot 0x68 with
    /// `FUN_140ac5210`'s) and the timer starts at −10 s. A hunt of class 10 would end its search
    /// (`set_state` 2), not modelled.
    pub(crate) fn hunt_set_search(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        if !roles
            .of_some_class(hunt.target_class)
            .contains(ClassRoles::PREDATOR)
        {
            let target = hunt.target.take();
            actions.list.set_hunt(Some(*hunt));
            self.hunt_release(step, [target])?;
        }
        self.hunt_search_state(hunt, actions, step, roles)
    }

    /// The Hunt's `set_state(3)` itself (`0x140ac60e0` case 3; its target stays): the unit AI is
    /// told ([`Unit::hunt_search_notify`]), the unit takes the sprite it stands with and the timer
    /// starts at −10 s. A human player's hunt of class 10 is done instead (`set_state` 2: the
    /// target and the second target go), a computer player's would end (1), not modelled.
    pub(crate) fn hunt_search_state(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let task = hunt.task;
        if roles
            .of_some_class(hunt.target_class)
            .contains(ClassRoles::PREDATOR)
        {
            // a human player's hunt is done (set_state 2), a computer player's ends (1)
            if step.w.players.player(step.player)?.kind.is_computer() {
                return Err(GenieError::unsupported(
                    "a computer player's hunt of class 10's search (set_state 1)",
                ));
            }
            let released = hunt.set_done();
            actions.list.set_hunt(Some(*hunt));
            self.hunt_stand(step, &task)?;
            self.hunt_release(step, released)?;
            return Ok(());
        }
        hunt.timer = SEARCH_AGAIN;
        actions.list.set_hunt(Some(*hunt));
        self.hunt_search_notify(hunt, actions, step)?;
        self.hunt_stand(step, &task)
    }

    /// The hunter takes the sprite its hunt stands with (slot 0x68 with `FUN_140ac5210`'s, by the
    /// task and what the unit carries; [`crate::gathering::hunt::stand_sprite`]).
    pub(crate) fn hunt_stand(&mut self, step: &mut Step<'_>, task: &HuntTask) -> GenieResult<()> {
        let master = self.master(step.w.master_tables(), step.player)?;
        let stand = stand_sprite(
            task,
            master.standing_sprite,
            self.life.held(),
            &step.w.model.graphics,
        );
        if let Some(graphic) = stand {
            new_sprite(
                &mut self.sprite,
                step.w.parts.move_stats_mut(self.id),
                graphic,
                step.w.streams.get(Stream::Sprite),
                step.w.model.tuning.sprite,
            );
        }
        Ok(())
    }

    /// The objects a hunt lets go (−1 for none), each where the hunt does (slot 0x180,
    /// [`crate::world::lets::release`]).
    ///
    /// # Errors
    ///
    /// The release's.
    pub(crate) fn hunt_release<K: ObjectKey, const N: usize>(
        &mut self,
        step: &mut Step<'_>,
        ids: [K; N],
    ) -> GenieResult<()> {
        let by = self.releaser(step)?;
        for id in ids.into_iter().filter_map(ObjectKey::key) {
            release(step, self, id, by)?;
        }
        Ok(())
    }

    /// The Hunt in state 3 tells its unit AI to look for the next target (`0x140ac60e0` case 3,
    /// `0x140ac7a00` case 2): notify 0x202 with the hunt action (0x265), its target class (`+0x68`)
    /// and its drop site: the second target; −1 for a master without drop sites (`+0x7a0`); else
    /// the closest (`FUN_1411671e0` as `set_state(8)` searches, [`Unit::drop_site`]), with none
    /// found 0, and the hunt's class goes (−1). `hunt` is the head's copy, which the list holds.
    pub(crate) fn hunt_search_notify(
        &self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let by_id = ObjectsRef::new(
            &step.w.players,
            &step.w.index,
            &step.w.model.masters,
            &step.w.parts,
            &step.w.model.roles,
            &step.w.model.tuning,
        );
        let others = OthersRef::of(&step.w.players, &step.w.index, step.player)?;
        let (carry_to, class) = (hunt.carry_to, i32::from(ClassId::field(hunt.target_class)));
        let master = self.master(step.w.master_tables(), step.player)?;
        let site = if let Some(carry) = carry_to {
            carry.id().get()
        } else if master.action_part().drop_sites.is_empty() {
            -1
        } else {
            let mut drop_world = DropWorld {
                masters: OwnerMasters::new(
                    &step.w.parts.own_master,
                    step.w.model.masters.of(step.player),
                ),
                spatial: &mut step.w.spatial,
                zones: &mut step.w.zones,
                pathing: &mut step.w.pathing,
                object: step.w.streams.get(Stream::Object),
                world_time: step.w.world_time,
                player_kind: step.w.players.player(step.player)?.kind,
                groups: &step.w.groups,
            };
            let (site, _) = self.drop_site(
                (actions, step.w.parts.mover_of(self.id)),
                &mut drop_world,
                others,
                None,
                ActionState::new(HuntState::Search.get()),
                by_id,
            )?;
            match site {
                None => {
                    hunt.target_class = None;
                    actions.list.set_hunt(Some(*hunt));
                    0
                }
                // the notify's record holds the drop site's id
                Some(site) => site.id().get(),
            }
        };
        self.post_own(
            &mut step.w.parts.unit_ai,
            NotifyKind::REQUIRES_SEARCH,
            [AiAction::Hunt.value(), class, site],
        )
    }

    /// The hunt's state 8 after its load went (`0x140ac7a00` case 7, after `FUN_14119d310`): a
    /// hunt with a target lets the drop site go (`FUN_140a7a190` with none: slot 0x180) and walks
    /// back to it (`set_state` 4). One without asks its unit AI: the queued orders (gather, hunt,
    /// on a target) whose target is gone or dead go (`FUN_140c5de60`); orders left end the hunt
    /// (the AI's order and action clear, `set_state` 1, the action list removes it: its teardown
    /// lets the drop site go). The target class (the AI's target's, else the hunt's `+0x68`) the
    /// civilian's slot 0x1b0 knows (`FUN_140c1c4f0`) takes the search by kind (slot 0x80); a hunt
    /// of class 10 searches nothing; a unit in a pasture (`FUN_1411c7670`) and a farm found are
    /// not modelled. Else `ClosestObject` (`FUN_140c488b0`, every relation, below one worker for
    /// class 49) of the class over the AI's search (`FUN_140c63e20`: its sight plus a tile, at
    /// least its search radius, plus the civilian's growth; enemies found are not modelled, else
    /// the alert mode clears). The object found becomes the hunt's target (slot 0x48,
    /// [`HuntAction::retarget`], which takes it: its workers count one more); with none a human
    /// player's hunt walks to its point to search again (`set_state(11)`,
    /// [`HuntAction::search_walk`]). A hunt that fails at once takes the standing sprite (slot
    /// 0x68 with `FUN_140ac5210`'s: the unit carries nothing now).
    ///
    /// [`HuntAction::retarget`]: crate::gathering::hunt::HuntAction::retarget
    /// [`HuntAction::search_walk`]: crate::gathering::hunt::HuntAction::search_walk
    pub(crate) fn hunt_after_drop(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let _ = self.master(step.w.master_tables(), step.player)?;
        if hunt.target.is_some() {
            return self
                .hunt_back_to_target(hunt, actions, step, roles)
                .map(|()| HeadCode::Keeps);
        }
        if self.hunt_drop_orders(step)? == ActionEnd::Ends {
            return Ok(HeadCode::Deletes);
        }
        let (found, kind) = self.hunt_next_search(hunt, actions, step, roles)?;
        let Some(found) = found else {
            return self
                .hunt_found_nothing(hunt, actions, step, kind, roles)
                .map(|()| HeadCode::Keeps);
        };
        self.hunt_take_found((hunt, actions), step, found, roles)
            .map(|()| HeadCode::Keeps)
    }

    /// [`Unit::hunt_after_drop`] with a target: the drop site goes (`FUN_140a7a190` with none)
    /// and the hunt walks back to it (`set_state(4)`, [`crate::gathering::hunt::HuntAction::chase`]
    /// with the hunter's engagement and arms). A chase that starts arrived, or fails, is not
    /// modelled.
    fn hunt_back_to_target(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(step.w.master_tables(), step.player)?;
        let victim = victim(step, hunt.target, roles)?.ok_or(GenieError::unsupported(
            "a hunt's target the slice does not hold live",
        ))?;
        let quarry = Quarry {
            id: victim.id,
            position: victim.position,
            class: victim.class,
            kind: victim.kind,
            state: victim.state,
            player: victim.player,
            amount: victim.amount,
            allied: victim.allied.by_attacker,
            top_action: None,
            drop_site: false,
        };
        let (me_at, them) = (
            step.w
                .spatial
                .get_known(self.id)?
                .ok_or(GenieError::unsupported("a hunter without a place"))?,
            step.w
                .spatial
                .get_known(victim.id)?
                .ok_or(GenieError::unsupported("a hunt's target without a place"))?,
        );
        let speed = step
            .w
            .parts
            .mover_of(self.id)
            .map_or(Speed::ZERO, |m| m.stats.speed);
        let engagement = engagement(
            step.w.parts.unit_ai.get(self.id),
            speed,
            master,
            me_at,
            them,
            step.w.world_time,
        )?;
        let moving = hunt.task.walk_sprite_id(self.life.held());
        let arms = arms(
            master,
            &step.w.model.graphics,
            hunt.task.sprites,
            moving,
            roles,
        );
        let site = hunt.carry_to.take();
        actions.list.set_hunt(Some(*hunt));
        let by = self.releaser(step)?;
        if site.is_some() {
            release(step, self, site, by)?;
        }
        let walks = self.move_walks(
            step.w.parts.mover_of(self.id),
            MasterTables::new(&step.w.model.masters, &step.w.parts.own_master),
            step.player,
            (
                &step.w.model.graphics,
                step.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let Some(mover) = step.w.parts.mover_mut(self.id) else {
            return Err(GenieError::unsupported(
                "a hunter without its hunt or movement state",
            ));
        };
        let chase = HuntChase {
            target: &quarry,
            player: step.player,
            walks: &walks,
        };
        let chased = hunt.chase(
            chase,
            &engagement,
            arms,
            mover,
            BeginLayer {
                roles,
                attack_tuning: &step.w.model.tuning.attack,
                move_to_tuning: &step.w.model.tuning.move_to,
            },
            &step.w.model.tuning.hunt,
        );
        actions.list.set_hunt(Some(*hunt));
        if chased? == MoveStart::Arrived {
            return Err(GenieError::unsupported(
                "a hunt's Attack whose MoveTo starts arrived after its drop-off",
            ));
        }
        if hunt.state == HuntState::Failed {
            return Err(GenieError::unsupported(
                "a hunt failing on its target after its drop-off",
            ));
        }
        Ok(())
    }

    /// The unit AI's queued orders after the drop: the orders (gather, hunt, on a target) whose
    /// target is gone (the world does not hold it, or its sight sees it past state 5) leave
    /// (`FUN_140c5de60`); with orders left the AI's order and action clear (`FUN_140c64b60`,
    /// `FUN_140c64b50`) and the hunt ends (`set_state(1)`; its update returns 1, whose deletion
    /// lets the drop site go): its unit AI runs the queued order ([`ActionEnd::Ends`]).
    fn hunt_drop_orders(&self, step: &mut Step<'_>) -> GenieResult<ActionEnd> {
        let ai = step
            .w
            .parts
            .unit_ai
            .get_mut(self.id)
            .ok_or(GenieError::unsupported(
                "a hunt without a unit AI after its drop-off",
            ))?;
        let spatial: &Spatial = &step.w.spatial;
        let gone = |id: Option<ObjectId>| {
            let Some(id) = id.filter(|&id| spatial.get(id).is_some()) else {
                return true;
            };
            spatial
                .sight
                .as_ref()
                .and_then(|visibility| visibility.seer(id))
                .is_some_and(|seer| ObjectState::Dead < seer.state)
        };
        ai.orders.retain(|queued_order| {
            !(matches!(
                queued_order.order,
                OrderKind::Gather | OrderKind::Hunt | OrderKind::OnTarget
            ) && gone(queued_order.target))
        });
        if ai.orders.is_empty() {
            return Ok(ActionEnd::GoesOn);
        }
        ai.end_order_and_action()?;
        Ok(ActionEnd::Ends)
    }

    /// The search after the drop by the target class (the AI's target's, else the hunt's
    /// `+0x68`) the civilian's slot 0x1b0 knows (`FUN_140c1c4f0`): of a kind, the unit AI's
    /// `ClosestResourceObject` (slot 0x80, at most 2 workers for wood, no drop site, state 2, no
    /// point; the probe of 4533's call); a hunt of class 10 searches nothing; any other class
    /// `ClosestObject` ([`Unit::hunt_closest_object`]). The object found, if any, and the
    /// kind.
    fn hunt_next_search(
        &self,
        hunt: &HuntAction,
        actions: &Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<(Option<ObjectId>, Option<GatherGroup>)> {
        let ai = step
            .w
            .parts
            .unit_ai
            .get(self.id)
            .ok_or(GenieError::unsupported(
                "a hunt without a unit AI after its drop-off",
            ))?;
        let class = ai.target_class.or(hunt.target_class);
        if ai.ai_kind != UnitAiKind::Civilian {
            return Err(GenieError::unsupported(
                "slot 0x1b0 of another unit AI than the civilian's",
            ));
        }
        let kind = class.and_then(|class| civilian_kind(class, roles));
        let found = match kind {
            None if roles
                .of_some_class(hunt.target_class)
                .contains(ClassRoles::PREDATOR) =>
            {
                None
            }
            None => self.hunt_closest_object((hunt, actions), step, class, roles)?,
            Some(group) => {
                let cap = if group == GatherGroup::WOOD {
                    WorkerCount::of_data(2)
                } else {
                    WorkerCount::NO_CAP
                };
                self.resource_search(actions, step, group, cap)?
            }
        };
        Ok((found, kind))
    }

    /// `ClosestObject` (`FUN_140c488b0`) after the drop for a class of no kind: every relation,
    /// below one worker for class 49, reached, over the unit AI's search (`FUN_140c63e20`: its
    /// sight plus a tile, at least its search radius, plus the civilian's growth); its alert
    /// follows the enemies found. A unit in a pasture (`FUN_1411c7670`) and a class-10 searcher
    /// are not modelled. The object found, -1 for none.
    fn hunt_closest_object(
        &self,
        (hunt, actions): (&HuntAction, &Action),
        step: &mut Step<'_>,
        class: Option<ClassId>,
        roles: &Roles,
    ) -> GenieResult<Option<ObjectId>> {
        if self.my_pasture.is_some() {
            return Err(GenieError::unsupported(
                "a hunter tending a pasture (FUN_1411c7670)",
            ));
        }
        let master = self.master(step.w.master_tables(), step.player)?;
        let mover = step.w.parts.mover_of(self.id).ok_or(HUNTER_UNMOVING)?;
        let radius =
            civilian_search_radius(master.line_of_sight, master.action_part().search_radius);
        let search = Search {
            at: self.position().ground(),
            radius,
            classes: &CIVILIAN_CARED,
            size: master.radius,
            pick: Pick::Closest {
                group: class,
                relation: None,
                state: None,
                master: None,
                workers: if roles.of_some_class(class).contains(ClassRoles::FARM) {
                    WorkerCount::ONE
                } else {
                    WorkerCount::NO_CAP
                },
                spared: None,
                reach: true,
            },
            memory: Memory::default(),
        };
        let me = Me {
            action_state: ActionState::new(hunt.state.get()),
            ..self.me(actions)
        };
        let mut plan = Planning {
            pathing: &mut step.w.pathing,
            now: step.w.world_time,
            player_kind: step.w.players.player(step.player)?.kind,
            object: step.w.streams.get(Stream::Object),
            groups: &step.w.groups,
        };
        let (master_id, at) = (self.life.master_id, self.position());
        let by_id = ObjectsRef::new(
            &step.w.players,
            &step.w.index,
            &step.w.model.masters,
            &step.w.parts,
            &step.w.model.roles,
            &step.w.model.tuning,
        );
        let mut reach = |spatial: &mut Spatial, id: ObjectId, range: Distance| {
            mover.reaches_object((at, &me), (spatial, by_id), &mut plan, id, range, master_id)
        };
        let result = step.w.spatial.search(
            SearchQuery {
                searcher: self.id,
                player: step.player,
                search: &search,
                attack: None,
                reach: &mut reach,
                objects: by_id,
            },
            roles,
            &step.w.model.tuning.best_target,
            &step.w.model.tuning.spatial,
            step.w.model.tuning.registry,
        )?;
        // FUN_140c63e20's alert: enemies found set the alert mode and its time again
        let ai = step
            .w
            .parts
            .unit_ai
            .get_mut(self.id)
            .ok_or(GenieError::unsupported(
                "a hunt without a unit AI after its drop-off",
            ))?;
        ai.alert(result, step.dt.seconds());
        Ok(result.id)
    }

    /// Nothing found after the drop (`LAB_140ac9509`): a computer player's hunt of a kind but
    /// wood would end (`set_state(1)`), not modelled; a hunt with its point walks there to search
    /// again (`set_state(11)`, its second target kept,
    /// [`crate::gathering::hunt::HuntAction::search_walk`]); one without would wait
    /// (`set_state(2)`), not modelled.
    fn hunt_found_nothing(
        &self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        kind: Option<GatherGroup>,
        roles: &Roles,
    ) -> GenieResult<()> {
        if step.w.players.player(step.player)?.kind.is_computer() && kind != Some(GatherGroup::WOOD)
        {
            return Err(GenieError::unsupported(
                "a computer player's hunt that found nothing after its drop-off (set_state 1)",
            ));
        }
        let walks = self.move_walks(
            step.w.parts.mover_of(self.id),
            MasterTables::new(&step.w.model.masters, &step.w.parts.own_master),
            step.player,
            (
                &step.w.model.graphics,
                step.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        if hunt.point.x() == -1.0 || hunt.point.y() == -1.0 {
            return Err(GenieError::unsupported(
                "a hunt without a point that found nothing after its drop-off (set_state 2)",
            ));
        }
        let mover = step.w.parts.mover_mut(self.id).ok_or(HUNTER_UNMOVING)?;
        let walked = hunt.search_walk(mover, &walks, &step.w.model.tuning.move_to);
        actions.list.set_hunt(Some(*hunt));
        walked
    }
}
