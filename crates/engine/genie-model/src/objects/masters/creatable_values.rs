//! The creatable layer's values under the TRIBE combat master's setters (slots 0x18, 0x20 and
//! 0x28): each attribute whose set, add or scale writes one value of the layer. The setters
//! ([`crate::objects::masters::Master::set_tribe_combat`] …) take the layer once and hand these
//! attributes over.

use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::combat::{Firepower, MissileCount};
use crate::map::Distance;
use crate::objects::HitPoints;
use crate::objects::MasterHitPoints;
use crate::objects::Regeneration;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::CreatablePart;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::FormationClass;
use crate::objects::masters::HeroFlags;
use crate::objects::masters::InvulnerabilityLevel;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::ROUND;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::TrainLocationEntry;
use crate::objects::masters::add_sprite;
use crate::objects::masters::id_of;
use crate::objects::masters::insert_train_location;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::resize_train_locations;
use crate::objects::masters::select_entry;

impl CreatablePart {
    /// [`crate::objects::masters::Master::set_tribe_combat`]'s values of the layer
    /// (`FUN_1412661d0`): what `attribute` set to `value` writes here; `hit_points` is the
    /// master's, which the regeneration percent reads.
    pub(super) fn set_value(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        hit_points: MasterHitPoints,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HERO_STATUS => {
                self.hero_flag = HeroFlags::from_bits_retain(low8(value));
            }
            MasterAttribute::SHOWN_PIERCE_ARMOR => {
                self.pierce_armor_shown = DamageAmount::of_effect(value);
            }
            MasterAttribute::SECONDARY_PROJECTILE_UNIT => {
                self.volley_missile = id_of(cvttss2si(value));
            }
            MasterAttribute::SPECIAL_ABILITY => {
                self.special_attack = SpecialAttack::of_data(low8(value));
            }
            MasterAttribute::TOTAL_MISSILES => {
                self.volley_fire = Firepower::new(value);
            }
            MasterAttribute::MAXIMUM_TOTAL_MISSILES => {
                self.max_volley = MissileCount::of_data(low8(value));
            }
            MasterAttribute::REGENERATION_RATE => {
                self.regeneration = Regeneration::new(value);
            }
            MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) = value;
            }
            MasterAttribute::FORMATION_CATEGORY => {
                self.formation_category = FormationClass::of_data(low8(value));
            }
            MasterAttribute::REGENERATION_HP_PERCENT => {
                self.regeneration =
                    Regeneration::new(HitPoints::of_master(hit_points).get() * value);
            }
            MasterAttribute::ATTACK_PRIORITY => {
                self.attack_priority = AttackPriority::of_byte(low8(value));
            }
            MasterAttribute::INVULNERABILITY_LEVEL => {
                self.invulnerability = InvulnerabilityLevel::new(value);
            }
            MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) = Distance::new(value);
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::add_tribe_combat`]'s values of the layer
    /// (`FUN_141266a60`): what an add of `value` to `attribute` writes here; `hit_points` is the
    /// master's, `world` holds the sprites. A sprite add reads the sprite's id without the others'
    /// test for none: none counts as −1.
    pub(super) fn add_value(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        hit_points: MasterHitPoints,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HERO_STATUS => {
                self.hero_flag =
                    HeroFlags::from_bits_retain(self.hero_flag.bits().wrapping_add(low8(value)));
            }
            MasterAttribute::SHOWN_PIERCE_ARMOR => {
                self.pierce_armor_shown =
                    self.pierce_armor_shown.plus(DamageAmount::of_effect(value));
            }
            MasterAttribute::MAXIMUM_CHARGE => {
                self.max_charge += Charge::new(value);
            }
            MasterAttribute::RECHARGE_RATE => {
                self.recharge_rate += ChargeRate::new(value);
            }
            MasterAttribute::CHARGE_EVENT => {
                self.charge_event = self.charge_event.plus(low16(value));
            }
            MasterAttribute::SPECIAL_GRAPHIC => {
                add_sprite(&mut self.special_sprite, value, world);
            }
            MasterAttribute::SPECIAL_ABILITY => {
                self.special_attack =
                    SpecialAttack::of_data(self.special_attack.get().wrapping_add(low8(value)));
            }
            MasterAttribute::IDLE_ATTACK_GRAPHIC => {
                add_sprite(&mut self.idle_attack_sprite, value, world);
            }
            MasterAttribute::HERO_GLOW_GRAPHIC => {
                add_sprite(&mut self.hero_glow_sprite, value, world);
            }
            MasterAttribute::GARRISON_GRAPHIC => {
                add_sprite(&mut self.garrison_sprite, value, world);
            }
            MasterAttribute::TOTAL_MISSILES => {
                self.volley_fire += Firepower::new(value);
            }
            MasterAttribute::MAXIMUM_TOTAL_MISSILES => {
                self.max_volley = self.max_volley.plus(low8(value));
            }
            MasterAttribute::REGENERATION_RATE => {
                self.regeneration += Regeneration::new(value);
            }
            MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) += value;
            }
            MasterAttribute::REGENERATION_HP_PERCENT => {
                self.regeneration +=
                    Regeneration::new(HitPoints::of_master(hit_points).get() * value);
            }
            MasterAttribute::INVULNERABILITY_LEVEL => {
                self.invulnerability += InvulnerabilityLevel::new(value);
            }
            MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) += Distance::new(value);
            }
            MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD => {
                let entry = cvttss2si(f32::from(self.train_location_entry.get()) + value);
                if let Some(selected) = select_entry(self.train_locations.len(), entry) {
                    self.train_location_entry = TrainLocationEntry::of_selected(selected);
                }
            }
            // the game's: the selected entry plus the amount, not the count
            MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                let count = f32::from(self.train_location_entry.get()) + value;
                resize_train_locations(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    count,
                );
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::multiply_tribe_combat`]'s values of the layer
    /// (`FUN_141267040`): what a scale of `attribute` by `value` writes here.
    pub(super) fn multiply_value(&mut self, attribute: MasterAttribute, value: f32) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::SHOWN_PIERCE_ARMOR => {
                self.pierce_armor_shown = self.pierce_armor_shown.times(low16(value));
            }
            MasterAttribute::MAXIMUM_CHARGE => {
                self.max_charge *= value;
            }
            MasterAttribute::RECHARGE_RATE => {
                self.recharge_rate *= value;
            }
            MasterAttribute::TOTAL_MISSILES => {
                self.volley_fire *= value;
            }
            MasterAttribute::MAXIMUM_TOTAL_MISSILES => {
                self.max_volley =
                    MissileCount::of_data(low8(f32::from(self.max_volley.get()) * value + ROUND));
            }
            MasterAttribute::REGENERATION_RATE => {
                self.regeneration *= value;
            }
            MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) *= value;
            }
            MasterAttribute::INVULNERABILITY_LEVEL => {
                self.invulnerability *= value;
            }
            MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) *= value;
            }
            MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD => {
                insert_train_location(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    value,
                );
            }
            MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                let count = self.train_locations.len() as f32 * value;
                resize_train_locations(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    count,
                );
            }
            _ => {}
        }
    }
}
