//! The action layer's values under the action master's setters (slots 0x18, 0x20 and 0x28):
//! each attribute whose set, add or scale writes one value of the layer. The setters
//! ([`crate::objects::masters::Master::set_action`] …) take the layer once and hand these
//! attributes over.

use crate::map::Distance;
use crate::objects::WorkRate;
use crate::objects::masters::ActionPart;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::RunPattern;
use crate::objects::masters::low8;

impl ActionPart {
    /// [`crate::objects::masters::Master::set_action`]'s values of the layer: what `attribute` with
    /// `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn set_value(&mut self, attribute: MasterAttribute, v: f32) {
        match attribute {
            MasterAttribute::WORK_RATE => {
                self.work_rate = WorkRate::new(v);
            }
            MasterAttribute::SEARCH_RADIUS => {
                self.search_radius = Distance::new(v);
            }
            MasterAttribute::RUN_PATTERN => {
                self.run_pattern = RunPattern::of_data(low8(v));
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::add_action`]'s values of the layer: what `attribute` with
    /// `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn add_value(&mut self, attribute: MasterAttribute, v: f32) {
        match attribute {
            MasterAttribute::WORK_RATE => {
                self.work_rate += WorkRate::new(v);
            }
            MasterAttribute::SEARCH_RADIUS => {
                self.search_radius += Distance::new(v);
            }
            MasterAttribute::RUN_PATTERN => {
                self.run_pattern = self.run_pattern.plus(low8(v));
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::multiply_action`]'s values of the layer: what `attribute`
    /// with `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn multiply_value(&mut self, attribute: MasterAttribute, v: f32) {
        match attribute {
            MasterAttribute::WORK_RATE => {
                self.work_rate *= v;
            }
            MasterAttribute::SEARCH_RADIUS => {
                self.search_radius *= v;
            }
            _ => {}
        }
    }
}
