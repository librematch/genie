//! The hunter's part of its update: the hunt's targets checked, a lost tree, and the state the
//! hunt is in (the drop-off's wait and walk, the failure, the walks to a point, the search, the
//! idle, the lead).

use crate::GenieError;
use crate::combat::damage::Victim;
use crate::combat::victims::readable;
use crate::gathering::DropSiteId;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::NO_HUNT;
use crate::gathering::hunt::{DropOff, Dropped, PointWalk, PointWalker};
use crate::movement::move_to::MoveTo;
use crate::movement::{Me, footprint_gap_squared};
use crate::objects::ObjectState;
use crate::objects::UpdateCode;
use crate::objects::layers::Action;
use crate::objects::masters::MasterTables;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::object_master;
use crate::objects::unit::{Unit, second_target_checked};
use crate::players::Amount;
use crate::unit_ai::ActionKind;
use crate::unit_ai::NotifyKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::Actor;
use crate::unit_ai::task_finder::drops_at;
use crate::unit_ai::task_finder::world::state_of;
use crate::world::HUNTER_UNMOVING;
use crate::world::Seconds;
use crate::world::lets::release;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::world::{SEARCH_AGAIN, TREE_SEARCH_WITHIN};

impl Unit {
    /// The Hunt's update (`0x140ac7a00`) first checks its targets: one gone from the world's
    /// table is dropped (slot 0x68 with none), one past state 6 is let go (slot 0x180, its
    /// `+0x60` being 0) and dropped; then the second target ([`second_target_checked`]).
    pub(crate) fn hunt_targets(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        if let Some(target) = actions
            .list
            .hunt()
            .map(|h| h.target)
            .filter(|&t| t.is_some())
        {
            let state = readable(c, target)?.map(|(_, u, _)| u.life.state);
            let gone = match state {
                Some(s) => ObjectState::Undead < s,
                None if c.w.spatial.get(target).is_some() => {
                    return Err(GenieError::unsupported(
                        "a hunt's target the slice cannot read",
                    ));
                }
                None => true,
            };
            if gone {
                if state.is_some() {
                    let by = self.releaser(c)?;
                    release(c, self, target, by)?;
                }
                if let Some(h) = actions.list.hunt_mut() {
                    h.set_target(None);
                }
            }
        }
        if let Some(site) = actions.list.hunt().map(|h| h.carry_to) {
            let held = c.w.spatial.get(site).is_some();
            let (kept, let_go) = second_target_checked(
                site,
                held,
                state_of(
                    site.map(DropSiteId::id),
                    OthersRef::of(&c.w.players, &c.w.index, c.player)?,
                ),
            );
            if let Some(h) = actions.list.hunt_mut() {
                h.carry_to = kept;
            }
            if let Some(t) = let_go {
                self.hunt_release(c, [Some(t.id())])?;
            }
        }
        Ok(())
    }

    /// A hunt whose target went on the walk to it (state 4: its quiet `MoveTo`'s first check
    /// finds the target it holds past state 6, code 5), or whose `MoveTo` failed (code 3), and
    /// whose class is a tree's: it searches for the next tree (`FUN_140ac4670` with 8 and -1
    /// both times).
    pub(crate) fn tree_lost(&self, actions: &Action, c: &Step<'_>) -> bool {
        actions.list.hunt().is_some_and(|h| {
            matches!(h.state, HuntState::Walk { .. })
                && Seconds::ZERO <= h.timer
                && h.state.move_to().is_some_and(|m| {
                    m.state == MoveTo::Failed
                        || m.target.is_some_and(|t| {
                            c.w.object_state(t).is_some_and(|s| ObjectState::Undead < s)
                        })
                })
        }) && c.w.parts.unit_ai.get(self.id).is_some_and(|ai| {
            let class = ai
                .target_class
                .or_else(|| actions.list.hunt().and_then(|h| h.target_class));
            class == Some(class::TREE)
        })
    }

    /// The Hunt's part of the hunter's update after its targets' check, by the state the hunt is
    /// in, in the update's order of its tests: the drop-off's wait ([`Unit::hunt_drop_wait`]) and
    /// walk ([`Unit::hunt_drop_walk`]), the failure ([`Unit::hunt_failed`]), the walks to a point
    /// ([`Unit::hunt_point_walk`]), the search ([`Unit::hunt_search_wait`]), the idle
    /// ([`Unit::hunt_idle`]), the wait after a given-up `MoveTo` ([`Unit::hunt_wait_walk`]), a
    /// lead without its target, a lost tree's search, else its strike ([`Unit::hunt_strike`]).
    /// Nothing without a hunt. Its code: the failure's 3, the end after a drop-off's 1.
    pub(crate) fn hunt_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        own: &mut Option<Victim>,
        tree_lost: bool,
    ) -> Result<HeadCode, GenieError> {
        let (state, timer, target) = actions
            .list
            .hunt()
            .map_or((None, Seconds::ZERO, None), |h| {
                (Some(h.state), h.timer, h.target)
            });
        match state {
            Some(HuntState::Drop { .. }) if timer < Seconds::ZERO => {
                self.hunt_drop_wait(actions, c)?;
            }
            Some(HuntState::Drop { .. }) => return self.hunt_drop_walk(actions, c, me),
            // State 13: the hunt failed; its next update tells its unit AI (notify 0x1f9) and ends it.
            Some(HuntState::Failed) => return self.hunt_failed(c),
            Some(HuntState::WalkPoint { .. } | HuntState::WalkSearch { .. }) => {
                self.hunt_point_walk(actions, c, me)?;
            }
            Some(HuntState::Search) => self.hunt_search_wait(actions, c)?,
            Some(HuntState::Idle) => {
                actions.hunt_idle()?;
            }
            Some(HuntState::Walk { .. }) if target.is_none() && timer < Seconds::ZERO => {
                self.hunt_wait_walk(actions, c)?;
            }
            Some(HuntState::Lead) if target.is_none() => {
                // state 10 without its target (0x140ac7a00 case 9): set_state(3), the search
                actions.list.hunt_mut().ok_or(NO_HUNT)?.state = HuntState::Search;
                self.hunt_set_search(actions, c)?;
            }
            // the hunt took the next tree (slot 0x48): its MoveTo's code 5 ends the update
            _ if tree_lost
                && self.hunt_tree_search(actions, c, Some(TREE_SEARCH_WITHIN), None)? => {}
            Some(_) => self.hunt_strike(actions, c, me, own, tree_lost)?,
            None => {}
        }
        Ok(HeadCode::Keeps)
    }

    /// Case 7 waiting to try again (state 8 with its timer below 0): the timer runs by dt; at its
    /// end the drop site goes (`FUN_140a7a190` with none: slot 0x180) and `set_state(8)` searches
    /// again ([`Unit::hunt_drop_off`]).
    pub(crate) fn hunt_drop_wait(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let hunt = actions.list.hunt_mut().ok_or(NO_HUNT)?;
        hunt.timer += c.dt.seconds();
        if Seconds::ZERO <= hunt.timer {
            hunt.timer = Seconds::ZERO;
            let site = hunt.carry_to.take();
            self.hunt_release(c, [site.map(DropSiteId::id)])?;
            self.hunt_drop_off(actions, c)?;
        }
        Ok(())
    }

    /// State 8, which has no target: its drop site (the second target) is what it walks to
    /// ([`crate::gathering::hunt::HuntAction::update_drop`]); a load dropped goes to the player
    /// at once ([`Step::give_load`]) and the hunt walks back ([`Unit::hunt_after_drop`]); a
    /// drop site that does not take it goes and the drop-off searches again. Its idling
    /// (`set_state` 2) is not modelled.
    pub(crate) fn hunt_drop_walk(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<HeadCode, GenieError> {
        let mut hunt = *actions.list.hunt().ok_or(NO_HUNT)?;
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
        )?;
        let me_at =
            c.w.spatial
                .get_known(self.id)?
                .ok_or(GenieError::unsupported("a hunter without a place"))?;
        let (me_position, me_radius) = (me_at.position, me_at.radius);
        let site = match c.w.spatial.get_known(hunt.carry_to)? {
            Some(s) => {
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
                Some(DropOff {
                    takes: drops_at(&c.w.drop_sites, s.master, &actor),
                    gap_squared: footprint_gap_squared(
                        me_position,
                        me_radius,
                        s.position,
                        s.radius,
                    ),
                })
            }
            None => None,
        };
        let who = PointWalker {
            player_type: c.w.players.player(c.player)?.kind,
            villager: master.class == class::VILLAGER,
            stance: c
                .w
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
        };
        let dropped = hunt.update_drop(c, self, me, site, who);
        if let Some(h) = actions.list.hunt_mut() {
            *h = hunt;
        }
        let dropped = dropped?;
        match dropped {
            Dropped::Going => {}
            Dropped::Load(kind, amount) => {
                // the player's slot 0xf8 (FUN_141616820) with 1 and 1: the resource, and its
                // gathered total (166 to 168; gold's 49 and 169); the statistics (slot 0xb8) and
                // the local player's tracker (FUN_14097daf0) are not held; a sharing player's
                // allies' share (+0x1426) is not modelled
                c.give_load(kind, amount)?;
                return self.hunt_after_drop(actions, c);
            }
            Dropped::Again => {
                // the drop site goes (FUN_140a7a190 with none), then set_state(8)
                let hunt = actions.list.hunt_mut().ok_or(NO_HUNT)?;
                let site = hunt.carry_to.take();
                self.hunt_release(c, [site.map(DropSiteId::id)])?;
                self.hunt_drop_off(actions, c)?;
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
    pub(crate) fn hunt_failed(&self, c: &mut Step<'_>) -> Result<HeadCode, GenieError> {
        self.post_own(
            &mut c.w.parts.unit_ai,
            NotifyKind::ACTION_FAILED,
            [ActionKind::Hunt.value(), 0, 0],
        )?;
        Ok(HeadCode::of(UpdateCode::FAILED))
    }

    /// States 15 and 11: the walk to a point
    /// ([`crate::gathering::hunt::HuntAction::update_point_walk`]); state 15's end is
    /// `set_state(2)`: the unit takes the hunt's sprite to stand with (slot 0x68 with
    /// `FUN_140ac5210`'s), the target and the second target go; state 11's is `set_state(3)`, the
    /// search ([`Unit::hunt_search_state`]).
    pub(crate) fn hunt_point_walk(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
    ) -> Result<(), GenieError> {
        let mut hunt = *actions.list.hunt().ok_or(NO_HUNT)?;
        let master = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
        )?;
        let who = PointWalker {
            player_type: c.w.players.player(c.player)?.kind,
            villager: master.class == class::VILLAGER,
            stance: c
                .w
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
        };
        let walked = hunt.update_point_walk(c, self, me, who);
        if let Some(h) = actions.list.hunt_mut() {
            *h = hunt;
        }
        let walked = walked?;
        match walked {
            PointWalk::Walking => {}
            PointWalk::Search => self.hunt_search_state(actions, c)?,
            PointWalk::Done { target, second } => {
                let task = hunt.task;
                self.hunt_stand(c, &task)?;
                self.hunt_release(c, [target, second])?;
            }
        }
        Ok(())
    }

    /// State 3 (case 2): the timer runs by dt; at its end it starts again at −10 s and the unit
    /// AI is told again (notify 0x202, [`Unit::hunt_search_notify`]).
    pub(crate) fn hunt_search_wait(
        &self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let hunt = actions.list.hunt_mut().ok_or(NO_HUNT)?;
        if hunt.timer < Seconds::ZERO {
            hunt.timer += c.dt.seconds();
            if Seconds::ZERO <= hunt.timer {
                hunt.timer = SEARCH_AGAIN;
                self.hunt_search_notify(actions, c)?;
            }
        }
        Ok(())
    }

    /// Case 3 waiting after a given-up `MoveTo`, its target gone: the timer runs by dt, then the
    /// walk to the point (`set_state` 11, [`crate::gathering::hunt::HuntAction::wait_then_walk`]).
    pub(crate) fn hunt_wait_walk(
        &self,
        actions: &mut Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let walks = self.move_walks(
            c.w.parts.mover_of(self.id),
            MasterTables::new(&c.w.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.graphics,
                c.w.parts.combat_of(self.id).abilities.look.as_ref(),
            ),
        )?;
        let hunt = actions.list.hunt_mut().ok_or(NO_HUNT)?;
        let mover = c.w.parts.mover_mut(self.id).ok_or(HUNTER_UNMOVING)?;
        hunt.wait_then_walk(c.dt.seconds(), mover, &walks)
    }
}

impl Action {
    /// State 2 (case 1): a done hunt waits; one of a tree's target class would search again
    /// (`set_state` 3), which is not modelled.
    pub(crate) fn hunt_idle(&self) -> Result<(), GenieError> {
        if self
            .list
            .hunt()
            .is_some_and(|h| h.target_class == Some(class::TREE))
        {
            return Err(GenieError::unsupported(
                "a done hunt of a tree's class (set_state 3)",
            ));
        }
        Ok(())
    }
}
