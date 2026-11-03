//! The moving layer's values under the moving master's setters (slots 0x18, 0x20 and 0x28):
//! each attribute whose set, add or scale writes one value of the layer. The setters
//! ([`crate::objects::masters::Master::set_moving`] …) take the layer once and hand these
//! attributes over.

use crate::math::conversions::cvttss2si;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterField;
use crate::objects::masters::MovingPart;
use crate::objects::masters::TrailDensity;
use crate::objects::masters::TrailMode;
use crate::objects::masters::add_sprite;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::set_sprite;
use crate::world::Seconds;

impl MovingPart {
    /// [`crate::objects::masters::Master::set_moving`]'s values of the layer: what `attribute` with
    /// `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn set_value(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            MasterAttribute::ROTATION_SPEED => {
                self.turn_speed = Seconds::new(v);
            }
            MasterAttribute::WALKING_GRAPHIC => {
                set_sprite(&mut self.move_sprite, cvttss2si(v), world);
            }
            MasterAttribute::RUNNING_GRAPHIC => {
                set_sprite(&mut self.run_sprite, cvttss2si(v), world);
            }
            MasterAttribute::TRAILING_UNIT => {
                self.trail.unit = MasterField::of_data(low16(v));
            }
            MasterAttribute::TRAIL_MODE => {
                self.trail.mode = TrailMode::of_data(low8(v));
            }
            MasterAttribute::TRAIL_DENSITY => {
                self.trail.density = TrailDensity::new(v);
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::add_moving`]'s values of the layer: what `attribute` with
    /// `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn add_value(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match attribute {
            MasterAttribute::ROTATION_SPEED => {
                self.turn_speed += Seconds::new(v);
            }
            MasterAttribute::WALKING_GRAPHIC => {
                add_sprite(&mut self.move_sprite, v, world);
            }
            MasterAttribute::RUNNING_GRAPHIC => {
                add_sprite(&mut self.run_sprite, v, world);
            }
            MasterAttribute::TRAILING_UNIT => {
                self.trail.unit = self.trail.unit.plus(low16(v));
            }
            MasterAttribute::TRAIL_MODE => {
                self.trail.mode = self.trail.mode.plus(low8(v));
            }
            MasterAttribute::TRAIL_DENSITY => {
                self.trail.density += TrailDensity::new(v);
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::multiply_moving`]'s values of the layer: what `attribute`
    /// with `v` writes here (the attributes the setter hands over, each one of the layer's values).
    pub(super) fn multiply_value(&mut self, attribute: MasterAttribute, v: f32) {
        match attribute {
            MasterAttribute::ROTATION_SPEED => {
                self.turn_speed *= v;
            }
            MasterAttribute::TRAIL_DENSITY => {
                self.trail.density *= v;
            }
            _ => {}
        }
    }
}
