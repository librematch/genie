//! The hunter's part of its update: the hunt's targets checked, a lost tree, and the state the
//! hunt is in (the drop-off's wait and walk, the failure, the walks to a point, the search, the
//! idle, the lead).

use crate::combat::damage::Victim;
use crate::combat::victims::readable;
use crate::gathering::DropSiteId;
use crate::gathering::hunt::HuntAction;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::TreeSearch;
use crate::gathering::hunt::{DropOff, Dropped, PointWalk, PointWalker};
use crate::map::Distance;
use crate::movement::move_to::MoveTo;
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::MasterTables;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Presence;
use crate::objects::unit::object_master;
use crate::objects::unit::{Unit, second_target_checked};
use crate::players::Amount;
use crate::unit_ai::AiAction;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::drops_at;
use crate::unit_ai::task_finder::world::state_of;
use crate::world::HUNTER_UNMOVING;
use crate::world::SEARCH_AGAIN;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl Unit {
    /// The Hunt's update (`0x140ac7a00`) first checks its targets: one gone from the world's
    /// table is dropped (slot 0x68 with none), one past state 6 is let go (slot 0x180, its
    /// `+0x60` being 0) and dropped; then the second target ([`second_target_checked`]).
    pub(crate) fn hunt_targets(
        &mut self,
        hunt: &mut HuntAction,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        let target = hunt.target;
        if target.is_some() {
            let state = readable(step, target)?.map(|(_, unit, _)| unit.life.state);
            let gone = match state {
                Some(object_state) => ObjectState::Undead < object_state,
                None if step.world.spatial.get(target).is_some() => {
                    return Err(GenieError::unsupported(
                        "a hunt's target the slice cannot read",
                    ));
                }
                None => true,
            };
            if gone {
                if state.is_some() {
                    let by = self.releaser(step)?;
                    release(step, self, target, by)?;
                }
                hunt.set_target(None);
            }
        }
        let site = hunt.carry_to;
        let held = Presence::of_found(step.world.spatial.get(site));
        let (kept, let_go) = second_target_checked(
            site,
            held,
            state_of(
                site.map(DropSiteId::id),
                OthersRef::of(&step.world.players, &step.world.index, step.player)?,
            ),
        );
        hunt.carry_to = kept;
        if let Some(role_id) = let_go {
            self.hunt_release(step, [Some(role_id.id())])?;
        }
        Ok(())
    }

    /// A hunt whose target went on the walk to it (state 4: its quiet `MoveTo`'s first check
    /// finds the target it holds past state 6, code 5), or whose `MoveTo` failed (code 3), and
    /// whose class is a tree's: it searches for the next tree (`FUN_140ac4670` with 8 and -1
    /// both times).
    pub(crate) fn tree_lost(&self, hunt: &HuntAction, step: &Step<'_>, roles: &Roles) -> bool {
        matches!(hunt.state, HuntState::Walk { .. })
            && Seconds::ZERO <= hunt.timer
            && hunt.state.move_to().is_some_and(|move_to_action| {
                move_to_action.state == MoveTo::Failed
                    || move_to_action.target.is_some_and(|object_id| {
                        step.world
                            .object_state(object_id)
                            .is_some_and(|object_state| ObjectState::Undead < object_state)
                    })
            })
            && step.world.parts.unit_ai.get(self.id).is_some_and(|ai| {
                let class = ai.target_class.or(hunt.target_class);
                roles.of_some_class(class).contains(ClassRoles::TREE)
            })
    }

    /// The Hunt's part of the hunter's update after its targets' check, by the state the hunt is
    /// in, in the update's order of its tests: the drop-off's wait ([`Unit::hunt_drop_wait`]) and
    /// walk ([`Unit::hunt_drop_walk`]), the failure ([`Unit::hunt_failed`]), the walks to a point
    /// ([`Unit::hunt_point_walk`]), the search ([`Unit::hunt_search_wait`]), the idle
    /// ([`Action::hunt_idle`](crate::objects::layers::Action::hunt_idle)), the wait after a
    /// given-up `MoveTo` ([`Unit::hunt_wait_walk`]), a lead without its target, a lost tree's
    /// search, else its strike ([`Unit::hunt_strike`]). Its code: the failure's 3, the end
    /// after a drop-off's 1.
    ///
    /// `hunt` is the head's copy, which the list holds as each routine starts and ends: the
    /// game runs the update on the Hunt itself (`this`), and a routine writes the copy back
    /// before a call that takes the action layer, and before an error.
    pub(crate) fn hunt_part(
        &mut self,
        (hunt, actions): (&mut HuntAction, &mut Action),
        step: &mut Step<'_>,
        me: Me,
        (own, tree_lost): (&mut Option<Victim>, bool),
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let (state, timer, target) = (hunt.state, hunt.timer, hunt.target);
        match state {
            HuntState::Drop { .. } if timer < Seconds::ZERO => {
                self.hunt_drop_wait(hunt, actions, step, roles)?;
            }
            HuntState::Drop { .. } => return self.hunt_drop_walk(hunt, actions, step, me, roles),
            // State 13: the hunt failed; its next update tells its unit AI (notify 0x1f9) and ends it.
            HuntState::Failed => return self.hunt_failed(step),
            HuntState::WalkPoint { .. } | HuntState::WalkSearch { .. } => {
                self.hunt_point_walk(hunt, actions, step, me, roles)?;
            }
            HuntState::Search => self.hunt_search_wait(hunt, actions, step)?,
            HuntState::Idle => {
                actions.hunt_idle(roles)?;
            }
            HuntState::Walk { .. } if target.is_none() && timer < Seconds::ZERO => {
                self.hunt_wait_walk(hunt, actions, step, roles)?;
            }
            HuntState::Lead if target.is_none() => {
                // state 10 without its target (0x140ac7a00 case 9): set_state(3), the search
                hunt.state = HuntState::Search;
                actions.list.set_hunt(Some(*hunt));
                self.hunt_set_search(hunt, actions, step, roles)?;
            }
            // the hunt took the next tree (slot 0x48): its MoveTo's code 5 ends the update
            _ if tree_lost
                && self.hunt_tree_search(
                    (hunt, actions),
                    step,
                    Some(Distance::new(
                        step.world.model.tuning.turn.tree_search_within,
                    )),
                    None,
                    roles,
                )? == TreeSearch::Found => {}
            _ => self.hunt_strike((hunt, actions), step, me, (own, tree_lost), roles)?,
        }
        Ok(HeadCode::Keeps)
    }

    /// Case 7 waiting to try again (state 8 with its timer below 0): the timer runs by dt; at its
    /// end the drop site goes (`FUN_140a7a190` with none: slot 0x180) and `set_state(8)` searches
    /// again ([`Unit::hunt_drop_off`]).
    pub(crate) fn hunt_drop_wait(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        hunt.timer += step.dt.seconds();
        if Seconds::ZERO <= hunt.timer {
            hunt.timer = Seconds::ZERO;
            let site = hunt.carry_to.take();
            actions.list.set_hunt(Some(*hunt));
            self.hunt_release(step, [site.map(DropSiteId::id)])?;
            return self.hunt_drop_off(hunt, actions, step, roles);
        }
        actions.list.set_hunt(Some(*hunt));
        Ok(())
    }

    /// State 8, which has no target: its drop site (the second target) is what it walks to
    /// ([`crate::gathering::hunt::HuntAction::update_drop`]); a load dropped goes to the player
    /// at once ([`Step::give_load`]) and the hunt walks back ([`Unit::hunt_after_drop`]); a
    /// drop site that does not take it goes and the drop-off searches again. Its idling
    /// (`set_state` 2) is not modelled.
    pub(crate) fn hunt_drop_walk(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let me_at = step
            .world
            .spatial
            .get_known(self.id)?
            .ok_or(GenieError::unsupported("a hunter without a place"))?;
        let (me_position, me_radius) = (me_at.position, me_at.radius);
        let site = match step.world.spatial.get_known(hunt.carry_to)? {
            Some(placed) => {
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
                Some(DropOff {
                    takes: drops_at(&step.world.model.drop_sites, placed.master, &actor),
                    gap_squared: footprint_gap_squared(
                        me_position,
                        me_radius,
                        placed.position,
                        placed.radius,
                    ),
                })
            }
            None => None,
        };
        let who = PointWalker {
            player_kind: step.world.players.player(step.player)?.kind,
            villager: roles.of_class(master.class).contains(ClassRoles::VILLAGER),
            stance: step
                .world
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
        };
        let dropped = hunt.update_drop(step, self, me, site, who);
        actions.list.set_hunt(Some(*hunt));
        let dropped = dropped?;
        match dropped {
            Dropped::Going => {}
            Dropped::Load(kind, amount) => {
                // the player's slot 0xf8 (FUN_141616820) with 1 and 1: the resource, and its
                // gathered total (166 to 168; gold's 49 and 169); the statistics (slot 0xb8) and
                // the local player's tracker (FUN_14097daf0) are not held; a sharing player's
                // allies' share (+0x1426) is not modelled
                step.give_load(kind, amount)?;
                return self.hunt_after_drop(hunt, actions, step, roles);
            }
            Dropped::Again => {
                // the drop site goes (FUN_140a7a190 with none), then set_state(8)
                let site = hunt.carry_to.take();
                actions.list.set_hunt(Some(*hunt));
                self.hunt_release(step, [site.map(DropSiteId::id)])?;
                self.hunt_drop_off(hunt, actions, step, roles)?;
            }
            Dropped::Idle => {
                return Err(GenieError::unsupported(
                    "a hunt's drop-off idling (set_state 2)",
                ));
            }
        }
        Ok(HeadCode::Keeps)
    }

    /// State 13 (`0x140ac7a00` case 0xc): notify 0x1f9 with the hunt's action value, and the
    /// update returns 3, whose deletion lets its target, then its second target go. The failed
    /// state holds no sub-action (its `set_state` dropped them).
    pub(crate) fn hunt_failed(&self, step: &mut Step<'_>) -> GenieResult<HeadCode> {
        self.post_own(
            &mut step.world.parts.unit_ai,
            NotifyKind::ACTION_FAILED,
            [AiAction::Hunt.value(), 0, 0],
        )?;
        Ok(HeadCode::of(UpdateCode::Failed))
    }

    /// States 15 and 11: the walk to a point
    /// ([`crate::gathering::hunt::HuntAction::update_point_walk`]); state 15's end is
    /// `set_state(2)`: the unit takes the hunt's sprite to stand with (slot 0x68 with
    /// `FUN_140ac5210`'s), the target and the second target go; state 11's is `set_state(3)`, the
    /// search ([`Unit::hunt_search_state`]).
    pub(crate) fn hunt_point_walk(
        &mut self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        let who = PointWalker {
            player_kind: step.world.players.player(step.player)?.kind,
            villager: roles.of_class(master.class).contains(ClassRoles::VILLAGER),
            stance: step
                .world
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
        };
        let walked = hunt.update_point_walk(step, self, me, who);
        actions.list.set_hunt(Some(*hunt));
        let walked = walked?;
        match walked {
            PointWalk::Walking => {}
            PointWalk::Search => self.hunt_search_state(hunt, actions, step, roles)?,
            PointWalk::Done { target, second } => {
                let task = hunt.task;
                self.hunt_stand(step, &task)?;
                self.hunt_release(step, [target, second])?;
            }
        }
        Ok(())
    }

    /// State 3 (case 2): the timer runs by dt; at its end it starts again at −10 s and the unit
    /// AI is told again (notify 0x202, [`Unit::hunt_search_notify`]).
    pub(crate) fn hunt_search_wait(
        &self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if hunt.timer < Seconds::ZERO {
            hunt.timer += step.dt.seconds();
            if Seconds::ZERO <= hunt.timer {
                hunt.timer = SEARCH_AGAIN;
                actions.list.set_hunt(Some(*hunt));
                return self.hunt_search_notify(hunt, actions, step);
            }
        }
        actions.list.set_hunt(Some(*hunt));
        Ok(())
    }

    /// Case 3 waiting after a given-up `MoveTo`, its target gone: the timer runs by dt, then the
    /// walk to the point (`set_state` 11, [`crate::gathering::hunt::HuntAction::wait_then_walk`]).
    pub(crate) fn hunt_wait_walk(
        &self,
        hunt: &mut HuntAction,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let walks = self.move_walks(
            (
                step.world.parts.mover_of(self.id),
                step.world.top_speed_terms(step.player, self)?,
            ),
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
            (
                &step.world.model.graphics,
                step.world.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let mover = step.world.parts.mover_mut(self.id).ok_or(HUNTER_UNMOVING)?;
        let walked = hunt.wait_then_walk(
            step.dt.seconds(),
            mover,
            &walks,
            &step.world.model.tuning.move_to,
        );
        actions.list.set_hunt(Some(*hunt));
        walked
    }
}

impl Action {
    /// State 2 (case 1): a done hunt waits; one of a tree's target class would search again
    /// (`set_state` 3), which is not modelled.
    pub(crate) fn hunt_idle(&self, roles: &Roles) -> GenieResult<()> {
        if self.list.hunt().is_some_and(|hunt_action| {
            roles
                .of_some_class(hunt_action.target_class)
                .contains(ClassRoles::TREE)
        }) {
            return Err(GenieError::unsupported(
                "a done hunt of a tree's class (set_state 3)",
            ));
        }
        Ok(())
    }
}
