//! The Hunt's start, its chase and retarget, a move's target and the walk to a point (states
//! 11 and 15).

use crate::GenieError;
use crate::combat::attack::BeginLayer;
use crate::combat::attack::{Arms, Attack, Engagement};
use crate::gathering::DropSiteId;
use crate::gathering::PastureId;
use crate::gathering::gather::Walks;
use crate::gathering::hunt::HuntState;
use crate::gathering::hunt::walks::QUIET_CODE;
use crate::gathering::hunt::{HuntAction, HuntTask, Lead, Quarry};
use crate::gathering::{HuntTargetId, both_food};
use crate::map::Distance;
use crate::map::Position;
use crate::movement::move_to::GiveUp;
use crate::movement::move_to::MoveToAction;
use crate::movement::move_to::Patience;
use crate::movement::move_to::WalkTo;
use crate::movement::{Me, Mover};
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::lifecycle::Held;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::Answer;
use crate::unit_ai::UnitStance;
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::generated::HuntTuning;
use crate::world::layer::generated::MoveToTuning;
use crate::world::step::Step;

/// Who walks to a hunt's point ([`HuntAction::update_point_walk`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointWalker {
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub player_kind: PlayerKind,
    /// The unit is a villager (class 4): its `MoveTo` waits once when the motion step gives up.
    pub villager: bool,
    /// Its unit AI's stance (`+0x1a9`), 0 without one: the wait is a second below 2, else two.
    pub stance: UnitStance,
}

/// What a hunt's walk to its point did in an update ([`HuntAction::update_point_walk`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointWalk {
    /// It walks on, or waits to ask again.
    Walking,
    /// State 15's walk ended or failed: the hunt is done ([`HuntAction::set_done`]), its target
    /// and second target to release.
    Done {
        /// The target it let go, if any.
        target: Option<ObjectId>,
        /// The second target it let go, if any.
        second: Option<ObjectId>,
    },
    /// State 11's walk ended or failed: the hunt searches (`set_state(3)`; the caller tells the
    /// unit AI).
    Search,
}

/// What a hunt's start takes besides its attack and its mover ([`HuntAction::start`]'s arguments).
#[derive(Debug)]
pub struct HuntStart<'a> {
    /// The task.
    pub task: HuntTask,
    /// The target.
    pub target: &'a Quarry,
    /// The owner.
    pub player: PlayerId,
    /// The owner's type (`player+0x10`; [`PlayerKind::is_computer`] is `FUN_141659cf0`).
    pub player_kind: PlayerKind,
    /// The game settings' test `FUN_141302150`.
    pub carry_rule: bool,
    /// What the hunter carries, its resource kind and amount.
    pub held: &'a mut Held,
    /// What its `MoveTo`s' constructors read of the hunter; its load is `held`'s.
    pub walks: Walks<'a>,
}

/// What a hunt's retarget takes besides its attack and its mover ([`HuntAction::retarget`]'s
/// arguments).
#[derive(Clone, Copy, Debug)]
pub struct HuntRetarget<'a> {
    /// The work command's new target.
    pub target: &'a Quarry,
    /// The owner.
    pub player: PlayerId,
    /// What the hunter carries, and what its `MoveTo`s' constructors read of it.
    pub walks: Walks<'a>,
    /// The target is a drop site of the hunter's master.
    pub drops_here: bool,
}

/// What a hunt's chase of its target takes besides its attack and its mover
/// ([`HuntAction::chase`]'s arguments).
#[derive(Clone, Copy, Debug)]
pub struct HuntChase<'a> {
    /// The target.
    pub target: &'a Quarry,
    /// The hunter's owner.
    pub player: PlayerId,
    /// What its `MoveTo`s' constructors read of the hunter.
    pub walks: &'a Walks<'a>,
}

/// A move a hunt may take ([`HuntAction::take_move`]'s arguments before the mover).
#[derive(Clone, Copy, Debug)]
pub struct HuntMove<'a> {
    /// The move's target (none for a point).
    pub target: Option<&'a Quarry>,
    /// The move's point.
    pub point: Position,
    /// The owner.
    pub player: PlayerId,
    /// What the hunter carries, and what its `MoveTo`s' constructors read of it.
    pub walks: Walks<'a>,
    /// The target is a drop site of the hunter's master.
    pub drops_here: bool,
    /// The pasture the unit is on (`mMyPastureId`).
    pub my_pasture: Option<PastureId>,
    /// The gap to a drop site target.
    pub gap: Distance,
}

impl HuntAction {
    /// The factory's constructor (`FUN_140ac2dd0`; its target's add, slot 0x178, is the caller's)
    /// and the install's start (slot 5 with 1, `0x140ac49b0`): a hunter carrying another food
    /// keeps the amount as the task's kind (`FUN_14119d310` on `held`); a loaded hunter sent to
    /// one of its own drop sites carries the load there (`set_state` 8); else `set_state(4)`. A
    /// target of another, unallied player (or of class 49) that is not dead yet is attacked first
    /// (`set_state` 12). `player_type` is the owner's type (`FUN_141659cf0` tests it); `carry_rule` the
    /// game settings' test `FUN_141302150`, with which a hunter drops another resource it carries
    /// (a computer player's does when it carries none).
    pub(crate) fn start(
        start: HuntStart<'_>,
        attack: &Engagement,
        arms: Arms,
        mover: &mut Mover,
        layer: BeginLayer<'_>,
        hunt_tuning: &HuntTuning,
    ) -> Result<(Self, bool), GenieError> {
        let BeginLayer {
            roles,
            attack_tuning,
            move_to_tuning,
        } = layer;
        let HuntStart {
            task,
            target,
            player,
            player_kind,
            carry_rule,
            held,
            walks,
        } = start;
        let mut hunt = Self {
            target: Some(HuntTargetId::new(target.id)),
            state: HuntState::New,
            point: target.position,
            target_class: Some(target.class),
            task,
            drop_site: None,
            carry_to: None,
            timer: Seconds::ZERO,
            lead: Lead::NEW,
        };
        if Amount::ZERO < target.amount {
            let kind = task.resource;
            let both_food = both_food(kind, held.kind);
            let drops = player_kind.is_computer() && held.amount == Amount::ZERO;
            if held.kind != kind {
                if both_food {
                    held.kind = kind;
                } else if drops || carry_rule {
                    *held = Held {
                        kind,
                        amount: Amount::ZERO,
                    };
                }
            }
        }
        let walks = Walks {
            held: *held,
            ..walks
        };
        // the start (slot 5 with 1): an own target among the hunter's master's drop sites
        // (`+0x7a0`, FUN_14109d4f0; a building annex's main building) takes a load there: it
        // becomes the second target (FUN_140a7a190), the target goes (slot 0x68 with none; the
        // caller takes and releases it), the point and `+0x68` clear, `set_state(8)`; a hunter
        // carrying nothing does not get there
        if target.player == player && target.drop_site && Amount::ZERO < held.amount {
            hunt.set_target(None);
            hunt.point = Position::from([-1.0; 3]);
            hunt.target_class = None;
            hunt.carry_to = Some(DropSiteId::new(target.id));
            hunt.set_drop(target.position, attack.gap, mover, &walks, move_to_tuning)?;
            return Ok((hunt, false));
        }
        let chase = HuntChase {
            target,
            player,
            walks: &walks,
        };
        let arrived = hunt.chase(
            chase,
            attack,
            arms,
            mover,
            BeginLayer {
                roles,
                attack_tuning,
                move_to_tuning,
            },
            hunt_tuning,
        )?;
        Ok((hunt, arrived))
    }

    /// `set_state(4)` (`0x140ac60e0` case 4): the sub-actions go (an Attack's `MoveTo` stops the
    /// unit), the timer clears and the target's place becomes the point. A dead target, or the hunter's own
    /// player's livestock, is walked to: a quiet `MoveTo` within the task's range, or
    /// `HuntTuning::own_livestock_range` to the livestock (`FUN_140aa0df0`; its start `0x140aa2100`: a unit
    /// within 0.01 of the target's footprint is there already). A target not dead yet of another,
    /// unallied player (or of class 49) is attacked first: `set_state(12)` (case 0xc) with an
    /// Attack of the master's ranges. A live allied one fails the hunt (`set_state` 13).
    pub(crate) fn chase(
        &mut self,
        chase: HuntChase<'_>,
        attack: &Engagement,
        arms: Arms,
        mover: &mut Mover,
        layer: BeginLayer<'_>,
        hunt_tuning: &HuntTuning,
    ) -> Result<bool, GenieError> {
        let BeginLayer {
            roles,
            attack_tuning,
            move_to_tuning,
        } = layer;
        let HuntChase {
            target,
            player,
            walks,
        } = chase;
        self.tear_down(Some(mover))?;
        self.state = HuntState::Walk { move_to: None };
        self.timer = Seconds::ZERO;
        self.point = target.position;
        let own_livestock =
            roles.of_class(target.class).contains(ClassRoles::LIVESTOCK) && target.player == player;
        if ObjectState::Alive < target.state || own_livestock {
            let range = if ObjectState::Alive < target.state {
                self.task.range
            } else {
                Distance::new(hunt_tuning.own_livestock_range)
            };
            let to = WalkTo {
                id: Some(target.id),
                at: target.position,
                gap: attack.gap,
                state: Some(target.state),
            };
            let move_to = self.move_to_object(to, range, mover, walks, move_to_tuning)?;
            self.state = HuntState::Walk { move_to };
            return Ok(false);
        }
        if !roles.of_class(target.class).contains(ClassRoles::FARM) && target.allied {
            // LAB_140ac679d: set_state(13): the timer clears; the unit's sprite (slot 0x68 with
            // FUN_140ac5210's) is the caller's
            self.state = HuntState::Failed;
            self.timer = Seconds::ZERO;
            return Ok(false);
        }
        self.state = HuntState::Attack { attack: None };
        let reads = walks.live(mover);
        let (a, arrived) = Attack::start(
            target.id,
            attack,
            arms,
            mover,
            &reads,
            BeginLayer {
                roles,
                attack_tuning,
                move_to_tuning,
            },
        )?;
        self.state = HuntState::Attack { attack: Some(a) };
        Ok(arrived)
    }

    /// Slot 9 (`0x140ac97c0`), the work command's new target for the hunt it keeps: true when the
    /// hunt takes it. A building whose top action is an Unpack ([`ActionKind::UNPACK`], 125) takes it to `set_state` 15,
    /// not modelled. Its own player's drop site (one of its master's, `drops_here`,
    /// `FUN_14109d4f0`) while it carries something (`held`) takes the load there: the target goes
    /// (slot 0x68 with none; the caller releases it), the timer and the point clear, the drop site
    /// becomes the second target (`FUN_140a7a190`) with its class, and `set_state(8)`. Another target
    /// becomes the hunt's (slot 0x68, `FUN_140ac7980`; the caller releases the old target and takes
    /// the new) and the hunt starts over at `set_state(4)`. Its own target changes nothing. Some
    /// when it took the target, true when its Attack's `MoveTo` starts arrived (the caller shows
    /// the idle sprite, [`crate::combat::attack::show_arrived`]).
    pub(crate) fn retarget(
        &mut self,
        retarget: HuntRetarget<'_>,
        attack: &Engagement,
        arms: Arms,
        mover: &mut Mover,
        layer: BeginLayer<'_>,
        hunt_tuning: &HuntTuning,
    ) -> Result<Option<bool>, GenieError> {
        let BeginLayer {
            roles,
            attack_tuning,
            move_to_tuning,
        } = layer;
        let HuntRetarget {
            target,
            player,
            walks,
            drops_here,
        } = retarget;
        let held = walks.held.amount;
        // A building's master type (`'P'`).
        // A building whose top action is an Unpack takes a hunt sent to it to `set_state` 15.
        if target.kind == MasterKind::BUILDING && target.top_action == Some(ActionKind::UNPACK) {
            return Err(GenieError::unsupported(
                "a hunt sent to a building whose top action is an Unpack (set_state 15)",
            ));
        }
        if target.player == player && drops_here && Amount::ZERO < held {
            self.set_target(None);
            self.timer = Seconds::ZERO;
            self.point = Position::from([-1.0; 3]);
            self.carry_to = Some(DropSiteId::new(target.id));
            self.target_class = Some(target.class);
            self.set_drop(target.position, attack.gap, mover, &walks, move_to_tuning)?;
            return Ok(Some(false));
        }
        if self.target.map(HuntTargetId::id) == Some(target.id) {
            return Ok(None);
        }
        self.set_target(Some(HuntTargetId::new(target.id)));
        self.target_class = Some(target.class);
        self.carry_to = None;
        let chase = HuntChase {
            target,
            player,
            walks: &walks,
        };
        self.chase(
            chase,
            attack,
            arms,
            mover,
            BeginLayer {
                roles,
                attack_tuning,
                move_to_tuning,
            },
            hunt_tuning,
        )
        .map(Some)
    }

    /// Slot 0x40 (`FUN_140ac52c0`): the hunt takes a move's `target` (None for a point) and
    /// `point`; taken when it does. A point with a task of a work range it does not take. A unit
    /// on a pasture (`mMyPastureId` +0x794) of another target would leave it (`FUN_141189ca0`),
    /// not modelled. Its own player's drop site (`drops_here`) while it carries something
    /// (`held`) takes the load there: unless it walks there already (state 8), the target goes
    /// (slot 0x68 with none; the caller releases it), the point clears, `+0x68` takes the drop
    /// site's class, which becomes the second target (`FUN_140a7a190`; the caller takes it) and
    /// `set_state(8)`. Any other target, or a point: the point is the move's, `+0x68` −1, the
    /// target goes and `set_state(15)`: a quiet `MoveTo` to the point within the task's range
    /// (`FUN_140aa0d30`).
    pub fn take_move(
        &mut self,
        take: HuntMove<'_>,
        mover: &mut Mover,
        move_to_tuning: &MoveToTuning,
    ) -> Result<Answer, GenieError> {
        let HuntMove {
            target,
            point,
            player,
            walks,
            drops_here,
            my_pasture,
            gap,
        } = take;
        let held = walks.held.amount;
        if target.is_none() && self.task.range != Distance::ZERO {
            return Ok(Answer::NotTaken);
        }
        if my_pasture.is_some()
            && target.is_none_or(|t| Some(t.id) != my_pasture.map(PastureId::id))
        {
            return Err(GenieError::unsupported(
                "a hunter leaving its pasture (FUN_141189ca0)",
            ));
        }
        if let Some(t) = target.filter(|t| t.player == player && drops_here && Amount::ZERO < held)
        {
            if Some(t.id) == self.carry_to.map(DropSiteId::id)
                && matches!(self.state, HuntState::Drop { .. })
            {
                return Ok(Answer::Taken);
            }
            self.set_target(None);
            self.point = Position::from([-1.0; 3]);
            self.target_class = Some(t.class);
            self.carry_to = Some(DropSiteId::new(t.id));
            self.set_drop(t.position, gap, mover, &walks, move_to_tuning)?;
            return Ok(Answer::Taken);
        }
        self.point = point;
        self.target_class = None;
        self.set_target(None);
        self.set_point_walk(
            |move_to| HuntState::WalkPoint { move_to },
            mover,
            &walks,
            move_to_tuning,
        )?;
        Ok(Answer::Taken)
    }

    /// `set_state(11)` after a drop-off that found no next target: the walk to the hunt's point
    /// ([`HuntAction::set_point_walk`]), whose end searches again.
    ///
    /// # Errors
    ///
    /// A unit without speed.
    pub fn search_walk(
        &mut self,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(), GenieError> {
        self.set_point_walk(
            |move_to| HuntState::WalkSearch { move_to },
            mover,
            walks,
            move_to_tuning,
        )
    }

    /// `set_state` 11 and 15 (`0x140ac60e0` case 0xb), the walk `walk` makes: the sub-actions go,
    /// the timer clears and a quiet `MoveTo` to the point within the task's range starts
    /// ([`HuntAction::move_to_point`]).
    pub(super) fn set_point_walk(
        &mut self,
        walk: fn(Option<MoveToAction>) -> HuntState,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(), GenieError> {
        self.tear_down(Some(mover))?;
        self.state = walk(None);
        self.timer = Seconds::ZERO;
        let move_to = self.move_to_point(mover, walks, move_to_tuning)?;
        self.state = walk(move_to);
        Ok(())
    }

    /// State 4's wait after its `MoveTo` failed (`0x140ac7a00` case 3, its timer below 0): the
    /// timer runs by `dt`; at its end it clears and the hunt walks to its point (`set_state(11)`,
    /// the target kept), whose end searches.
    ///
    /// # Errors
    ///
    /// A unit that cannot move.
    pub fn wait_then_walk(
        &mut self,
        dt: Seconds,
        mover: &mut Mover,
        walks: &Walks<'_>,
        move_to_tuning: &MoveToTuning,
    ) -> Result<(), GenieError> {
        self.timer += dt;
        if self.timer < Seconds::ZERO {
            return Ok(());
        }
        self.timer = Seconds::ZERO;
        self.set_point_walk(
            |move_to| HuntState::WalkSearch { move_to },
            mover,
            walks,
            move_to_tuning,
        )
    }

    /// State 15's and 11's update (`0x140ac7a00` cases 0xe and 0xa) of `unit`'s hunt on the step,
    /// the hunt copied out of its list: the `MoveTo` to the point walks (the units in its way go
    /// to the unit AI at once, [`HuntAction::update_list`]); the motion step giving up turns it
    /// to its wait or its failure (`who`); a waiting one asks again after its wait (its case 2,
    /// its update returns 0). Once it ended (its update returns 1, 2 or 5) or failed (3, quiet: no
    /// notify), the `set_state` that follows tears it down (`FUN_140a98620`; its destructor stops
    /// the unit, slot 0x5d0): a human player's hunt in state 11 searches (`set_state(3)`: the
    /// state here, the rest the caller's), one in state 15 is done (`set_state(2)`: the target
    /// goes, the point clears and the timer; the unit takes its standing sprite and the second
    /// target goes, the caller's). A computer player's ended walk in state 15 (`set_state` 1) or
    /// failed one (`set_state` 13) is not modelled; a point walk has no target to lose (code 4).
    ///
    /// # Errors
    ///
    /// What the point walk does not model, and the walk's.
    pub(crate) fn update_point_walk(
        &mut self,
        c: &mut Step<'_>,
        unit: &mut Unit,
        me: Me,
        who: PointWalker,
    ) -> Result<PointWalk, GenieError> {
        let gives_up = GiveUp {
            patience: if !who.player_kind.is_computer() && who.villager {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: who.stance,
        };
        let ended = match self.update_list(c, unit, me, gives_up)?.get() {
            0 => return Ok(PointWalk::Walking),
            1 | 2 | 5 => true,
            3 => false,
            code => return Err(QUIET_CODE.with("code", code)),
        };
        if who.player_kind.is_computer() {
            return Err(GenieError::unsupported(if ended {
                "a computer player's hunt after its walk (set_state 1)"
            } else {
                "a computer player's hunt whose walk to a point failed (set_state 13)"
            }));
        }
        self.tear_down(c.w.parts.mover_mut(unit.id))?;
        if matches!(self.state, HuntState::WalkSearch { .. }) {
            self.state = HuntState::Search;
            return Ok(PointWalk::Search);
        }
        let [target, second] = self.set_done();
        Ok(PointWalk::Done { target, second })
    }

    /// `set_state(2)` (`0x140ac60e0` case 2): the hunt is done. The unit takes the sprite it
    /// stands with (slot 0x68 with `FUN_140ac5210`'s, the caller's), the target and the second
    /// target go (slot 0x68 and `FUN_140a7a190` with none: returned in that order for the caller
    /// to release), the point clears and the timer.
    pub fn set_done(&mut self) -> [Option<ObjectId>; 2] {
        self.state = HuntState::Idle;
        let target = self.target.take();
        let second = self.carry_to.take();
        self.point = Position::from([-1.0; 3]);
        self.timer = Seconds::ZERO;
        [target.map(HuntTargetId::id), second.map(DropSiteId::id)]
    }
}
