//! A group motion's flags ([`MotionFlags`], `mMotionFlags`).

bitflags::bitflags! {
    /// A group motion's flags (`mMotionFlags.m_bits`, the motion's `+0xb8`). The constructor
    /// (`FUN_141041850`) sets both; the end of the group's update (`FUN_14104a240`) clears them.
    /// Every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct MotionFlags: u32 {
        /// The motion moved or turned in this update (bit 0: `FUN_141049e10`, `FUN_14104a090`,
        /// `FUN_14104c150`): the update's end stores the new position.
        const MOVED = 1;
        /// The motion is new (bit 1): until the end of its first update no formation follows
        /// its moves (`commandMoveTo`), its place is the members' centre
        /// (`movePositionToFront`), its update lays out anew instead of moving the members, a
        /// hibernating group keeps updating and the statistics work out its formation type
        /// (`FUN_14100bcb0`). That end clears it and lays the formation out anew
        /// ([`crate::groups::GroupFlags::RELAYOUT`]).
        const NEW = 1 << 1;
        /// Any other bit.
        const _ = !0;
    }
}

impl MotionFlags {
    /// No flag.
    pub const NONE: Self = Self::empty();
}
