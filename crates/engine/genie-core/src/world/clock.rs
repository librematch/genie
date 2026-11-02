//! The world's clock in the game's milliseconds: a moment of it ([`WorldTime`]) and a span
//! ([`Millis`]). The game keeps both as `u32` and its arithmetic on them wraps. The update's
//! span in seconds and the timers that count it down are `f32` ([`Seconds`]).

/// A moment of the world's clock in milliseconds since the game's start (`world_time`): the
/// clock itself and the moments the objects keep (a unit AI's last retarget, a reload's end,
/// a building's last garrison count).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldTime(u32);

impl WorldTime {
    /// The game's start.
    pub const ZERO: Self = Self(0);

    /// The moment `ms` milliseconds after the game's start.
    #[must_use]
    pub const fn from_millis(ms: u32) -> Self {
        Self(ms)
    }

    /// The milliseconds since the game's start, as the save and the game's log hold them.
    #[must_use]
    pub const fn millis(self) -> u32 {
        self.0
    }

    /// The moment `d` after it (the game's wrapping add).
    #[must_use]
    pub const fn after(self, d: Millis) -> Self {
        Self(self.0.wrapping_add(d.0))
    }

    /// The moment `d` before it, not before the game's start (the game's saturating
    /// subtraction: a unit's last human order time that the world's dt counts down).
    #[must_use]
    pub const fn saturating_before(self, d: Millis) -> Self {
        Self(self.0.saturating_sub(d.0))
    }

    /// The span from `earlier` to it (the game's wrapping subtraction).
    #[must_use]
    pub const fn since(self, earlier: Self) -> Millis {
        Millis(self.0.wrapping_sub(earlier.0))
    }

    /// The span from the game's start to it: where the game passes the world time as a span
    /// (a unit AI's notifies cleared on a MOVE or an ownership change, whose full update passes
    /// the owner's time since its last turn).
    #[must_use]
    pub const fn since_start(self) -> Millis {
        Millis(self.0)
    }

    /// The moment in seconds (`cvtsi2ss`, then `divss` by the image's 1000.0, `DAT_14330ebe0`:
    /// the facing's clock, `FUN_141162db0`).
    #[must_use]
    pub fn seconds(self) -> Seconds {
        Seconds(self.0 as f32 / crate::math::image::facing::MS_PER_SECOND)
    }
}

impl std::fmt::Display for WorldTime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for WorldTime {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A span of the world's clock in milliseconds: an update's `dt`, a timer, an interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Millis(u32);

impl Millis {
    /// No time.
    pub const ZERO: Self = Self(0);

    /// A span of `ms` milliseconds.
    #[must_use]
    pub const fn new(ms: u32) -> Self {
        Self(ms)
    }

    /// The milliseconds, as the save and the commands hold them.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The span and `d` (the game's wrapping add: a timer that counts the updates' time up).
    #[must_use]
    pub const fn wrapping_add(self, d: Self) -> Self {
        Self(self.0.wrapping_add(d.0))
    }

    /// The span less `d`, at least none (a timer that counts down to 0).
    #[must_use]
    pub const fn saturating_sub(self, d: Self) -> Self {
        Self(self.0.saturating_sub(d.0))
    }

    /// The span in seconds (`cvtsi2ss`, then `divss` by 1000 in f32: `world+0x164` of an
    /// update's `dt`).
    #[must_use]
    pub fn seconds(self) -> Seconds {
        Seconds(self.0 as f32 / 1000.0)
    }
}

impl std::fmt::Display for Millis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for Millis {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A build, train or research time in whole seconds as a master's or a technology's data holds it
/// (a short: a master's `+0x898` train locations, its build time): every short is kept, and
/// [`Seconds::of_master`] is the game's conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WholeSeconds(i16);

impl WholeSeconds {
    /// No time (0).
    pub const ZERO: Self = Self(0);

    /// The time of the data's short `value`.
    #[must_use]
    pub const fn of_data(value: i16) -> Self {
        Self(value)
    }

    /// The short plus `amount` (an effect's add, wrapping as the game's 16-bit add).
    #[must_use]
    pub const fn plus(self, amount: i16) -> Self {
        Self(self.0.wrapping_add(amount))
    }

    /// The short, as the data and the save hold it.
    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }
}

impl genie_diff::Diff for WholeSeconds {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

/// A span in seconds as the game's `f32` holds it: an update's (`world+0x164`, the dt over
/// 1000), the timers that count it down or up, the masters' times. Each operator is the one
/// `f32` operation the game does; a comparison is `f32`'s (a NaN compares false).
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Seconds(f32);

impl Seconds {
    /// No time.
    pub const ZERO: Self = Self(0.0);
    /// The longest time, the game's `FLT_MAX`: never (a group member that never attacked).
    pub const MAX: Self = Self(f32::MAX);

    /// A span of `s` seconds.
    #[must_use]
    pub const fn new(s: f32) -> Self {
        Self(s)
    }

    /// A master's whole seconds (a build, train or research time) as the game converts them
    /// (`CVTSI2SS`, exact for an `i16`).
    #[must_use]
    pub fn of_master(s: WholeSeconds) -> Self {
        Self(f32::from(s.0))
    }

    /// The seconds, as the save and the game's arithmetic hold them.
    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::Add for Seconds {
    type Output = Self;

    /// The `f32` sum.
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl std::ops::Sub for Seconds {
    type Output = Self;

    /// The `f32` difference.
    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl std::ops::AddAssign for Seconds {
    /// The `f32` sum in place (a timer that counts up).
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
    }
}

impl std::ops::SubAssign for Seconds {
    /// The `f32` difference in place (a timer that counts down).
    fn sub_assign(&mut self, other: Self) {
        self.0 -= other.0;
    }
}

impl std::ops::Neg for Seconds {
    type Output = Self;

    /// The span negated (its sign bit flipped).
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl std::ops::Mul<f32> for Seconds {
    type Output = Self;

    /// The span scaled by `f` (one `f32` product).
    fn mul(self, f: f32) -> Self {
        Self(self.0 * f)
    }
}

impl std::ops::MulAssign<f32> for Seconds {
    /// The span scaled by `f` in place (an effect's multiplier).
    fn mul_assign(&mut self, f: f32) {
        self.0 *= f;
    }
}

impl std::ops::Div<f32> for Seconds {
    type Output = Self;

    /// The span divided by `f` (one `f32` quotient).
    fn div(self, f: f32) -> Self {
        Self(self.0 / f)
    }
}

impl std::ops::Div for Seconds {
    type Output = f32;

    /// The ratio of two spans (one `f32` quotient).
    fn div(self, other: Self) -> f32 {
        self.0 / other.0
    }
}

impl std::ops::Mul<Seconds> for f32 {
    type Output = Self;

    /// A rate per second over the span: what it adds up to (one `f32` product).
    fn mul(self, s: Seconds) -> Self {
        self * s.0
    }
}

impl std::fmt::Display for Seconds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for Seconds {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        self.0.diff(&other.0, d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_wraps_as_the_game_u32() {
        let late = WorldTime::from_millis(u32::MAX - 9);
        let later = late.after(Millis::new(20));
        assert_eq!(later.millis(), 10);
        assert_eq!(later.since(late), Millis::new(20));
        assert_eq!(WorldTime::ZERO.since(late).get(), 10);
        assert!(WorldTime::from_millis(3) < WorldTime::from_millis(4));
        assert_eq!(WorldTime::from_millis(1500).to_string(), "1500");
        assert_eq!(
            WorldTime::from_millis(1500).since_start(),
            Millis::new(1500)
        );
        let order = WorldTime::from_millis(30);
        assert_eq!(order.saturating_before(Millis::new(50)), WorldTime::ZERO);
        assert_eq!(order.saturating_before(Millis::new(20)).millis(), 10);
    }

    #[test]
    fn span_counts_up_wrapping_and_down_to_none() {
        let t = Millis::new(u32::MAX).wrapping_add(Millis::new(2));
        assert_eq!(t.get(), 1);
        assert_eq!(
            Millis::new(50).saturating_sub(Millis::new(80)),
            Millis::ZERO
        );
        assert_eq!(Millis::new(80).saturating_sub(Millis::new(50)).get(), 30);
        assert_eq!(Millis::new(7).to_string(), "7");
    }

    #[test]
    fn seconds_do_the_game_f32_operations() {
        let (a, b) = (Seconds::new(0.1), Seconds::new(0.2));
        assert_eq!((a + b).get().to_bits(), (0.1_f32 + 0.2).to_bits());
        assert_eq!((b - a).get().to_bits(), (0.2_f32 - 0.1).to_bits());
        let mut t = Seconds::new(1.0);
        t -= Seconds::new(0.069);
        assert_eq!(t.get().to_bits(), (1.0_f32 - 0.069).to_bits());
        t += Seconds::new(0.069);
        assert_eq!(t.get().to_bits(), ((1.0_f32 - 0.069) + 0.069).to_bits());
        assert_eq!((-a).get().to_bits(), (-0.1_f32).to_bits());
        assert_eq!((a * 3.0).get().to_bits(), (0.1_f32 * 3.0).to_bits());
        let mut scaled = a;
        scaled *= 3.0;
        assert_eq!(scaled, a * 3.0);
        assert_eq!((a / 3.0).get().to_bits(), (0.1_f32 / 3.0).to_bits());
        assert_eq!((b / a).to_bits(), (0.2_f32 / 0.1).to_bits());
        assert_eq!((1.5_f32 * a).to_bits(), (1.5_f32 * 0.1).to_bits());
        assert!(Seconds::ZERO < a && a <= a);
        assert_eq!(Seconds::new(f32::NAN).partial_cmp(&Seconds::ZERO), None);
        assert_eq!(Seconds::new(0.5).to_string(), "0.5");
        assert_eq!(
            Seconds::of_master(WholeSeconds::of_data(-3))
                .get()
                .to_bits(),
            (-3.0_f32).to_bits()
        );
        assert!(WholeSeconds::ZERO < WholeSeconds::of_data(1));
        assert_eq!(WholeSeconds::default().get(), 0);
        assert_eq!(WholeSeconds::of_data(i16::MAX).plus(1).get(), i16::MIN);
    }

    #[test]
    fn seconds_are_the_milliseconds_over_thousand_in_f32() {
        assert_eq!(
            Millis::new(50).seconds().get().to_bits(),
            (50.0_f32 / 1000.0).to_bits()
        );
        assert_eq!(
            WorldTime::from_millis(16_777_217).seconds(),
            Seconds::new(16_777.216)
        );
    }
}
