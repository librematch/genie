//! One update (slot 6, `0x140ac7a00`), a target lost on the way and the attack on live
//! livestock (state 12).

use crate::GenieError;
use crate::combat::attack::{Attack, Fight, set_walker_anim};
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::walks::QUIET_CODE;
use crate::gathering::hunt::{Herd, HuntAction, HuntNext, HuntReads};
use crate::math::rng::Stream;
use crate::movement::Me;
use crate::movement::Speed;
use crate::movement::move_to::GiveUp;
use crate::objects::layers::MotionMut;
use crate::objects::masters::class;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::world::Seconds;
use crate::world::step::Step;

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
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut crate::objects::layers::Action,
        me: Me,
        f: &mut Fight,
        r: &HuntReads,
    ) -> Result<HuntNext, GenieError> {
        if r.attack.computer {
            return Err(GenieError::unsupported(
                "a computer player's hunt (FUN_140fde4f0)",
            ));
        }
        match self.state {
            HuntState::Attack => self.update_attack(c, unit, actions, me, f, r),
            // case 3: a hunt that gave up on its MoveTo waits (its timer below 0), then set_state 11
            HuntState::Walk if self.timer < Seconds::ZERO => {
                let walks = r.walks(&c.w.graphics, (unit, c.w.parts.mover_of(unit.id)));
                let mover =
                    c.w.parts
                        .mover_mut(unit.id)
                        .ok_or(crate::world::HUNTER_UNMOVING)?;
                self.wait_then_walk(c.dt.seconds(), mover, &walks)?;
                Ok(HuntNext::Stay)
            }
            HuntState::Walk => self.update_walk(c, unit, actions, me, f, r),
            HuntState::Lead => self.update_lead(c, unit, f, r),
            HuntState::Face => self.update_face(c, unit, f, r),
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
                let full = self.gather(c.dt.seconds(), unit, &mut f.victim, r)?;
                write_target_back(c, unit, actions, f)?;
                Ok(if full { HuntNext::Drop } else { HuntNext::Stay })
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
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut crate::objects::layers::Action,
        me: Me,
        f: &mut Fight,
        r: &HuntReads,
    ) -> Result<HuntNext, GenieError> {
        let attack = self.attack.as_mut().ok_or(GenieError::unsupported(
            "a Hunt in state 12 without its Attack",
        ))?;
        match attack.update(c, unit, actions, me, f, &r.attack)?.get() {
            0 | 6.. => Ok(HuntNext::Stay),
            1 | 2 | 5 if self.target.is_none() => {
                self.tear_down(c.w.parts.mover_mut(unit.id))?;
                self.state = HuntState::Search;
                Ok(HuntNext::Search)
            }
            1 | 2 | 5
                if r.predator_hunts_end
                    && f.victim.class == class::PREDATOR
                    && !r.attack.queue_empty =>
            {
                Err(GenieError::unsupported(
                    "a hunt that ends with its Attack on a predator (set_state 1)",
                ))
            }
            1 | 2 | 5 => {
                self.decay_check(c, unit, actions, r.stops_decay, f)?;
                let walks = r.walks(&c.w.graphics, (unit, c.w.parts.mover_of(unit.id)));
                let player = f.own.player;
                self.set_walk(
                    (&c.w.spatial, unit.position()),
                    c.w.parts.motion_mut(unit.id),
                    &f.victim,
                    player,
                    &walks,
                )?;
                Ok(HuntNext::Stay)
            }
            3 => Ok(HuntNext::AttackFailed),
            _ => {
                let target = self.let_go();
                unit.hunt_release(c, [target])?;
                let walks = r.walks(&c.w.graphics, (unit, c.w.parts.mover_of(unit.id)));
                let mover =
                    c.w.parts
                        .mover_mut(unit.id)
                        .ok_or(crate::world::HUNTER_UNMOVING)?;
                self.set_point_walk(HuntState::WalkSearch, mover, &walks)?;
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
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut crate::objects::layers::Action,
        me: Me,
        f: &mut Fight,
        r: &HuntReads,
    ) -> Result<HuntNext, GenieError> {
        let gives_up = GiveUp {
            waits: !r.attack.computer && f.own.class == class::VILLAGER,
            stance: r.attack.stance,
        };
        match self.update_list(c, unit, me, gives_up)?.get() {
            0 => Ok(HuntNext::Stay),
            1 | 2 => self.walk_arrived(c, unit, actions, f, r),
            3 => self
                .walk_failed(c.w.parts.motion_mut(unit.id), r)
                .map(|()| HuntNext::Stay),
            4 | 5 => self.lost_on_the_way(c, unit, r).map(|()| HuntNext::Stay),
            code => Err(QUIET_CODE.with("code", code)),
        }
    }

    /// The failed `MoveTo`'s update returns 3 (quiet: no notify): `FUN_140ac4670` with 8 and -1
    /// looks for another target only for a class the unit AI's slot 0x1b0 gives 0
    /// (`FUN_140c1c4f0`: 15, trees; the class its unit AI targets, else the hunt's: the caller's
    /// search, `Unit::hunt_tree_search`, which found none); then the hunt waits 3 seconds and the
    /// unit's animation stops (`FUN_141161850`).
    fn walk_failed(&mut self, motion: MotionMut<'_>, r: &HuntReads) -> Result<(), GenieError> {
        self.no_tree_search(r)?;
        self.timer = Seconds::new(-3.0);
        set_walker_anim(motion, Speed::new(0.0));
        Ok(())
    }

    /// The `MoveTo` arrived: its update in state 1 returns 1 (the list deletes it, and its
    /// destructor `0x140aa1f30` stops the unit, slot 0x5d0); the decay check (`FUN_140ac44b0`)
    /// and `set_state(6)`.
    fn walk_arrived(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        actions: &mut crate::objects::layers::Action,
        f: &mut Fight,
        r: &HuntReads,
    ) -> Result<HuntNext, GenieError> {
        self.tear_down(c.w.parts.mover_mut(unit.id))?;
        // a target the hunt let go (the `MoveTo`'s went from the world): `FUN_140ac44b0` does
        // nothing, and `set_state(6)` without a target only shows the walking sprite
        if self.target.is_none() {
            self.begin_face(c, unit, r);
            return Ok(HuntNext::Stay);
        }
        self.decay_check(c, unit, actions, r.stops_decay, f)?;
        self.set_face(c, unit, actions, f, r)
    }

    /// Case 5, the face (`FUN_141162db0`; a unit without a turn time turns at once, by the
    /// facets between; one that turns over time is not modelled): a target holding something
    /// is gathered (`set_state(7)`); an empty one goes (slot 0x68 with none, the caller's) and
    /// the hunt searches (`set_state(3)`).
    fn update_face(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        f: &Fight,
        r: &HuntReads,
    ) -> Result<HuntNext, GenieError> {
        self.face_animal(
            (unit.position(), c.w.parts.mover_mut(unit.id)),
            f.victim.position,
            r.attack.world_time,
        )?;
        if Amount::ZERO < f.victim.amount {
            self.set_gather(
                unit,
                c.w.parts.motion_mut(unit.id),
                c.w.streams.get(Stream::Sprite),
                r,
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
        c: &mut Step<'_>,
        unit: &Unit,
        r: &HuntReads,
    ) -> Result<(), GenieError> {
        self.no_tree_search(r)?;
        self.set_target(None);
        let walks = r.walks(&c.w.graphics, (unit, c.w.parts.mover_of(unit.id)));
        let mover =
            c.w.parts
                .mover_mut(unit.id)
                .ok_or(crate::world::HUNTER_UNMOVING)?;
        self.set_point_walk(HuntState::WalkSearch, mover, &walks)
    }

    /// `FUN_140ac4670` with 8 and −1 for a class the unit AI's slot 0x1b0 gives 0 (trees, the
    /// class [`class::TREE`]: the class its unit AI targets, else the hunt's): the caller's
    /// search (`Unit::hunt_tree_search`) found none; another one here is not modelled.
    fn no_tree_search(&self, r: &HuntReads) -> Result<(), GenieError> {
        if r.has_ai {
            let class = r.ai_class.or(self.target_class);
            if class == Some(class::TREE) && !r.tree_searched {
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
        c: &mut Step<'_>,
        unit: &mut Unit,
        herd: &Herd,
    ) -> Result<(), GenieError> {
        let reads = unit.move_to_reads(
            c.w.parts.motion.mover(unit.id),
            crate::objects::masters::MasterTables::new(&c.w.masters, &c.w.parts.own_master),
            c.player,
            (
                &c.w.graphics,
                c.w.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        )?;
        let mover =
            c.w.parts
                .mover_mut(unit.id)
                .ok_or(crate::world::HUNTER_UNMOVING)?;
        self.move_to = None;
        self.state = HuntState::Attack;
        // the herd is read only on a live target
        let target = self.target.ok_or(GenieError::unreachable(
            "a hunt's Attack on its livestock without a target",
        ))?;
        let (a, arrived) = Attack::start(target, &herd.engagement, herd.arms, mover, &reads)?;
        if arrived {
            a.arrived_sprite(
                unit,
                c.w.parts.motion_mut(unit.id),
                c.w.streams.get(Stream::Sprite),
            )?;
        }
        self.attack = Some(a);
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
    c: &mut Step<'_>,
    hunter: &mut Unit,
    actions: &mut crate::objects::layers::Action,
    f: &mut Fight,
) -> Result<(), GenieError> {
    if let Some(k) = crate::combat::victims::write_back(c, &mut f.victim)? {
        crate::combat::victims::file_kill(c, (hunter, actions), k)?;
    }
    Ok(())
}
