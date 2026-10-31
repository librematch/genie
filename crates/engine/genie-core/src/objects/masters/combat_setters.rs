//! The RGE combat and missile masters' setters: the armor and attack amounts.

use crate::map::Distance;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::{
    AttributeWorld, Master, PERCENT, ROUND, add_sprite, attribute, class_amount, handicapped, low8,
    low16, set_sprite,
};
use crate::objects::masters::{ClassAmount, DamageClass};
use crate::world::Seconds;

impl Master {
    /// The armor (`+0x808`) or attack (`+0x818`) entry of `class`, the first in the list.
    pub(super) fn class_entry(list: &mut [ClassAmount], class: DamageClass) -> Option<&mut i16> {
        list.iter_mut()
            .find(|e| e.class == class)
            .map(|e| &mut e.amount)
    }

    /// `FUN_1410c12b0`, the RGE combat master's slot 0x18: an armor or attack amount
    /// ([`class_amount`]) replaces its class's; an attack class at 0 the handicap raises
    /// ([`handicapped`]) takes the amount times the factor.
    pub(super) fn set_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ARMOR | a::ATTACK => self.set_class_amount(attribute, v, world),
            a::ATTACK_RELOAD_TIME => self.reload = Seconds::new(v),
            a::ACCURACY_PERCENT => self.hit_chance = low16(v),
            a::MAXIMUM_RANGE => self.weapon_range = Distance::new(v),
            a::BASE_ARMOR => self.base_armor = low16(v),
            a::PROJECTILE_UNIT => self.missile = MasterId::of_data(low16(v)),
            a::TERRAIN_DEFENSE_BONUS => self.terrain_bonus = low16(v),
            a::MINIMUM_RANGE => self.min_range = Distance::new(v),
            a::BLAST_WIDTH => self.area_effect = Distance::new(v),
            a::BONUS_DAMAGE_RESISTANCE => self.resistance = v,
            a::FRAME_DELAY => self.fire_frame = low16(v),
            a::BLAST_ATTACK_LEVEL => self.blast_level = low8(v),
            a::SHOWN_ATTACK => self.shown.attack = low16(v),
            a::SHOWN_RANGE => self.shown.range = Distance::new(v),
            a::SHOWN_MELEE_ARMOR => self.shown.melee_armor = low16(v),
            a::COMBAT_ABILITY => self.combat_ability = low8(v),
            a::ATTACK_DISPERSION => self.missed_spread = v,
            a::ATTACK_GRAPHIC => set_sprite(&mut self.attack_sprite, cvttss2si(v), world),
            a::AREA_DAMAGE => self.area_damage = v,
            a::DAMAGE_REFLECTION => self.reflection = v,
            a::FRIENDLY_FIRE_DAMAGE => self.friendly_fire = v,
            a::GARRISON_FIREPOWER => self.garrison_firepower = v,
            a::DISPLACEMENT_X => self.weapon_offset = self.weapon_offset.with_x(v),
            a::DISPLACEMENT_Y => self.weapon_offset = self.weapon_offset.with_y(v),
            a::DISPLACEMENT_Z => self.weapon_offset = self.weapon_offset.with_z(v),
            _ => self.set_action(attribute, v, world),
        }
    }

    /// [`Master::set_combat`]'s armor and attack amounts (`FUN_1410c12b0`): the amount
    /// ([`class_amount`]) replaces its class's; an attack class at 0 the handicap raises takes the
    /// amount times the factor.
    pub(super) fn set_class_amount(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = amount;
                }
            }
            a::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    *e = match handicapped(world.handicap, class) {
                        Some(f) if *e == 0 => low16(f32::from(amount) * f),
                        _ => amount,
                    };
                }
            }
            _ => {}
        }
    }

    /// `FUN_1410c16e0`, the RGE combat master's slot 0x20: an armor or attack amount adds to its
    /// class's (an attack class at 0 the handicap raises takes the amount times the factor).
    pub(super) fn add_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ROTATION_SPEED => self.turn_speed += Seconds::new(v),
            a::ARMOR | a::ATTACK => self.add_class_amount(attribute, v, world),
            a::ATTACK_RELOAD_TIME => self.reload += Seconds::new(v),
            a::ACCURACY_PERCENT => self.hit_chance = self.hit_chance.wrapping_add(low16(v)),
            a::MAXIMUM_RANGE => self.weapon_range += Distance::new(v),
            a::BASE_ARMOR => self.base_armor = self.base_armor.wrapping_add(low16(v)),
            a::PROJECTILE_UNIT => {
                self.missile =
                    MasterId::of_data(MasterId::field(self.missile).wrapping_add(low16(v)));
            }
            a::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = self.terrain_bonus.wrapping_add(low16(v));
            }
            a::MINIMUM_RANGE => self.min_range += Distance::new(v),
            a::BLAST_WIDTH => self.area_effect += Distance::new(v),
            a::BONUS_DAMAGE_RESISTANCE => self.resistance += v,
            a::FRAME_DELAY => self.fire_frame = self.fire_frame.wrapping_add(low16(v)),
            a::BLAST_ATTACK_LEVEL => self.blast_level = self.blast_level.wrapping_add(low8(v)),
            a::SHOWN_ATTACK => self.shown.attack = self.shown.attack.wrapping_add(low16(v)),
            a::SHOWN_RANGE => self.shown.range += Distance::new(v),
            a::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = self.shown.melee_armor.wrapping_add(low16(v));
            }
            a::COMBAT_ABILITY => {
                self.combat_ability = self.combat_ability.wrapping_add(low8(v));
            }
            a::ATTACK_DISPERSION => self.missed_spread += v,
            a::ATTACK_GRAPHIC => add_sprite(&mut self.attack_sprite, v, world),
            a::AREA_DAMAGE => self.area_damage += v,
            a::DAMAGE_REFLECTION => self.reflection += v,
            a::FRIENDLY_FIRE_DAMAGE => self.friendly_fire += v,
            a::GARRISON_FIREPOWER => self.garrison_firepower += v,
            a::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() + v);
            }
            a::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() + v);
            }
            a::DISPLACEMENT_Z => {
                self.weapon_offset = self.weapon_offset.with_z(self.weapon_offset.z() + v);
            }
            _ => self.add_action(attribute, v, world),
        }
    }

    /// [`Master::add_combat`]'s armor and attack amounts (`FUN_1410c16e0`): the amount
    /// ([`class_amount`]) adds to its class's; an attack class at 0 the handicap raises takes the
    /// amount times the factor.
    pub(super) fn add_class_amount(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = e.wrapping_add(amount);
                }
            }
            a::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    *e = match handicapped(world.handicap, class) {
                        Some(f) if *e == 0 => low16(f32::from(amount) * f),
                        _ => e.wrapping_add(amount),
                    };
                }
            }
            _ => {}
        }
    }

    /// `FUN_1410c1bf0`, the RGE combat master's slot 0x28: an armor or attack amount is a
    /// percentage of its class's (`amount / 100 × value + 0.5`, truncated; an attack class at 0
    /// the handicap raises scales the amount by the factor first).
    pub(super) fn multiply_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ROTATION_SPEED => self.turn_speed *= v,
            a::ARMOR | a::ATTACK => self.multiply_class_amount(attribute, v, world),
            a::ATTACK_RELOAD_TIME => self.reload *= v,
            a::ACCURACY_PERCENT => self.hit_chance = low16(f32::from(self.hit_chance) * v + ROUND),
            a::MAXIMUM_RANGE => self.weapon_range *= v,
            a::BASE_ARMOR => self.base_armor = low16(f32::from(self.base_armor) * v + ROUND),
            a::PROJECTILE_UNIT => {
                self.missile =
                    MasterId::of_data(low16(f32::from(MasterId::field(self.missile)) * v + ROUND));
            }
            a::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = low16(f32::from(self.terrain_bonus) * v + ROUND);
            }
            a::MINIMUM_RANGE => self.min_range *= v,
            a::BLAST_WIDTH => self.area_effect *= v,
            a::BONUS_DAMAGE_RESISTANCE => self.resistance *= v,
            // integer products (`IMUL`) of the truncated amount
            a::FRAME_DELAY => self.fire_frame = self.fire_frame.wrapping_mul(low16(v)),
            a::BLAST_ATTACK_LEVEL => self.blast_level = self.blast_level.wrapping_mul(low8(v)),
            a::SHOWN_ATTACK => self.shown.attack = self.shown.attack.wrapping_mul(low16(v)),
            a::SHOWN_RANGE => self.shown.range *= v,
            a::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = self.shown.melee_armor.wrapping_mul(low16(v));
            }
            a::ATTACK_DISPERSION => self.missed_spread *= v,
            a::AREA_DAMAGE => self.area_damage *= v,
            a::DAMAGE_REFLECTION => self.reflection *= v,
            a::FRIENDLY_FIRE_DAMAGE => self.friendly_fire *= v,
            a::GARRISON_FIREPOWER => self.garrison_firepower *= v,
            a::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() * v);
            }
            a::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() * v);
            }
            a::DISPLACEMENT_Z => {
                self.weapon_offset = self.weapon_offset.with_z(self.weapon_offset.z() * v);
            }
            _ => self.multiply_action(attribute, v, world),
        }
    }

    /// [`Master::multiply_combat`]'s armor and attack amounts (`FUN_1410c1bf0`): the amount is a
    /// percentage of its class's (`amount / 100 × value + 0.5`, truncated; an attack class at 0
    /// the handicap raises scales the amount by the factor first).
    pub(super) fn multiply_class_amount(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = low16(f32::from(amount) / PERCENT * f32::from(*e) + ROUND);
                }
            }
            a::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    let share = match handicapped(world.handicap, class) {
                        Some(f) if *e == 0 => f32::from(amount) * f,
                        _ => f32::from(amount),
                    };
                    *e = low16(f32::from(*e) * (share / PERCENT) + ROUND);
                }
            }
            _ => {}
        }
    }

    /// `FUN_1410d5a20`, the missile master's slot 0x18: its own four values, then always the RGE
    /// combat master's setter (which none of them reaches).
    pub(super) fn set_missile(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(m) = self.missile_stats.as_mut() {
            match attribute {
                attribute::PROJECTILE_SMART_MODE => m.smart_mode = low8(v),
                attribute::PROJECTILE_HIT_MODE => m.hit_mode = low8(v),
                attribute::PROJECTILE_VANISH_MODE => m.vanish_mode = low8(v),
                attribute::PROJECTILE_ARC => m.arc = v,
                _ => {}
            }
        }
        self.set_combat(attribute, v, world);
    }

    /// `FUN_1410d5ab0`, the missile master's slot 0x20, as [`Master::set_missile`].
    pub(super) fn add_missile(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(m) = self.missile_stats.as_mut() {
            match attribute {
                attribute::PROJECTILE_SMART_MODE => {
                    m.smart_mode = m.smart_mode.wrapping_add(low8(v));
                }
                attribute::PROJECTILE_HIT_MODE => m.hit_mode = m.hit_mode.wrapping_add(low8(v)),
                attribute::PROJECTILE_VANISH_MODE => {
                    m.vanish_mode = m.vanish_mode.wrapping_add(low8(v));
                }
                attribute::PROJECTILE_ARC => m.arc += v,
                _ => {}
            }
        }
        self.add_combat(attribute, v, world);
    }

    /// `FUN_1410d5b50`, the missile master's slot 0x28, as [`Master::set_missile`]; its smart
    /// mode takes the amount (the game's set, not a product).
    pub(super) fn multiply_missile(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(m) = self.missile_stats.as_mut() {
            match attribute {
                attribute::PROJECTILE_SMART_MODE => m.smart_mode = low8(v),
                attribute::PROJECTILE_ARC => m.arc *= v,
                _ => {}
            }
        }
        self.multiply_combat(attribute, v, world);
    }
}
