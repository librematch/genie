//! A master's adjustments of a conversion ([`ConversionAdjust`]).

use crate::objects::masters::MasterAttribute;
use genie_diff::Diff;
use serde::Deserialize;
use serde::Serialize;

/// A master's `mMinConversionTimeAdj`, `mMaxConversionTimeAdj` and `mConversionChanceAdj`
/// (`+0x934`, `+0x938`, `+0x93c`): what its effects add to a conversion's shortest and longest
/// time and to its chance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Diff, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ConversionAdjust {
    /// `mMinConversionTimeAdj` (`+0x934`, "Min Conversion Time Modifier").
    pub min_time: f32,
    /// `mMaxConversionTimeAdj` (`+0x938`, "Max Conversion Time Modifier").
    pub max_time: f32,
    /// `mConversionChanceAdj` (`+0x93c`, "Conversion Chance Modifier").
    pub chance: f32,
}

impl ConversionAdjust {
    /// The value an effect's attribute changes: the shortest time's, the longest's, else the
    /// chance's.
    pub const fn lane_mut(&mut self, attribute: MasterAttribute) -> &mut f32 {
        match attribute {
            MasterAttribute::MIN_CONVERSION_TIME_MODIFIER => &mut self.min_time,
            MasterAttribute::MAX_CONVERSION_TIME_MODIFIER => &mut self.max_time,
            _ => &mut self.chance,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::ConversionAdjust;
    use crate::objects::masters::MasterAttribute;

    /// Each conversion attribute changes its own value.
    #[test]
    fn each_conversion_attribute_changes_its_value() {
        let mut c = ConversionAdjust::default();
        *c.lane_mut(MasterAttribute::CONVERSION_CHANCE_MODIFIER) = 3.0;
        *c.lane_mut(MasterAttribute::MAX_CONVERSION_TIME_MODIFIER) = 2.0;
        *c.lane_mut(MasterAttribute::MIN_CONVERSION_TIME_MODIFIER) = 1.0;
        assert_eq!(
            c,
            ConversionAdjust {
                min_time: 1.0,
                max_time: 2.0,
                chance: 3.0
            }
        );
    }
}
