//! One update (slot 6, `0x140ac7a00`), a target lost on the way and the attack on live
//! livestock (state 12).

use crate::combat::attack::AttackerFlags;
use crate::combat::attack::BeginLayer;
use crate::combat::attack::{Attack, Fight, set_walker_anim};
use crate::combat::victims::{file_kill, write_back};
use crate::gathering::hunt::HuntGather;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::{Herd, HuntAction, HuntNext, HuntReads};
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::MoveStart;
use crate::movement::Speed;
use crate::movement::move_to::GiveUp;
use crate::movement::move_to::Patience;
use crate::movement::move_to::SubCode;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::world::HUNTER_UNMOVING;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};

impl HuntAction {
    /// One update (slot 6, `0x140ac7a00`) of `unit`'s hunt on the step, the hunt copied out of
    /// its list: state 12 updates the Attack (`FUN_140a992b0`); its end on the dead target (code
    /// 5) runs `FUN_140ac44b0` and `set_state(4)`. State 4 updates the `MoveTo`; its end (code 1)
    /// runs `FUN_140ac44b0` and `set_state(6)`. State 6 faces the target (`FUN_141162db0`) and,
    /// while it holds something, `set_state(7)`; state 7 gathers. Its notifies, the target it
    /// lets go and the target's changes reach the world at once; what follows that reads the
    /// hunt in its list is the caller's ([`HuntNext`]). A computer player's hunt and the other
    /// codes are not modelled.
    ///
    /// # Errors
    ///
    /// What the update does not model.
    pub(crate) fn update(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        me: Me,
        fight: &mut Fight,
        hunt_reads: &HuntReads,
    ) -> GenieResult<HuntNext> {
        let roles = &step.world.model.roles.clone();
        if hunt_reads.attack.player_kind.is_computer() {
            return Err(GenieError::unsupported(
                "a computer player's hunt (FUN_140fde4f0)",
            ));
        }
        match self.state {
            HuntState::Attack { .. } => {
                self.update_attack(step, unit, actions, me, fight, hunt_reads)
            }
            // case 3: a hunt that gave up on its MoveTo waits (its timer below 0), then set_state 11
            HuntState::Walk { .. } if self.timer < Seconds::ZERO => {
                let walks = hunt_reads.walks(
                    &step.world.model.graphics,
                    (unit, step.world.parts.mover_of(unit.id)),
                );
                let mover = step.world.parts.mover_mut(unit.id).ok_or(HUNTER_UNMOVING)?;
                self.wait_then_walk(
                    step.dt.seconds(),
                    mover,
                    &walks,
                    &step.world.model.tuning.move_to,
                )?;
                Ok(HuntNext::Stay)
            }
            HuntState::Walk { .. } => self.update_walk(step, unit, actions, me, fight, hunt_reads),
            HuntState::Lead => self.update_lead(step, unit, fight, hunt_reads, roles),
            HuntState::Face => self.update_face(step, unit, fight, hunt_reads),
            // case 6 without a target (or a task): set_state(8) to carry the load off, set_state(3)
            // with none
            HuntState::Gather if self.target.is_none() => {
                Ok(if self.ends_without_target(unit.life.amount) {
                    HuntNext::Drop
                } else {
                    HuntNext::Search
                })
            }
            HuntState::Gather => {
                let gathered = self.gather(
                    step.dt.seconds(),
                    unit,
                    &mut fight.victim,
                    hunt_reads,
                    roles,
                )?;
                write_target_back(step, unit, actions, fight, roles)?;
                Ok(match gathered {
                    HuntGather::DropOff => HuntNext::Drop,
                    HuntGather::GoesOn => HuntNext::Stay,
                })
            }
            _ => Err(GenieError::unsupported(
                "a Hunt action in a state the slice does not model",
            )),
        }
    }

    /// State 12, the Attack's update (`FUN_140a992b0`) and its end: ended (codes 1, 2 and 5)
    /// with no target left the hunt searches (`set_state(3)`); a predator (class 10) of a unit AI
    /// with orders queued (`FUN_140c564a0` above 0) would end the hunt (`set_state(1)`) under
    /// the game's setting `+0x98`, not modelled; else the decay check (`FUN_140ac44b0`) and the
    /// walk to the target (`set_state(4)`). Failed (3, its `MoveTo` did) the caller's search for
    /// the next tree follows, else `set_state(3)`; the target gone inside another (code 4,
    /// `LAB_140ac7ecd`) is let go at once and the hunt walks to its point to search there
    /// (`set_state(11)`).
    fn update_attack(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        me: Me,
        fight: &mut Fight,
        hunt_reads: &HuntReads,
    ) -> GenieResult<HuntNext> {
        let roles = &step.world.model.roles.clone();
        let attack =
            self.state
                .attack_mut()
                .and_then(Option::as_mut)
                .ok_or(GenieError::unsupported(
                    "a Hunt in state 12 without its Attack",
                ))?;
        match attack
            .update(step, unit, actions, me, fight, &hunt_reads.attack)?
            .get()
        {
            0 | 6.. => Ok(HuntNext::Stay),
            1 | 2 | 5 if self.target.is_none() => {
                self.tear_down(step.world.parts.mover_mut(unit.id))?;
                self.state = HuntState::Search;
                Ok(HuntNext::Search)
            }
            1 | 2 | 5
                if hunt_reads.predator_hunts_end
                    && roles
                        .of_class(fight.victim.class)
                        .contains(ClassRoles::PREDATOR)
                    && !hunt_reads.attack.flags.contains(AttackerFlags::QUEUE_EMPTY) =>
            {
                Err(GenieError::unsupported(
                    "a hunt that ends with its Attack on a predator (set_state 1)",
                ))
            }
            1 | 2 | 5 => {
                self.decay_check(step, unit, actions, hunt_reads.decay, fight, roles)?;
                let walks = hunt_reads.walks(
                    &step.world.model.graphics,
                    (unit, step.world.parts.mover_of(unit.id)),
                );
                let player = fight.own.player;
                self.set_walk(
                    (&step.world.spatial, unit.position()),
                    step.world.parts.motion_mut(unit.id),
                    &fight.victim,
                    player,
                    &walks,
                    &step.world.model,
                )?;
                Ok(HuntNext::Stay)
            }
            3 => Ok(HuntNext::AttackFailed),
            _ => {
                let target = self.let_go();
                unit.hunt_release(step, [target])?;
                let walks = hunt_reads.walks(
                    &step.world.model.graphics,
                    (unit, step.world.parts.mover_of(unit.id)),
                );
                let mover = step.world.parts.mover_mut(unit.id).ok_or(HUNTER_UNMOVING)?;
                self.set_point_walk(
                    |move_to| HuntState::WalkSearch { move_to },
                    mover,
                    &walks,
                    &step.world.model.tuning.move_to,
                )?;
                Ok(HuntNext::Stay)
            }
        }
    }

    /// State 4, the walk's `MoveTo` (case 3, `FUN_140a992b0`, [`HuntAction::update_list`]): its
    /// code 0 walks or waits; 1 and 2 (the `MoveTo` ended: the list drops it, its destructor
    /// stops the unit) are the decay check and `set_state(6)`
    /// ([`HuntAction::walk_arrived`]); 3 failed ([`HuntAction::walk_failed`]); 4 and 5, its
    /// target let go, walk to the point ([`HuntAction::lost_on_the_way`]). A quiet `MoveTo`
    /// never gives 4, and none gives 6 (the game's `set_state(13)` with the search's notify).
    fn update_walk(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        me: Me,
        fight: &mut Fight,
        hunt_reads: &HuntReads,
    ) -> GenieResult<HuntNext> {
        let roles = &step.world.model.roles.clone();
        let gives_up = GiveUp {
            patience: if !hunt_reads.attack.player_kind.is_computer()
                && roles
                    .of_class(fight.own.class)
                    .contains(ClassRoles::VILLAGER)
            {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: hunt_reads.attack.stance,
        };
        match self.update_list(step, unit, me, gives_up)? {
            SubCode::UnderWay => Ok(HuntNext::Stay),
            SubCode::Done => self.walk_arrived(step, unit, actions, fight, hunt_reads, roles),
            SubCode::Failed => self
                .walk_failed(step.world.parts.motion_mut(unit.id), hunt_reads, roles)
                .map(|()| HuntNext::Stay),
            SubCode::TargetGone => self
                .lost_on_the_way(step, unit, hunt_reads, roles)
                .map(|()| HuntNext::Stay),
        }
    }

    /// The failed `MoveTo`'s update returns 3 (quiet: no notify): `FUN_140ac4670` with 8 and -1
    /// looks for another target only for a class the unit AI's slot 0x1b0 gives 0
    /// (`FUN_140c1c4f0`: 15, trees; the class its unit AI targets, else the hunt's: the caller's
    /// search, `Unit::hunt_tree_search`, which found none); then the hunt waits 3 seconds and the
    /// unit's animation stops (`FUN_141161850`).
    fn walk_failed(
        &mut self,
        motion: MotionMut<'_>,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.no_tree_search(hunt_reads, roles)?;
        self.timer = Seconds::new(-3.0);
        set_walker_anim(motion, Speed::new(0.0));
        Ok(())
    }

    /// The `MoveTo` arrived: its update in state 1 returns 1 (the list deletes it, and its
    /// destructor `0x140aa1f30` stops the unit, slot 0x5d0); the decay check (`FUN_140ac44b0`)
    /// and `set_state(6)`.
    fn walk_arrived(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut Action,
        fight: &mut Fight,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<HuntNext> {
        self.tear_down(step.world.parts.mover_mut(unit.id))?;
        // a target the hunt let go (the `MoveTo`'s went from the world): `FUN_140ac44b0` does
        // nothing, and `set_state(6)` without a target only shows the walking sprite
        if self.target.is_none() {
            self.begin_face(step, unit, hunt_reads);
            return Ok(HuntNext::Stay);
        }
        self.decay_check(step, unit, actions, hunt_reads.decay, fight, roles)?;
        self.set_face(step, unit, actions, fight, hunt_reads, roles)
    }

    /// Case 5, the face (`FUN_141162db0`; a unit without a turn time turns at once, by the
    /// facets between; one that turns over time is not modelled): a target holding something
    /// is gathered (`set_state(7)`); an empty one goes (slot 0x68 with none, the caller's) and
    /// the hunt searches (`set_state(3)`).
    fn update_face(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        fight: &Fight,
        hunt_reads: &HuntReads,
    ) -> GenieResult<HuntNext> {
        self.face_animal(
            (unit.position(), step.world.parts.mover_mut(unit.id)),
            fight.victim.position,
            hunt_reads.attack.world_time,
        )?;
        if Amount::ZERO < fight.victim.amount {
            self.set_gather(
                unit,
                step.world.parts.motion_mut(unit.id),
                step.world.streams.get_mut(Stream::Sprite),
                hunt_reads,
                step.world.model.tuning.sprite,
            );
            return Ok(HuntNext::Stay);
        }
        self.state = HuntState::Search;
        Ok(HuntNext::Search)
    }

    /// State 4's answer to its `MoveTo` losing the target (codes 4 and 5): `FUN_140ac4670` with 8
    /// and -1 looks for another target of a class the unit AI's slot 0x1b0 gives 0 (trees: the
    /// caller's search, `Unit::hunt_tree_search`, which found none); then the target goes (slot
    /// 0x68 with none) and the hunt walks to its point (`set_state` 11).
    pub(super) fn lost_on_the_way(
        &mut self,
        step: &mut Step<'_>,
        unit: &Unit,
        hunt_reads: &HuntReads,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.no_tree_search(hunt_reads, roles)?;
        self.set_target(None);
        let walks = hunt_reads.walks(
            &step.world.model.graphics,
            (unit, step.world.parts.mover_of(unit.id)),
        );
        let mover = step.world.parts.mover_mut(unit.id).ok_or(HUNTER_UNMOVING)?;
        self.set_point_walk(
            |move_to| HuntState::WalkSearch { move_to },
            mover,
            &walks,
            &step.world.model.tuning.move_to,
        )
    }

    /// `FUN_140ac4670` with 8 and −1 for a class the unit AI's slot 0x1b0 gives 0 (trees, the
    /// role [`ClassRoles::TREE`]: the class its unit AI targets, else the hunt's): the caller's
    /// search (`Unit::hunt_tree_search`) found none; another one here is not modelled.
    fn no_tree_search(&self, hunt_reads: &HuntReads, roles: &Roles) -> GenieResult<()> {
        if hunt_reads.has_ai {
            let class = hunt_reads.ai_class.or(self.target_class);
            if roles.of_some_class(class).contains(ClassRoles::TREE) && !hunt_reads.tree_searched {
                return Err(GenieError::unsupported(
                    "a hunt's search for another tree (FUN_140ac4670)",
                ));
            }
        }
        Ok(())
    }

    /// `set_state(12)` on live livestock: the Attack of the master's ranges starts (with its
    /// target in reach its point and the facing, [`Attack::start`]), showing the idle sprite when
    /// its `MoveTo` starts arrived.
    pub(super) fn start_attack(
        &mut self,
        step: &mut Step<'_>,
        unit: &mut Unit,
        herd: &Herd,
        roles: &Roles,
    ) -> GenieResult<()> {
        let reads = unit.move_to_reads(
            step.world.top_speed(step.player, unit)?,
            MasterTables::new(&step.world.model.masters, &step.world.parts.own_master),
            step.player,
            (
                &step.world.model.graphics,
                step.world.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            roles,
        )?;
        let mover = step.world.parts.mover_mut(unit.id).ok_or(HUNTER_UNMOVING)?;
        self.state = HuntState::Attack { attack: None };
        let (attack, move_start) = Attack::start(
            herd.target.id(),
            &herd.engagement,
            herd.arms,
            mover,
            &reads,
            BeginLayer {
                roles,
                attack_tuning: &step.world.model.tuning.attack,
                move_to_tuning: &step.world.model.tuning.move_to,
            },
        )?;
        if move_start == MoveStart::Arrived {
            attack.arrived_sprite(
                unit,
                step.world.parts.motion_mut(unit.id),
                step.world.streams.get_mut(Stream::Sprite),
                step.world.model.tuning.sprite,
            )?;
        }
        self.state = HuntState::Attack {
            attack: Some(attack),
        };
        Ok(())
    }
}

/// The hunt's target back to the world where its own part changed it (its gathering, its
/// decay's stop), and a kill it left ([`crate::combat::victims::write_back`]).
///
/// # Errors
///
/// The write-back's and the kill's.
pub(super) fn write_target_back(
    step: &mut Step<'_>,
    hunter: &mut Unit,
    actions: &mut Action,
    fight: &mut Fight,
    roles: &Roles,
) -> GenieResult<()> {
    if let Some(kill_left) = write_back(step, &mut fight.victim)? {
        file_kill(step, (hunter, actions), kill_left, roles)?;
    }
    Ok(())
}
