//! The RGE combat and missile masters' setters: the armor and attack amounts.

use crate::combat::Firepower;
use crate::combat::missile::{ArcRatio, HitMode, MissSpread, SmartMode, VanishMode};
use crate::map::Distance;
use crate::objects::Frame;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::CombatAbility;
use crate::objects::masters::DamageShare;
use crate::objects::masters::HitChance;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::PERCENT;
use crate::objects::masters::ROUND;
use crate::objects::masters::add_sprite;
use crate::objects::masters::class_amount;
use crate::objects::masters::handicapped;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::set_sprite;
use crate::objects::masters::{ClassAmount, DamageAmount, DamageClass};
use crate::objects::masters::{MasterField, RestrictionField};
use crate::world::Seconds;

impl Master {
    /// The armor (`+0x808`) or attack (`+0x818`) entry of `class`, the first in the list.
    pub(super) fn class_entry(
        list: &mut [ClassAmount],
        class: DamageClass,
    ) -> Option<&mut DamageAmount> {
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.set_class_amount(attribute, v, world);
            }
            MasterAttribute::ATTACK_RELOAD_TIME => self.reload = Seconds::new(v),
            MasterAttribute::ACCURACY_PERCENT => self.hit_chance = HitChance::of_data(low16(v)),
            MasterAttribute::MAXIMUM_RANGE => self.weapon_range = Distance::new(v),
            MasterAttribute::BASE_ARMOR => self.base_armor = DamageAmount::of_effect(v),
            MasterAttribute::PROJECTILE_UNIT => self.missile = MasterField::of_data(low16(v)),
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = RestrictionField::of_data(low16(v));
            }
            MasterAttribute::MINIMUM_RANGE => self.min_range = Distance::new(v),
            MasterAttribute::BLAST_WIDTH => self.area_effect = Distance::new(v),
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => self.resistance = DamageShare::new(v),
            MasterAttribute::FRAME_DELAY => self.fire_frame = Frame::of_data(low16(v)),
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level = BlastLevel::from_bits_retain(low8(v));
            }
            MasterAttribute::SHOWN_ATTACK => self.shown.attack = DamageAmount::of_effect(v),
            MasterAttribute::SHOWN_RANGE => self.shown.range = Distance::new(v),
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = DamageAmount::of_effect(v);
            }
            MasterAttribute::COMBAT_ABILITY => {
                self.combat_ability = CombatAbility::from_bits_retain(low8(v));
            }
            MasterAttribute::ATTACK_DISPERSION => self.missed_spread = MissSpread::new(v),
            MasterAttribute::ATTACK_GRAPHIC => {
                set_sprite(&mut self.attack_sprite, cvttss2si(v), world);
            }
            MasterAttribute::AREA_DAMAGE => self.area_damage = DamageShare::new(v),
            MasterAttribute::DAMAGE_REFLECTION => self.reflection = DamageShare::new(v),
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => self.friendly_fire = DamageShare::new(v),
            MasterAttribute::GARRISON_FIREPOWER => self.garrison_firepower = Firepower::new(v),
            MasterAttribute::DISPLACEMENT_X => self.weapon_offset = self.weapon_offset.with_x(v),
            MasterAttribute::DISPLACEMENT_Y => self.weapon_offset = self.weapon_offset.with_y(v),
            MasterAttribute::DISPLACEMENT_Z => self.weapon_offset = self.weapon_offset.with_z(v),
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = amount;
                }
            }
            MasterAttribute::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    *e = match handicapped(world.handicap, class, world.masters_tuning) {
                        Some(f) if *e == DamageAmount::ZERO => {
                            DamageAmount::of_effect(amount.as_f32() * f)
                        }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ROTATION_SPEED => self.turn_speed += Seconds::new(v),
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.add_class_amount(attribute, v, world);
            }
            MasterAttribute::ATTACK_RELOAD_TIME => self.reload += Seconds::new(v),
            MasterAttribute::ACCURACY_PERCENT => self.hit_chance = self.hit_chance.plus(low16(v)),
            MasterAttribute::MAXIMUM_RANGE => self.weapon_range += Distance::new(v),
            MasterAttribute::BASE_ARMOR => {
                self.base_armor = self.base_armor.plus(DamageAmount::of_effect(v));
            }
            MasterAttribute::PROJECTILE_UNIT => {
                self.missile = self.missile.plus(low16(v));
            }
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = self.terrain_bonus.plus(low16(v));
            }
            MasterAttribute::MINIMUM_RANGE => self.min_range += Distance::new(v),
            MasterAttribute::BLAST_WIDTH => self.area_effect += Distance::new(v),
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => self.resistance += DamageShare::new(v),
            MasterAttribute::FRAME_DELAY => self.fire_frame = self.fire_frame.plus(low16(v)),
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level =
                    BlastLevel::from_bits_retain(self.blast_level.bits().wrapping_add(low8(v)));
            }
            MasterAttribute::SHOWN_ATTACK => {
                self.shown.attack = self.shown.attack.plus(DamageAmount::of_effect(v));
            }
            MasterAttribute::SHOWN_RANGE => self.shown.range += Distance::new(v),
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = self.shown.melee_armor.plus(DamageAmount::of_effect(v));
            }
            MasterAttribute::COMBAT_ABILITY => {
                self.combat_ability = CombatAbility::from_bits_retain(
                    self.combat_ability.bits().wrapping_add(low8(v)),
                );
            }
            MasterAttribute::ATTACK_DISPERSION => self.missed_spread += MissSpread::new(v),
            MasterAttribute::ATTACK_GRAPHIC => add_sprite(&mut self.attack_sprite, v, world),
            MasterAttribute::AREA_DAMAGE => self.area_damage += DamageShare::new(v),
            MasterAttribute::DAMAGE_REFLECTION => self.reflection += DamageShare::new(v),
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => self.friendly_fire += DamageShare::new(v),
            MasterAttribute::GARRISON_FIREPOWER => self.garrison_firepower += Firepower::new(v),
            MasterAttribute::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() + v);
            }
            MasterAttribute::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() + v);
            }
            MasterAttribute::DISPLACEMENT_Z => {
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = e.plus(amount);
                }
            }
            MasterAttribute::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    *e = match handicapped(world.handicap, class, world.masters_tuning) {
                        Some(f) if *e == DamageAmount::ZERO => {
                            DamageAmount::of_effect(amount.as_f32() * f)
                        }
                        _ => e.plus(amount),
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ROTATION_SPEED => self.turn_speed *= v,
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.multiply_class_amount(attribute, v, world);
            }
            MasterAttribute::ATTACK_RELOAD_TIME => self.reload *= v,
            MasterAttribute::ACCURACY_PERCENT => {
                self.hit_chance =
                    HitChance::of_data(low16(f32::from(self.hit_chance.get()) * v + ROUND));
            }
            MasterAttribute::MAXIMUM_RANGE => self.weapon_range *= v,
            MasterAttribute::BASE_ARMOR => {
                self.base_armor = DamageAmount::of_effect(self.base_armor.as_f32() * v + ROUND);
            }
            MasterAttribute::PROJECTILE_UNIT => {
                self.missile =
                    MasterField::of_data(low16(f32::from(self.missile.get()) * v + ROUND));
            }
            MasterAttribute::TERRAIN_DEFENSE_BONUS => {
                self.terrain_bonus = RestrictionField::of_data(low16(
                    f32::from(self.terrain_bonus.get()) * v + ROUND,
                ));
            }
            MasterAttribute::MINIMUM_RANGE => self.min_range *= v,
            MasterAttribute::BLAST_WIDTH => self.area_effect *= v,
            MasterAttribute::BONUS_DAMAGE_RESISTANCE => self.resistance *= v,
            // integer products (`IMUL`) of the truncated amount
            MasterAttribute::FRAME_DELAY => self.fire_frame = self.fire_frame.times(low16(v)),
            MasterAttribute::BLAST_ATTACK_LEVEL => {
                self.blast_level =
                    BlastLevel::from_bits_retain(self.blast_level.bits().wrapping_mul(low8(v)));
            }
            MasterAttribute::SHOWN_ATTACK => self.shown.attack = self.shown.attack.times(low16(v)),
            MasterAttribute::SHOWN_RANGE => self.shown.range *= v,
            MasterAttribute::SHOWN_MELEE_ARMOR => {
                self.shown.melee_armor = self.shown.melee_armor.times(low16(v));
            }
            MasterAttribute::ATTACK_DISPERSION => self.missed_spread *= v,
            MasterAttribute::AREA_DAMAGE => self.area_damage *= v,
            MasterAttribute::DAMAGE_REFLECTION => self.reflection *= v,
            MasterAttribute::FRIENDLY_FIRE_DAMAGE => self.friendly_fire *= v,
            MasterAttribute::GARRISON_FIREPOWER => self.garrison_firepower *= v,
            MasterAttribute::DISPLACEMENT_X => {
                self.weapon_offset = self.weapon_offset.with_x(self.weapon_offset.x() * v);
            }
            MasterAttribute::DISPLACEMENT_Y => {
                self.weapon_offset = self.weapon_offset.with_y(self.weapon_offset.y() * v);
            }
            MasterAttribute::DISPLACEMENT_Z => {
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.armors, class) {
                    *e = DamageAmount::of_effect(amount.as_f32() / PERCENT * e.as_f32() + ROUND);
                }
            }
            MasterAttribute::ATTACK => {
                let ClassAmount { class, amount } = class_amount(v);
                if let Some(e) = Self::class_entry(&mut self.weapons, class) {
                    let share = match handicapped(world.handicap, class, world.masters_tuning) {
                        Some(f) if *e == DamageAmount::ZERO => amount.as_f32() * f,
                        _ => amount.as_f32(),
                    };
                    *e = DamageAmount::of_effect(e.as_f32() * (share / PERCENT) + ROUND);
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
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    m.smart_mode = SmartMode::from_bits_retain(low8(v));
                }
                MasterAttribute::PROJECTILE_HIT_MODE => m.hit_mode = HitMode::of_data(low8(v)),
                MasterAttribute::PROJECTILE_VANISH_MODE => {
                    m.vanish_mode = VanishMode::from_bits_retain(low8(v));
                }
                MasterAttribute::PROJECTILE_ARC => m.arc = ArcRatio::new(v),
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
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    m.smart_mode =
                        SmartMode::from_bits_retain(m.smart_mode.bits().wrapping_add(low8(v)));
                }
                MasterAttribute::PROJECTILE_HIT_MODE => m.hit_mode = m.hit_mode.plus(low8(v)),
                MasterAttribute::PROJECTILE_VANISH_MODE => {
                    m.vanish_mode =
                        VanishMode::from_bits_retain(m.vanish_mode.bits().wrapping_add(low8(v)));
                }
                MasterAttribute::PROJECTILE_ARC => m.arc += ArcRatio::new(v),
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
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    m.smart_mode = SmartMode::from_bits_retain(low8(v));
                }
                MasterAttribute::PROJECTILE_ARC => m.arc *= v,
                _ => {}
            }
        }
        self.multiply_combat(attribute, v, world);
    }
}
