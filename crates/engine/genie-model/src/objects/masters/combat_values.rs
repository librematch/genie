//! The combat layer's values under the RGE combat master's setters (slots 0x18, 0x20 and 0x28):
//! each attribute whose set, add or scale writes one value of the layer. The setters
//! ([`crate::objects::masters::Master::set_combat`] …) take the layer once and hand these
//! attributes over.

use crate::combat::Firepower;
use crate::combat::missile::MissSpread;
use crate::map::Distance;
use crate::objects::Frame;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::CombatPart;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::DamageShare;
use crate::objects::masters::HitChance;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::ROUND;
use crate::objects::masters::add_sprite;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::set_sprite;
use crate::objects::masters::{MasterField, RestrictionField};
use crate::world::Seconds;

impl CombatPart {
    /// [`crate::objects::masters::Master::set_combat`]'s values of the layer (`FUN_1410c12b0`):
    /// what `attribute` set to `value` writes here; `world` holds the sprites.
    pub(super) fn set_value(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME => {
                self.reload = Seconds::new(value);
            }
            MasterAttribute::ACCURACY_PERCENT => {
                self.hit_chance = HitChance::of_data(low16(value));
            }
            MasterAttribute::MAXIMUM_RANGE => {
                self.weapon_range = Distance::new(value);
            }
            MasterAttribute::BASE_ARMOR => {
                self.base_armor = DamageAmount::of_effect(value);
            }
            MasterAttribute::PROJECTILE_UNIT => {
                self.missile = MasterField::of_data(low16(value));
            }
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = RestrictionField::of_data(low16(value));
            }
            MasterAttribute::MINIMUM_RANGE => {
                self.min_range = Distance::new(value);
            }
            MasterAttribute::BLAST_WIDTH => {
                self.area_effect = Distance::new(value);
            }
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => {
                self.resistance = DamageShare::new(value);
            }
            MasterAttribute::FRAME_DELAY => {
                self.fire_frame = Frame::of_data(low16(value));
            }
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level = BlastLevel::from_bits_retain(low8(value));
            }
            MasterAttribute::SHOWN_ATTACK => {
                self.shown.attack = DamageAmount::of_effect(value);
            }
            MasterAttribute::SHOWN_RANGE => {
                self.shown.range = Distance::new(value);
            }
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = DamageAmount::of_effect(value);
            }
            MasterAttribute::COMBAT_ABILITY => {
                self.combat_ability = CombatAbility::from_bits_retain(low8(value));
            }
            MasterAttribute::ATTACK_DISPERSION => {
                self.missed_spread = MissSpread::new(value);
            }
            MasterAttribute::ATTACK_GRAPHIC => {
                set_sprite(&mut self.attack_sprite, cvttss2si(value), world);
            }
            MasterAttribute::AREA_DAMAGE => {
                self.area_damage = DamageShare::new(value);
            }
            MasterAttribute::DAMAGE_REFLECTION => {
                self.reflection = DamageShare::new(value);
            }
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => {
                self.friendly_fire = DamageShare::new(value);
            }
            MasterAttribute::GARRISON_FIREPOWER => {
                self.garrison_firepower = Firepower::new(value);
            }
            MasterAttribute::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(value);
            }
            MasterAttribute::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(value);
            }
            MasterAttribute::DISPLACEMENT_Z => {
                self.weapon_offset = self.weapon_offset.with_z(value);
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::add_combat`]'s values of the layer (`FUN_1410c16e0`):
    /// what an add of `value` to `attribute` writes here; `world` holds the sprites.
    pub(super) fn add_value(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME => {
                self.reload += Seconds::new(value);
            }
            MasterAttribute::ACCURACY_PERCENT => {
                self.hit_chance = self.hit_chance.plus(low16(value));
            }
            MasterAttribute::MAXIMUM_RANGE => {
                self.weapon_range += Distance::new(value);
            }
            MasterAttribute::BASE_ARMOR => {
                self.base_armor = self.base_armor.plus(DamageAmount::of_effect(value));
            }
            MasterAttribute::PROJECTILE_UNIT => {
                self.missile = self.missile.plus(low16(value));
            }
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = self.terrain_bonus.plus(low16(value));
            }
            MasterAttribute::MINIMUM_RANGE => {
                self.min_range += Distance::new(value);
            }
            MasterAttribute::BLAST_WIDTH => {
                self.area_effect += Distance::new(value);
            }
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => {
                self.resistance += DamageShare::new(value);
            }
            MasterAttribute::FRAME_DELAY => {
                self.fire_frame = self.fire_frame.plus(low16(value));
            }
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level =
                    BlastLevel::from_bits_retain(self.blast_level.bits().wrapping_add(low8(value)));
            }
            MasterAttribute::SHOWN_ATTACK => {
                self.shown.attack = self.shown.attack.plus(DamageAmount::of_effect(value));
            }
            MasterAttribute::SHOWN_RANGE => {
                self.shown.range += Distance::new(value);
            }
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor =
                    self.shown.melee_armor.plus(DamageAmount::of_effect(value));
            }
            MasterAttribute::COMBAT_ABILITY => {
                self.combat_ability = CombatAbility::from_bits_retain(
                    self.combat_ability.bits().wrapping_add(low8(value)),
                );
            }
            MasterAttribute::ATTACK_DISPERSION => {
                self.missed_spread += MissSpread::new(value);
            }
            MasterAttribute::ATTACK_GRAPHIC => {
                add_sprite(&mut self.attack_sprite, value, world);
            }
            MasterAttribute::AREA_DAMAGE => {
                self.area_damage += DamageShare::new(value);
            }
            MasterAttribute::DAMAGE_REFLECTION => {
                self.reflection += DamageShare::new(value);
            }
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => {
                self.friendly_fire += DamageShare::new(value);
            }
            MasterAttribute::GARRISON_FIREPOWER => {
                self.garrison_firepower += Firepower::new(value);
            }
            MasterAttribute::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() + value);
            }
            MasterAttribute::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() + value);
            }
            MasterAttribute::DISPLACEMENT_Z => {
                self.weapon_offset = self.weapon_offset.with_z(self.weapon_offset.z() + value);
            }
            _ => {}
        }
    }

    /// [`crate::objects::masters::Master::multiply_combat`]'s values of the layer
    /// (`FUN_1410c1bf0`): what a scale of `attribute` by `value` writes here.
    pub(super) fn multiply_value(&mut self, attribute: MasterAttribute, value: f32) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME => {
                self.reload *= value;
            }
            MasterAttribute::ACCURACY_PERCENT => {
                self.hit_chance =
                    HitChance::of_data(low16(f32::from(self.hit_chance.get()) * value + ROUND));
            }
            MasterAttribute::MAXIMUM_RANGE => {
                self.weapon_range *= value;
            }
            MasterAttribute::BASE_ARMOR => {
                self.base_armor = DamageAmount::of_effect(self.base_armor.as_f32() * value + ROUND);
            }
            MasterAttribute::PROJECTILE_UNIT => {
                self.missile =
                    MasterField::of_data(low16(f32::from(self.missile.get()) * value + ROUND));
            }
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = RestrictionField::of_data(low16(
                    f32::from(self.terrain_bonus.get()) * value + ROUND,
                ));
            }
            MasterAttribute::MINIMUM_RANGE => {
                self.min_range *= value;
            }
            MasterAttribute::BLAST_WIDTH => {
                self.area_effect *= value;
            }
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => {
                self.resistance *= value;
            }
            // integer products (`IMUL`) of the truncated amount
            MasterAttribute::FRAME_DELAY => {
                self.fire_frame = self.fire_frame.times(low16(value));
            }
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level =
                    BlastLevel::from_bits_retain(self.blast_level.bits().wrapping_mul(low8(value)));
            }
            MasterAttribute::SHOWN_ATTACK => {
                self.shown.attack = self.shown.attack.times(low16(value));
            }
            MasterAttribute::SHOWN_RANGE => {
                self.shown.range *= value;
            }
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = self.shown.melee_armor.times(low16(value));
            }
            MasterAttribute::ATTACK_DISPERSION => {
                self.missed_spread *= value;
            }
            MasterAttribute::AREA_DAMAGE => {
                self.area_damage *= value;
            }
            MasterAttribute::DAMAGE_REFLECTION => {
                self.reflection *= value;
            }
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => {
                self.friendly_fire *= value;
            }
            MasterAttribute::GARRISON_FIREPOWER => {
                self.garrison_firepower *= value;
            }
            MasterAttribute::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() * value);
            }
            MasterAttribute::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() * value);
            }
            MasterAttribute::DISPLACEMENT_Z => {
                self.weapon_offset = self.weapon_offset.with_z(self.weapon_offset.z() * value);
            }
            _ => {}
        }
    }
}
