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
                if let Some(p) = self.combat.as_mut() {
                    p.set_value(attribute, v, world);
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.set_class_amount(attribute, v, world);
            }
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
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.armors, class) {
                        *e = amount;
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.weapons, class) {
                        *e = match handicapped(world.handicap, class, world.masters_tuning) {
                            Some(f) if *e == DamageAmount::ZERO => {
                                DamageAmount::of_effect(amount.as_f32() * f)
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
        v: f32,
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
                if let Some(p) = self.combat.as_mut() {
                    p.add_value(attribute, v, world);
                }
            }
            MasterAttribute::ROTATION_SPEED => {
                if let Some(p) = self.moving.as_mut() {
                    p.turn_speed += Seconds::new(v);
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.add_class_amount(attribute, v, world);
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
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.armors, class) {
                        *e = e.plus(amount);
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.weapons, class) {
                        *e = match handicapped(world.handicap, class, world.masters_tuning) {
                            Some(f) if *e == DamageAmount::ZERO => {
                                DamageAmount::of_effect(amount.as_f32() * f)
                            }
                            _ => e.plus(amount),
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
        v: f32,
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
                if let Some(p) = self.combat.as_mut() {
                    p.multiply_value(attribute, v);
                }
            }
            MasterAttribute::ROTATION_SPEED => {
                if let Some(p) = self.moving.as_mut() {
                    p.turn_speed *= v;
                }
            }
            MasterAttribute::ARMOR | MasterAttribute::ATTACK => {
                self.multiply_class_amount(attribute, v, world);
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
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.armors, class) {
                        *e =
                            DamageAmount::of_effect(amount.as_f32() / PERCENT * e.as_f32() + ROUND);
                    }
                }
            }
            MasterAttribute::ATTACK => {
                if let Some(p) = self.combat.as_mut() {
                    let ClassAmount { class, amount } = class_amount(v);
                    if let Some(e) = Self::class_entry(&mut p.weapons, class) {
                        let share = match handicapped(world.handicap, class, world.masters_tuning) {
                            Some(f) if *e == DamageAmount::ZERO => amount.as_f32() * f,
                            _ => amount.as_f32(),
                        };
                        *e = DamageAmount::of_effect(e.as_f32() * (share / PERCENT) + ROUND);
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
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        if let Some(m) = self.missile.as_mut() {
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
        if let Some(m) = self.missile.as_mut() {
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
        if let Some(m) = self.missile.as_mut() {
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
