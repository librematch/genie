//! The RGE combat and missile masters' setters: the armor and attack amounts.

use crate::combat::missile::{ArcRatio, HitMode, SmartMode, VanishMode};
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::PERCENT;
use crate::objects::masters::ROUND;
use crate::objects::masters::class_amount;
use crate::objects::masters::handicapped;
use crate::objects::masters::low8;
use crate::objects::masters::{ClassAmount, DamageAmount, DamageClass};
use crate::world::Seconds;

impl Master {
    /// The armor (`+0x808`) or attack (`+0x818`) entry of `class`, the first in the list.
    pub(super) fn class_entry(
        list: &mut [ClassAmount],
        class: DamageClass,
    ) -> Option<&mut DamageAmount> {
        list.iter_mut()
            .find(|class_amount| class_amount.class == class)
            .map(|class_amount| &mut class_amount.amount)
    }

    /// `FUN_1410c12b0`, the RGE combat master's slot 0x18: an armor or attack amount
    /// ([`class_amount`]) replaces its class's; an attack class at 0 the handicap raises
    /// ([`handicapped`]) takes the amount times the factor.
    pub(super) fn set_combat(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME
            | MasterAttribute::ACCURACY_PERCENT
            | MasterAttribute::MAXIMUM_RANGE
            | MasterAttribute::BASE_ARMOR
            | MasterAttribute::PROJECTILE_UNIT
            | MasterAttribute::TERRAIN_DEFENSE_BONUS
            | MasterAttribute::MINIMUM_RANGE
            | MasterAttribute::BLAST_WIDTH
            | MasterAttribute::BONUS_DAMAGE_RESISTANCE
            | MasterAttribute::FRAME_DELAY
            | MasterAttribute::BLAST_ATTACK_LEVEL
            | MasterAttribute::SHOWN_ATTACK
            | MasterAttribute::SHOWN_RANGE
            | MasterAttribute::SHOWN_MELEE_ARMOR
            | MasterAttribute::COMBAT_ABILITY
            | MasterAttribute::ATTACK_DISPERSION
            | MasterAttribute::ATTACK_GRAPHIC
            | MasterAttribute::AREA_DAMAGE
            | MasterAttribute::DAMAGE_REFLECTION
            | MasterAttribute::FRIENDLY_FIRE_DAMAGE
            | MasterAttribute::GARRISON_FIREPOWER
            | MasterAttribute::DISPLACEMENT_X
            | MasterAttribute::DISPLACEMENT_Y
            | MasterAttribute::DISPLACEMENT_Z => {
                if let Some(combat_part) = self.combat.as_mut() {
                    combat_part.set_value(attribute, value, world);
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.set_class_amount(attribute, value, world);
            }
            _ => self.set_action(attribute, value, world),
        }
    }

    /// [`Master::set_combat`]'s armor and attack amounts (`FUN_1410c12b0`): the amount
    /// ([`class_amount`]) replaces its class's; an attack class at 0 the handicap raises takes the
    /// amount times the factor.
    pub(super) fn set_class_amount(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.armors, class) {
                        *damage_amount = amount;
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.weapons, class)
                    {
                        *damage_amount =
                            match handicapped(world.handicap, class, world.masters_tuning) {
                                Some(factor) if *damage_amount == DamageAmount::ZERO => {
                                    DamageAmount::of_effect(amount.as_f32() * factor)
                                }
                                _ => amount,
                            };
                    }
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
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME
            | MasterAttribute::ACCURACY_PERCENT
            | MasterAttribute::MAXIMUM_RANGE
            | MasterAttribute::BASE_ARMOR
            | MasterAttribute::PROJECTILE_UNIT
            | MasterAttribute::TERRAIN_DEFENSE_BONUS
            | MasterAttribute::MINIMUM_RANGE
            | MasterAttribute::BLAST_WIDTH
            | MasterAttribute::BONUS_DAMAGE_RESISTANCE
            | MasterAttribute::FRAME_DELAY
            | MasterAttribute::BLAST_ATTACK_LEVEL
            | MasterAttribute::SHOWN_ATTACK
            | MasterAttribute::SHOWN_RANGE
            | MasterAttribute::SHOWN_MELEE_ARMOR
            | MasterAttribute::COMBAT_ABILITY
            | MasterAttribute::ATTACK_DISPERSION
            | MasterAttribute::ATTACK_GRAPHIC
            | MasterAttribute::AREA_DAMAGE
            | MasterAttribute::DAMAGE_REFLECTION
            | MasterAttribute::FRIENDLY_FIRE_DAMAGE
            | MasterAttribute::GARRISON_FIREPOWER
            | MasterAttribute::DISPLACEMENT_X
            | MasterAttribute::DISPLACEMENT_Y
            | MasterAttribute::DISPLACEMENT_Z => {
                if let Some(combat_part) = self.combat.as_mut() {
                    combat_part.add_value(attribute, value, world);
                }
            }
            MasterAttribute::ROTATION_SPEED => {
                if let Some(moving_part) = self.moving.as_mut() {
                    moving_part.turn_speed += Seconds::new(value);
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.add_class_amount(attribute, value, world);
            }
            _ => self.add_action(attribute, value, world),
        }
    }

    /// [`Master::add_combat`]'s armor and attack amounts (`FUN_1410c16e0`): the amount
    /// ([`class_amount`]) adds to its class's; an attack class at 0 the handicap raises takes the
    /// amount times the factor.
    pub(super) fn add_class_amount(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.armors, class) {
                        *damage_amount = damage_amount.plus(amount);
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.weapons, class)
                    {
                        *damage_amount =
                            match handicapped(world.handicap, class, world.masters_tuning) {
                                Some(factor) if *damage_amount == DamageAmount::ZERO => {
                                    DamageAmount::of_effect(amount.as_f32() * factor)
                                }
                                _ => damage_amount.plus(amount),
                            };
                    }
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
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ATTACK_RELOAD_TIME
            | MasterAttribute::ACCURACY_PERCENT
            | MasterAttribute::MAXIMUM_RANGE
            | MasterAttribute::BASE_ARMOR
            | MasterAttribute::PROJECTILE_UNIT
            | MasterAttribute::TERRAIN_DEFENSE_BONUS
            | MasterAttribute::MINIMUM_RANGE
            | MasterAttribute::BLAST_WIDTH
            | MasterAttribute::BONUS_DAMAGE_RESISTANCE
            | MasterAttribute::FRAME_DELAY
            | MasterAttribute::BLAST_ATTACK_LEVEL
            | MasterAttribute::SHOWN_ATTACK
            | MasterAttribute::SHOWN_RANGE
            | MasterAttribute::SHOWN_MELEE_ARMOR
            | MasterAttribute::ATTACK_DISPERSION
            | MasterAttribute::AREA_DAMAGE
            | MasterAttribute::DAMAGE_REFLECTION
            | MasterAttribute::FRIENDLY_FIRE_DAMAGE
            | MasterAttribute::GARRISON_FIREPOWER
            | MasterAttribute::DISPLACEMENT_X
            | MasterAttribute::DISPLACEMENT_Y
            | MasterAttribute::DISPLACEMENT_Z => {
                if let Some(combat_part) = self.combat.as_mut() {
                    combat_part.multiply_value(attribute, value);
                }
            }
            MasterAttribute::ROTATION_SPEED => {
                if let Some(moving_part) = self.moving.as_mut() {
                    moving_part.turn_speed *= value;
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.multiply_class_amount(attribute, value, world);
            }
            _ => self.multiply_action(attribute, value, world),
        }
    }

    /// [`Master::multiply_combat`]'s armor and attack amounts (`FUN_1410c1bf0`): the amount is a
    /// percentage of its class's (`amount / 100 × value + 0.5`, truncated; an attack class at 0
    /// the handicap raises scales the amount by the factor first).
    pub(super) fn multiply_class_amount(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::ARMOR => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.armors, class) {
                        *damage_amount = DamageAmount::of_effect(
                            amount.as_f32() / PERCENT * damage_amount.as_f32() + ROUND,
                        );
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(combat_part) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(value);
                    if let Some(damage_amount) = Self::class_entry(&mut combat_part.weapons, class)
                    {
                        let share = match handicapped(world.handicap, class, world.masters_tuning) {
                            Some(factor) if *damage_amount == DamageAmount::ZERO => {
                                amount.as_f32() * factor
                            }
                            _ => amount.as_f32(),
                        };
                        *damage_amount = DamageAmount::of_effect(
                            damage_amount.as_f32() * (share / PERCENT) + ROUND,
                        );
                    }
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
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(missile_part) = self.missile.as_mut() {
            match attribute {
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    missile_part.smart_mode = SmartMode::from_bits_retain(low8(value));
                }
                MasterAttribute::PROJECTILE_HIT_MODE => {
                    missile_part.hit_mode = HitMode::of_data(low8(value));
                }
                MasterAttribute::PROJECTILE_VANISH_MODE => {
                    missile_part.vanish_mode = VanishMode::from_bits_retain(low8(value));
                }
                MasterAttribute::PROJECTILE_ARC => missile_part.arc = ArcRatio::new(value),
                _ => {}
            }
        }
        self.set_combat(attribute, value, world);
    }

    /// `FUN_1410d5ab0`, the missile master's slot 0x20, as [`Master::set_missile`].
    pub(super) fn add_missile(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(missile_part) = self.missile.as_mut() {
            match attribute {
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    missile_part.smart_mode = SmartMode::from_bits_retain(
                        missile_part.smart_mode.bits().wrapping_add(low8(value)),
                    );
                }
                MasterAttribute::PROJECTILE_HIT_MODE => {
                    missile_part.hit_mode = missile_part.hit_mode.plus(low8(value));
                }
                MasterAttribute::PROJECTILE_VANISH_MODE => {
                    missile_part.vanish_mode = VanishMode::from_bits_retain(
                        missile_part.vanish_mode.bits().wrapping_add(low8(value)),
                    );
                }
                MasterAttribute::PROJECTILE_ARC => missile_part.arc += ArcRatio::new(value),
                _ => {}
            }
        }
        self.add_combat(attribute, value, world);
    }

    /// `FUN_1410d5b50`, the missile master's slot 0x28, as [`Master::set_missile`]; its smart
    /// mode takes the amount (the game's set, not a product).
    pub(super) fn multiply_missile(
        &mut self,
        attribute: MasterAttribute,
        value: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(missile_part) = self.missile.as_mut() {
            match attribute {
                MasterAttribute::PROJECTILE_SMART_MODE => {
                    missile_part.smart_mode = SmartMode::from_bits_retain(low8(value));
                }
                MasterAttribute::PROJECTILE_ARC => missile_part.arc *= value,
                _ => {}
            }
        }
        self.multiply_combat(attribute, value, world);
    }
}
