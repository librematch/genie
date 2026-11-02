//! A sprite's frame and its animation state.

use std::ops::{Add, Neg, Rem, Sub};

/// A frame of a sprite (a short): the frame a sprite shows (`+0x24`) and showed before
/// (`+0x30`), a graphic's frame count (`frame_count`, `+0x8`, one past its last frame), a
/// master's fire and interrupt frames and the frame a BUILD command chose. Each operator is the
/// game's 16-bit one; every value is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frame(i16);

impl Frame {
    /// The first frame.
    pub const ZERO: Self = Self(0);
    /// One frame: a step and the distance from a count to the last frame.
    pub const ONE: Self = Self(1);
    /// No frame (−1): a master's interrupt frame of none.
    pub const NONE: Self = Self(-1);

    /// The frame of the data's or the save's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The frame of a byte (a BUILD command's, a building's `+0xc54`).
    #[must_use]
    pub const fn of_byte(value: u8) -> Self {
        Self(value as i16)
    }

    /// The low byte, as a BUILD command holds the frame.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self.0 as u8
    }

    /// Half the frames (`IDIV` by 2: toward 0).
    #[must_use]
    pub const fn half(self) -> Self {
        Self(self.0 / 2)
    }

    /// The frame as a float (`CVTSI2SS`, exact).
    #[must_use]
    pub fn as_f32(self) -> f32 {
        f32::from(self.0)
    }

    /// The frame plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The frame times `factor` (an effect's multiply, the game's 16-bit `IMUL`).
    #[must_use]
    pub const fn times(self, factor: i16) -> Self {
        Self(self.0.wrapping_mul(factor))
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl Add for Frame {
    type Output = Self;

    /// The 16-bit sum.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Frame {
    type Output = Self;

    /// The 16-bit difference.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Neg for Frame {
    type Output = Self;

    /// The negated frames: a backwards step.
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Rem for Frame {
    type Output = Self;

    /// The 16-bit remainder (`IDIV`'s, its sign the frame's).
    fn rem(self, count: Self) -> Self {
        Self(self.0 % count.0)
    }
}

impl genie_diff::Diff for Frame {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A sprite's animation state (`animate_flag`, `+0x36`): stopped, playing or waiting out its
/// replay delay. Every byte is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AnimateState(u8);

impl AnimateState {
    /// 0: stopped (a static sprite's, or one played once to its end).
    pub const STOPPED: Self = Self(0);
    /// 1: playing.
    pub const PLAYING: Self = Self(1);
    /// 2: past its last frame, waiting out the replay delay.
    pub const DELAYED: Self = Self(2);

    /// The state of the save's byte `value`.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self(value)
    }

    /// The byte, as the save holds it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl genie_diff::Diff for AnimateState {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::{AnimateState, Frame};

    /// Frames add, subtract, negate and take remainders as shorts; the effects' ops wrap; a
    /// byte frame keeps its byte.
    #[test]
    fn frames_are_the_game_shorts() {
        let count = Frame::of_data(10);
        assert_eq!(count - Frame::ONE, Frame::of_data(9));
        assert_eq!(Frame::of_data(3) + -Frame::of_data(5), Frame::of_data(-2));
        assert_eq!(Frame::of_data(-13) % count, Frame::of_data(-3));
        assert_eq!(Frame::of_data(25) % count, Frame::of_data(5));
        assert_eq!(Frame::of_data(7).half(), Frame::of_data(3));
        assert_eq!(Frame::of_data(-7).half(), Frame::of_data(-3));
        assert_eq!(Frame::of_data(i16::MAX).plus(1).get(), i16::MIN);
        assert_eq!(
            Frame::of_data(300).times(300).get(),
            300_i16.wrapping_mul(300)
        );
        assert_eq!(Frame::of_byte(200), Frame::of_data(200));
        assert_eq!(Frame::of_byte(200).byte(), 200);
        assert_eq!(Frame::of_data(12).as_f32(), 12.0);
        assert!(Frame::ZERO < Frame::ONE);
    }

    /// The animation state keeps its byte.
    #[test]
    fn the_animation_state_keeps_its_byte() {
        assert_eq!(AnimateState::of_data(2), AnimateState::DELAYED);
        assert_eq!(AnimateState::default(), AnimateState::STOPPED);
        assert_eq!(AnimateState::PLAYING.get(), 1);
    }
}
