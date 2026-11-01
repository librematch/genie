//! How far a volley's missiles scatter ([`VolleySpread`]).

use crate::objects::masters::MasterAttribute;
use crate::objects::masters::attribute as a;
use genie_diff::Diff;

/// A master's `volley_x_spread`, `volley_y_spread` and `volley_start_spread_adjustment`
/// (`+0x8c8`..`+0x8d0`, the `.dat`'s projectile spawning area): how far a volley's missiles
/// scatter across and along the shot, and how far forward they start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff)]
pub struct VolleySpread {
    /// `volley_x_spread` (`+0x8c8`, the attribute "Spawning Area Width").
    pub x: crate::map::Distance,
    /// `volley_y_spread` (`+0x8cc`, "Spawning Area Length").
    pub y: crate::map::Distance,
    /// `volley_start_spread_adjustment` (`+0x8d0`, "Spawning Area Randomness").
    pub start: crate::map::Distance,
}

impl VolleySpread {
    /// The value an effect's attribute changes: its width, its length, else its randomness.
    pub(crate) const fn lane_mut(
        &mut self,
        attribute: MasterAttribute,
    ) -> &mut crate::map::Distance {
        match attribute {
            a::SPAWNING_AREA_WIDTH => &mut self.x,
            a::SPAWNING_AREA_LENGTH => &mut self.y,
            _ => &mut self.start,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::VolleySpread;
    use crate::objects::masters::attribute as a;

    /// Each spawning area attribute changes its own value.
    #[test]
    fn each_spawning_area_attribute_changes_its_value() {
        let mut s = VolleySpread::default();
        *s.lane_mut(a::SPAWNING_AREA_LENGTH) = crate::map::Distance::new(2.0);
        *s.lane_mut(a::SPAWNING_AREA_RANDOMNESS) = crate::map::Distance::new(3.0);
        *s.lane_mut(a::SPAWNING_AREA_WIDTH) = crate::map::Distance::new(1.0);
        assert_eq!(
            s,
            VolleySpread {
                x: crate::map::Distance::new(1.0),
                y: crate::map::Distance::new(2.0),
                start: crate::map::Distance::new(3.0)
            }
        );
    }
}
