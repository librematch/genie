//! Unit groups and formations (FD-036): the game's `BUnitGroup` (`group.cpp`) with its motion
//! (`BMotion`) and formations (`BFormation`).
//!
//! The order splitter puts soldiers that move together into a group (`FUN_140fd76d0`); the group
//! moves as one (its own path and motion), keeps its members in a formation and gives each a
//! move of its own. The world holds the groups in a list (`world+0x50`, created by
//! `FUN_14152efc0`, looked up by id by `FUN_14152ff40`) and counts the ids it gave out
//! (`mUnitGroupCounter`, `world+0x5c`); a unit names its group (`mGroupID`, `+0x6d0`).
//!
//! This module holds the state, field for field as the save writes it (`FUN_1410321e0`; the
//! read is `FUN_14101d350`), each named by the game's member, so that a save of any build fills
//! it and the forced replay compares every field. The behaviour is ported routine by routine
//! from build 158041, whose addresses each port cites.

use crate::GenieError;
use crate::map::Distance;
use crate::math::fmath::BVector;
use crate::movement::PathData;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::masters::ActionType;
use crate::players::PlayerId;
use crate::unit_ai::UnitStance;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::slots::CheckedIndex;

mod command_kind;
mod formation_kind;
mod group_flags;
mod group_id;
mod group_list;
mod motion_flags;
mod objects;
mod slot_state;

pub use self::command_kind::GroupCommandKind;
pub use self::formation_kind::FormationKind;
pub use self::group_flags::GroupFlags;
pub use self::group_id::GroupId;
pub use self::group_list::{GroupLink, UnitGroups, common_group};
pub use self::motion_flags::MotionFlags;
pub(crate) use self::objects::WorldObject;
pub use self::objects::sub_formation;
pub use self::slot_state::SlotState;
pub use self::target::GroupTarget;

mod attack;
mod command;
mod create;

pub use command::MoveTo as CommandMoveTo;
/// What `commandMoveTo` is told besides the target and the point ([`crate::world::World::command_move_to`]).
pub use order::CommandArgs;
pub mod formation;
mod layout;
mod lock;
mod motion;
mod order;
mod place;
mod stats;
mod steer;
mod target;
mod task;
mod update;

/// A group's motion (`BMotion`, read by `FUN_141046660`): where the group stands and faces, its
/// path and how fast it may move and turn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Motion {
    /// `mPosition`: the point, then the forward, up and right vectors.
    pub position: [BVector; 4],
    /// `mNewPosition`, the same four vectors.
    pub new_position: [BVector; 4],
    /// `mVelocity`.
    pub velocity: BVector,
    /// `mAcceleration`.
    pub acceleration: BVector,
    /// `mAngularVelocity`.
    pub angular_velocity: BVector,
    /// `mAngularAcceleration`.
    pub angular_acceleration: BVector,
    /// `mTurnRadius`.
    pub turn_radius: Distance,
    /// `mTurnRadiusSpeed`.
    pub turn_radius_speed: Speed,
    /// `mMaximumYawPerSecondMoving`.
    pub maximum_yaw_per_second_moving: TurnRate,
    /// `mStationaryYawRevolutionTime` (its 32 bits).
    pub stationary_yaw_revolution_time: u32,
    /// `mMaximumYawPerSecondStationary`.
    pub maximum_yaw_per_second_stationary: TurnRate,
    /// The path levels (`numPathDatas`, `tempPathData`): the level the group walks first, each
    /// next one its parent (`+8`), the top level last, as a unit's (the save writes them top
    /// first).
    pub paths: Vec<PathData>,
    /// `mMotionFlags`.
    pub flags: MotionFlags,
    /// `mMaximumVelocityForward`.
    pub maximum_velocity_forward: Speed,
    /// `mNumberWaitDelays`.
    pub number_wait_delays: i32,
    /// `+0xc4` (not saved): the group's lead unit (-1 from the constructor), which a formation's
    /// transform tests the slots' places with (`FUN_1410465d0`).
    pub lead: Option<ObjectId>,
}

/// A member's own move (`mSpecificMoves[i]`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpecificMove {
    /// `mActive`.
    pub active: u32,
    /// `mPathID`.
    pub path_id: i32,
    /// `mReason`.
    pub reason: i32,
    /// `mGoal`.
    pub goal: BVector,
    /// `mTimer`.
    pub timer: Seconds,
    /// `mConsecutiveMoveFailures`.
    pub consecutive_move_failures: i32,
    /// `mDistance`.
    pub distance: Distance,
    /// `mLastPosition`.
    pub last_position: BVector,
}

/// A member (`mUnits[i]`, with its move and `mSecondsSinceAttacking[i]`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Member {
    /// The unit's id.
    pub id: ObjectId,
    /// Its own move.
    pub specific_move: SpecificMove,
    /// `mSecondsSinceAttacking[i]`.
    pub seconds_since_attacking: Seconds,
}

/// A formation's slot (`mSlots[i]`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    /// `mUnitID`.
    pub unit_id: Option<ObjectId>,
    /// `mOffset`.
    pub offset: BVector,
    /// `mTransformPosition`.
    pub transform_position: BVector,
    /// `mTime`.
    pub time: WorldTime,
    /// `mState`.
    pub state: SlotState,
    /// `mDepth`.
    pub depth: i32,
    /// `mDesiredTransformPosition`.
    pub desired_transform_position: BVector,
    /// `mPositionFilled`.
    pub position_filled: u32,
    /// `mDesiredCategory`.
    pub desired_category: i32,
    /// `mActive`.
    pub active: u32,
    /// `mDesiredSubCategory`.
    pub desired_sub_category: i32,
    /// `+0x44` (not saved): the formation's transform took the slot.
    pub transformed: u32,
}

/// A formation's line (`FUN_14101b290`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormationLine {
    /// `tempDepth`.
    pub depth: i32,
    /// `mLineIndices` (as many as `tempWidth`).
    pub indices: Vec<u32>,
    /// `mAttackRange`.
    pub attack_range: Distance,
    /// `mDistanceBack`.
    pub distance_back: Distance,
}

/// A formation (`BFormation`, read by `FUN_1410180b0`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Formation {
    /// `mType`.
    pub kind: FormationKind,
    /// `mSlots` (as many as `mNumberSlots`).
    pub slots: Vec<Slot>,
    /// `mNumberTransformedSlots`.
    pub number_transformed_slots: i32,
    /// `mMaximumWidth`.
    pub maximum_width: i32,
    /// `mMaximumDepth`.
    pub maximum_depth: i32,
    /// `mAboutFaced`.
    pub about_faced: u32,
    /// The lines (`tempMaxLines` of them).
    pub lines: Vec<FormationLine>,
    /// `mTransformType`.
    pub transform_type: i32,
    /// `mNumberLines`.
    pub number_lines: i32,
    /// `mTransformOrientation`.
    pub transform_orientation: BVector,
    /// The slots' array past `mNumberSlots` (not saved): its capacity (`+0x8`) beyond the slots
    /// in use, which a pass over the whole array reads too.
    pub spare_slots: Vec<Slot>,
}

/// A point of the stored positions (`mPoint`, `mForward`, `mUp`, `mRight`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinePoint {
    /// `mPoint`.
    pub point: BVector,
    /// `mForward`.
    pub forward: BVector,
    /// `mUp`.
    pub up: BVector,
    /// `mRight`.
    pub right: BVector,
}

/// The group's stored positions (`mStoredPositions`, a point line, `FUN_14101bb60`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PointLine {
    /// The points (`mNumberPoints`).
    pub points: Vec<LinePoint>,
    /// `mLength`.
    pub length: Distance,
    /// `mDesiredLength`.
    pub desired_length: Distance,
}

impl PointLine {
    /// `FUN_14102bae0`: the points go; the length stays.
    pub fn clear_points(&mut self) {
        self.points.clear();
    }

    /// `AddPointAtFront` (`FUN_141008040`): `p` with its height 0 goes before the newest point;
    /// the first point's length is 0, any other adds its distance to the point after it. A line
    /// of two points or more longer than its desired length then drops its oldest points while
    /// it stays longer without the last stretch.
    pub fn add_at_front(&mut self, mut p: LinePoint) -> Result<(), GenieError> {
        p.point = p.point.with_height(0.0);
        self.points.insert(0, p);
        if self.points.len() == 1 {
            self.length = Distance::ZERO;
        } else {
            self.length += stretch(self.points.checked(0)?, self.points.checked(1)?);
        }
        if self.desired_length < self.length && 1 < self.points.len() {
            let n = self.points.len();
            let mut d = stretch(self.points.checked(n - 1)?, self.points.checked(n - 2)?);
            while self.desired_length < self.length - d {
                if self.points.len() < 2 {
                    break;
                }
                let _ = self.points.pop();
                self.length -= d;
                // one point left: the game's next pass breaks at once
                let n = self.points.len();
                if n < 2 {
                    break;
                }
                d = stretch(self.points.checked(n - 1)?, self.points.checked(n - 2)?);
            }
        }
        Ok(())
    }
}

/// The 3D distance between two stored points (`FUN_14087e560`, `FUN_14099ef70`).
fn stretch(a: &LinePoint, b: &LinePoint) -> Distance {
    let d = crate::math::fmath::sub(a.point, b.point);
    Distance::new(crate::math::fmath::sqrt(
        d.x() * d.x() + d.height() * d.height() + d.y() * d.y(),
    ))
}

/// The members of one formation category (`FUN_141023570`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitTypeStatistic {
    /// `mFormationCategory`.
    pub formation_category: i32,
    /// `mUnits` (as many as `mNumberUnits`).
    pub units: Vec<ObjectId>,
    /// `mUsed`.
    pub used: u32,
    /// `mAttackRange`.
    pub attack_range: Distance,
    /// `mDesiredSpacing`.
    pub desired_spacing: Distance,
    /// `mNumberUnitTypes`.
    pub number_unit_types: i32,
    /// `mUnitTypes`: the 200 bytes the save writes (`+0x38`), as 50 words.
    pub unit_types: Vec<u32>,
    /// `mUnitTypeCounts`: the 200 bytes the save writes (`+0x100`), as 50 words.
    pub unit_type_counts: Vec<u32>,
}

/// The group's target (`mCurrentTarget`, `FUN_14101c7d0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TargetData {
    /// `mTarget` with its kind `mType`.
    pub target: GroupTarget,
    /// `mOriginalPosition`.
    pub original_position: BVector,
    /// `mRange`.
    pub range: Distance,
    /// `mTargetRadius`.
    pub target_radius: Distance,
    /// `mDisableRetargeting`.
    pub disable_retargeting: u8,
    /// `mCertainAction`: the action type the attack asks of its units, none for −1 (any).
    pub certain_action: Option<ActionType>,
}

/// An order of the group's queue (`newOrder`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroupOrder {
    /// `mCommand`.
    pub command: GroupCommandKind,
    /// `mTargetID`.
    pub target_id: Option<ObjectId>,
    /// `mParm`.
    pub parm: f32,
    /// `mPoint`.
    pub point: BVector,
    /// `mExecuteTime`.
    pub execute_time: WorldTime,
}

/// A command of the group's queue (`newCommand`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroupCommand {
    /// `mCommand`.
    pub command: GroupCommandKind,
    /// `mTargetID`.
    pub target_id: Option<ObjectId>,
    /// `mRange`.
    pub range: Distance,
    /// `mPoint`.
    pub point: BVector,
    /// `mApproved`.
    pub approved: u8,
    /// `mDisableRetargeting`.
    pub disable_retargeting: u8,
    /// The 16 bits the save writes after them (read as `mApproved` again).
    pub approved_2: u16,
}

/// A unit group (`BUnitGroup`, 0x5c0 bytes, constructed by `FUN_140fe1550`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitGroup {
    /// Its motion.
    pub motion: Motion,
    /// `mID` (`+0xd8`).
    pub id: GroupId,
    /// `mOwner` (its 32 bits).
    pub owner: u32,
    /// `mState`: the members' most common stance
    /// ([`crate::world::World::stances_state`]), none (−1) until
    /// a formation first locks it.
    pub stance: Option<UnitStance>,
    /// The members (`mNumberUnits` at `+0xe4`, `mUnits` at `+0xf0`).
    pub members: Vec<Member>,
    /// `mPreviousFormation`, when it has one.
    pub previous_formation: Option<Formation>,
    /// `mCurrentFormation`, when it has one.
    pub current_formation: Option<Formation>,
    /// The word the save writes after the formations (`garbage`).
    pub garbage: u32,
    /// `mFormationLockedType`: none for the constructor's −1; a horde (0) is no lock either.
    pub formation_locked_type: Option<FormationKind>,
    /// `mGroupFlags`.
    pub flags: GroupFlags,
    /// `mCentroid`.
    pub centroid: BVector,
    /// `mDesiredHeight`.
    pub desired_height: f32,
    /// `mLargestUnitRadius`.
    pub largest_unit_radius: Distance,
    /// `mSmallestUnitRadius`.
    pub smallest_unit_radius: Distance,
    /// `mDesiredSpacing`.
    pub desired_spacing: Distance,
    /// `mSmallestUnitRange`.
    pub smallest_unit_range: Distance,
    /// `mSmallestRangedUnitRange`.
    pub smallest_ranged_unit_range: Distance,
    /// `mLargestUnitRange`.
    pub largest_unit_range: Distance,
    /// `mLargestRangedUnitRange`.
    pub largest_ranged_unit_range: Distance,
    /// `mStoredPositions`.
    pub stored_positions: PointLine,
    /// `mUnitTypes`: the members by formation category.
    pub unit_types: Vec<UnitTypeStatistic>,
    /// `mWaypoints`.
    pub waypoints: Vec<BVector>,
    /// `mCurrentCommand`.
    pub current_command: GroupCommandKind,
    /// `mCurrentTarget`.
    pub current_target: TargetData,
    /// `mAttackPlanType`.
    pub attack_plan_type: i32,
    /// `mIdleTimer`.
    pub idle_timer: Millis,
    /// The order queue.
    pub order_queue: Vec<GroupOrder>,
    /// `mHibernating`.
    pub hibernating: u8,
    /// The command queue.
    pub command_queue: Vec<GroupCommand>,
    /// The objects the save lists after the queues (`objCount`, `objId`): the units the group
    /// keeps apart (`+0x458`, size `+0x468`).
    pub objects: Vec<ObjectId>,
    /// What the members' searches found (`+0x49c`, at most 70, its count `+0x5b4`; not saved):
    /// the statistics refill it ([`crate::world::World::statistics_only`]), and nothing else in
    /// the game reads or writes it.
    pub sighted: Vec<ObjectId>,
    /// `mDifferentiatorID`.
    pub differentiator_id: i32,
    /// `mSecondsNotUnderAttack`.
    pub seconds_not_under_attack: Seconds,
    /// `mDifferentiatorGroups`.
    pub differentiator_groups: Vec<GroupId>,
    /// `mTotalSpacing`.
    pub total_spacing: Distance,
    /// `mNonRetargetUnits`.
    pub non_retarget_units: Vec<u32>,
    /// The members' `mSecondsSinceAttacking` slots past the members (not saved): the array's
    /// capacity beyond its count, which the next members take in order (`AddUnit`'s
    /// `FUN_14102d710`).
    pub spare_seconds: Vec<Seconds>,
    /// The special units (`+0x488` its count, `+0x48c` the ids; not saved): the members a group's
    /// command on an attack task lists for the target (`FUN_14100c6c0`, its one writer).
    pub special_units: Vec<ObjectId>,
    /// `+0x13d` (not saved; the constructor's 1): the group's last command was not queued
    /// (`BUnitGroup::command` sets it), so its next move empties the command queue first;
    /// `checkQueuedOrders` (`FUN_14100d460`) clears it.
    pub replaces_queue: u8,
}

impl Motion {
    /// `BMotion`'s constructor (`FUN_141041850`): at the origin facing x (forward (1, 0, 0), up
    /// (0, 1, 0), right their cross product (0, 0, -1)), no speed, the turn limits at `f32::MAX`
    /// (the constructor's `0x7f7fffff`), one level of path data without a path (`FUN_140f8f670`),
    /// flags [`MotionFlags::MOVED`] and [`MotionFlags::NEW`].
    #[must_use]
    pub fn new() -> Self {
        let frame = [
            BVector::ZERO,
            BVector::new(1.0, 0.0, 0.0),
            BVector::new(0.0, 1.0, 0.0),
            BVector::new(0.0, 0.0, -1.0),
        ];
        Self {
            position: frame,
            new_position: frame,
            turn_radius_speed: Speed::MAX,
            maximum_yaw_per_second_moving: TurnRate::MAX,
            maximum_yaw_per_second_stationary: TurnRate::MAX,
            paths: vec![PathData::without_path()],
            flags: MotionFlags::MOVED.union(MotionFlags::NEW),
            lead: None,
            ..Self::default()
        }
    }
}

impl UnitGroup {
    /// The player the group belongs to (`+0xdc`, set by the order's handler).
    ///
    /// # Errors
    ///
    /// A group no player owns yet (the constructor's `u32::MAX`).
    pub(crate) fn owner_player(&self) -> Result<PlayerId, GenieError> {
        usize::try_from(self.owner)
            .map_err(|_| GenieError::unreachable("a group's owner past the players"))
            .and_then(PlayerId::try_from)
            .map_err(|e| e.with("owner", self.owner))
    }

    /// `BUnitGroup`'s constructor (`FUN_140fe1550` after [`Motion::new`]) with id `id`
    /// (`FUN_141039640`): the owner -1 (the 64-bit -1 over `+0xd8` and `+0xdc`, whose id the id's
    /// setter then writes; [`UnitGroups::create`] sets the owner after), no stance (−1), no
    /// formation, flags [`GroupFlags::TURN`], no command (-1), no target (-1, type -1),
    /// differentiator -1, `+0x13d` 1 (the 16-bit `0x100` at `+0x13c`, `mHibernating` 0); every
    /// other field 0 or empty, the attack plan (`+0x3b0`, `FUN_140fd75c0`) too.
    ///
    /// The constructor's other values are no group state: `+0xc8` and `+0xd0` point at the
    /// group itself; `+0x1e0` (-1) is the category of the unit types list's sentinel node
    /// (`+0x1d0`), which every walk of the list stops at before reading; the -1 at `+0x3e8` and
    /// `+0x420` are the order and command queues' sentinels; `+0x470` (0x20) is the growth step
    /// of the array at `+0x458` (data, capacity `+0x460`, size `+0x468`), which sets its
    /// capacity only.
    #[must_use]
    pub fn new(id: GroupId) -> Self {
        Self {
            motion: Motion::new(),
            id,
            owner: u32::MAX,
            stance: None,
            flags: GroupFlags::TURN,
            current_command: GroupCommandKind::NONE,
            current_target: TargetData::default(),
            differentiator_id: -1,
            replaces_queue: 1,
            ..Self::default()
        }
    }

    /// `FUN_14102bb40`: the command queue empties (`+0x420`, its size `+0x450`); a hibernating
    /// group (`mHibernating`, `+0x13c`, which `commandMoveTo`'s queue path sets for a group
    /// without a command) wakes, with flags [`GroupFlags::RELAYOUT_AT_TURN`],
    /// [`GroupFlags::PLACE_AGAIN`] and [`GroupFlags::LEAD_TO_TARGET`] when it was 1.
    pub fn clear_command_queue(&mut self) {
        self.command_queue.clear();
        if self.hibernating != 0 {
            if self.hibernating == 1 {
                self.flags |= GroupFlags::RELAYOUT_AT_TURN
                    | GroupFlags::PLACE_AGAIN
                    | GroupFlags::LEAD_TO_TARGET;
            }
            self.hibernating = 0;
        }
    }

    /// Whether unit `id` is a member (`FUN_1410112b0`).
    #[must_use]
    pub fn has(&self, id: impl ObjectKey) -> bool {
        let id = id.key();
        self.members.iter().any(|m| m.id.key() == id)
    }

    /// Unit `id`'s place among the members (`FUN_1410175f0`: its index in `+0xf0`), −1 when it is
    /// not one.
    #[must_use]
    pub fn index_of(&self, id: ObjectId) -> i32 {
        self.members
            .iter()
            .position(|m| m.id == id)
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1)
    }

    /// `FUN_141017590`: unit `id`'s own move's reason (`mReason`, the `+8` of its record in
    /// `+0xf8`), −1 when it is not a member.
    #[must_use]
    pub fn member_reason(&self, id: ObjectId) -> i32 {
        self.members
            .iter()
            .find(|m| m.id == id)
            .map_or(-1, |m| m.specific_move.reason)
    }

    /// `FUN_1410177f0`: unit `id` is in its place, its first slot (`mUnitID`) in the current
    /// formation (`+0x128`) in state 3 (`+0x2c`); not without such a slot or formation.
    #[must_use]
    pub fn slot_in_place(&self, id: ObjectId) -> bool {
        self.current_formation
            .as_ref()
            .and_then(|f| f.slots.iter().find(|s| s.unit_id == Some(id)))
            .is_some_and(|s| s.state == SlotState::InPlace)
    }
}

#[cfg(test)]
mod tests;
