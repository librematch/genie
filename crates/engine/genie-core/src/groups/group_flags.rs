//! A group's flags ([`GroupFlags`], `mGroupFlags`).

bitflags::bitflags! {
    /// A group's flags (`mGroupFlags.m_bits`, `+0x140`): the steps of its command and its
    /// update still due. Named by the routines that set and read each bit; every bit is kept
    /// (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct GroupFlags: u32 {
        /// The formation lays out anew once it is spread and idle (bit 1; a new group's).
        const RELAYOUT = 1 << 1;
        /// The desired positions make a line, whatever the members (bit 2); the move's end
        /// clears it, and no routine of build 158041 sets it by an immediate.
        const LINE = 1 << 2;
        /// The attack's group has arrived (bit 3, `FUN_14103f4e0`).
        const ARRIVED = 1 << 3;
        /// The attack's group is on its way (bit 4).
        const ATTACKING = 1 << 4;
        /// The move's members walk to their places (bit 5, `FUN_141041420`'s end); a new move,
        /// an order or a waypoint clears it.
        const MEMBERS_SENT = 1 << 5;
        /// A horde command found no unit to walk its path (bit 6, `FUN_141010180`: "We don't
        /// have a path unit"): the awake update moves nothing.
        const NO_PATH_UNIT = 1 << 6;
        /// The members changed while the group stood close: it lays out anew once spread
        /// (bit 7).
        const RELAYOUT_SPREAD = 1 << 7;
        /// The formation turns in the next update, which runs the motion without its budget
        /// (bit 8; `commandMoveTo`'s end sets it).
        const TURN = 1 << 8;
        /// A member is far from its slot and catches up (bit 9).
        const CATCHING_UP = 1 << 9;
        /// The group has its initial place (bit 10, `SetInitialPosition`).
        const PLACED = 1 << 10;
        /// The group walks a patrol (bit 11).
        const PATROL = 1 << 11;
        /// The follow's update moved the group after its target (bit 12, `FUN_14103d910`); a
        /// new move clears it.
        const FOLLOWED = 1 << 12;
        /// The bit the group's update clears first (bit 13, `FUN_14103d910`); no routine of
        /// build 158041 sets it by an immediate.
        const UPDATE_CLEARED = 1 << 13;
        /// The formation lays out anew at its turn (bit 14).
        const RELAYOUT_AT_TURN = 1 << 14;
        /// The group takes its initial place again (bit 15).
        const PLACE_AGAIN = 1 << 15;
        /// The statistics are due (bit 16; a new group's).
        const STATISTICS = 1 << 16;
        /// A loose unit near the place joined the group (bit 17, the patrol's split).
        const JOINED = 1 << 17;
        /// The lead goes to the target first (bit 18).
        const LEAD_TO_TARGET = 1 << 18;
        /// Any other bit.
        const _ = !0;
    }
}

impl GroupFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
}

impl genie_diff::Diff for GroupFlags {
    /// By equality: the flags are one value.
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}
