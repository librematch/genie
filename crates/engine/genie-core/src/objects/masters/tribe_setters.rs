//! The TRIBE combat and building masters' setters: the costs, the charge, the sprites and the
//! train locations.

use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::map::Distance;
use crate::map::TerrainId;
use crate::objects::HitPoints;
use crate::objects::Regeneration;
use crate::objects::masters::AttackPriority;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ChargeType;
use crate::objects::masters::CostAmount;
use crate::objects::masters::DamageAmount;
use crate::objects::masters::FormationClass;
use crate::objects::masters::InvulnerabilityLevel;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::SpecialAttack;
use crate::objects::masters::TrainLocationEntry;
use crate::objects::masters::{
    AttributeWorld, Master, ROUND, add_sprite, attribute, cost_resource, id_of,
    insert_train_location, low8, low16, resize_train_locations, select_entry, set_sprite,
};
use crate::objects::masters::{GarrisonKinds, HeroFlags};
use crate::players::Amount;
use crate::players::attribute as players_attribute;
use crate::production::LocationButton;
use crate::world::WholeSeconds;

impl Master {
    /// The cost entries a cost attribute (100, 103 to 106) changes (`FUN_1412661d0`): those of
    /// its resource, or for 100 the paid ones. The empty slots (−1), which nothing reads, keep
    /// their amounts.
    pub(super) fn costs_of(
        &mut self,
        attribute: MasterAttribute,
    ) -> impl Iterator<Item = &mut CostAmount> {
        let resource = cost_resource(attribute);
        self.cost
            .iter_mut()
            .filter(move |c| {
                c.attribute.is_some() && ((c.paid && resource.is_none()) || c.attribute == resource)
            })
            .map(|c| &mut c.amount)
    }

    /// `FUN_1412661d0`, the combat master's slot 0x18.
    pub(super) fn set_tribe_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HERO_STATUS => self.hero_flag = HeroFlags::from_bits_retain(low8(v)),
            a::TRAIN_LOCATION
            | a::TRAIN_BUTTON
            | a::HOTKEY_ID
            | a::TRAIN_TIME
            | a::TRAIN_LOCATIONS_ENTRY_MOD
            | a::TRAIN_LOCATIONS_TOTAL_NUM => self.set_train_attribute(attribute, v),
            a::SHOWN_PIERCE_ARMOR => self.shown.pierce_armor = DamageAmount::of_effect(v),
            a::MAXIMUM_CHARGE
            | a::RECHARGE_RATE
            | a::CHARGE_EVENT
            | a::CHARGE_TYPE
            | a::CHARGE_PROJECTILE_UNIT => self.set_charge(attribute, v),
            a::SECONDARY_PROJECTILE_UNIT => self.volley_missile = id_of(cvttss2si(v)),
            a::SPECIAL_GRAPHIC
            | a::IDLE_ATTACK_GRAPHIC
            | a::HERO_GLOW_GRAPHIC
            | a::GARRISON_GRAPHIC
            | a::ATTACK_GRAPHIC_2 => self.set_tribe_sprite(attribute, v, world),
            a::SPECIAL_ABILITY => self.special_attack = SpecialAttack::of_data(low8(v)),
            a::RESOURCE_COSTS | a::FOOD_COSTS | a::WOOD_COSTS | a::GOLD_COSTS | a::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = CostAmount::of_data(low16(v));
                }
            }
            a::TOTAL_MISSILES => self.volley_fire = crate::combat::Firepower::new(v),
            a::MAXIMUM_TOTAL_MISSILES => {
                self.max_volley = crate::combat::MissileCount::of_data(low8(v));
            }
            a::REGENERATION_RATE => self.regeneration = Regeneration::new(v),
            a::POPULATION => {
                // the unpaid entries take the negated amount; the storage of population (4) the
                // amount, of resources 11 and 19 its negation
                let negated = -v;
                for c in self
                    .cost
                    .iter_mut()
                    .filter(|c| c.attribute.is_some() && !c.paid)
                {
                    c.amount = CostAmount::of_data(low16(negated));
                }
                for s in &mut self.storage {
                    match s.0 {
                        Some(players_attribute::HEADROOM) => s.1 = Amount::new(v),
                        Some(players_attribute::POPULATION | players_attribute::UNITS_OWNED) => {
                            s.1 = Amount::new(negated);
                        }
                        _ => {}
                    }
                }
            }
            a::MIN_CONVERSION_TIME_MODIFIER
            | a::MAX_CONVERSION_TIME_MODIFIER
            | a::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) = v;
            }
            a::FORMATION_CATEGORY => self.formation_category = FormationClass::of_data(low8(v)),
            a::REGENERATION_HP_PERCENT => {
                self.regeneration =
                    Regeneration::new(HitPoints::of_master(self.hit_points).get() * v);
            }
            a::ATTACK_PRIORITY => self.attack_priority = AttackPriority::of_byte(low8(v)),
            a::INVULNERABILITY_LEVEL => self.invulnerability = InvulnerabilityLevel::new(v),
            a::SPAWNING_AREA_WIDTH | a::SPAWNING_AREA_LENGTH | a::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) = Distance::new(v);
            }
            // the ability's button, tooltips and hotkey action are the interface's
            a::ABILITY_BUTTON_ICON_ID
            | a::ABILITY_SHORT_TOOLTIP_ID
            | a::ABILITY_EXTENDED_TOOLTIP_ID
            | a::ABILITY_HOTKEY_ACTION => {}
            _ => self.set_combat(attribute, v, world),
        }
    }

    /// [`Master::set_tribe_combat`]'s charge (`FUN_1412661d0`): its most, its recharge rate, its
    /// event, its type and its projectile.
    pub(super) fn set_charge(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::MAXIMUM_CHARGE => self.max_charge = Charge::new(v),
            a::RECHARGE_RATE => self.recharge_rate = ChargeRate::new(v),
            a::CHARGE_EVENT => self.charge_event = ChargeEvent::of_data(low16(v)),
            a::CHARGE_TYPE => self.charge_type = ChargeType::of_data(low16(v)),
            a::CHARGE_PROJECTILE_UNIT => self.charge_projectile = id_of(cvttss2si(v)),
            _ => {}
        }
    }

    /// [`Master::set_tribe_combat`]'s sprites (`FUN_1412661d0`): special, idle attack, hero glow,
    /// garrison and second attack, each a sprite the world's table holds.
    pub(super) fn set_tribe_sprite(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::SPECIAL_GRAPHIC => set_sprite(&mut self.special_sprite, cvttss2si(v), world),
            a::IDLE_ATTACK_GRAPHIC => {
                set_sprite(&mut self.idle_attack_sprite, cvttss2si(v), world);
            }
            a::HERO_GLOW_GRAPHIC => set_sprite(&mut self.hero_glow_sprite, cvttss2si(v), world),
            a::GARRISON_GRAPHIC => set_sprite(&mut self.garrison_sprite, cvttss2si(v), world),
            a::ATTACK_GRAPHIC_2 => set_sprite(&mut self.attack_sprite2, cvttss2si(v), world),
            _ => {}
        }
    }

    /// [`Master::set_tribe_combat`]'s train locations (`FUN_1412661d0`): the selected entry's
    /// location, button, hotkey and time, the selection (at most the last) and the count.
    pub(super) fn set_train_attribute(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::TRAIN_LOCATION => {
                if let Some(t) = self.train_location_mut() {
                    t.location = MasterId::of_data(low16(v));
                }
            }
            a::TRAIN_BUTTON => {
                if let Some(t) = self.train_location_mut() {
                    t.button = LocationButton::of_data(low8(v));
                }
            }
            a::HOTKEY_ID => {
                if let Some(t) = self.train_location_mut() {
                    t.hotkey = crate::production::HotkeyId::of_data(cvttss2si(v));
                }
            }
            a::TRAIN_TIME => {
                if let Some(t) = self.train_location_mut() {
                    t.time = WholeSeconds::of_data(low16(v));
                }
            }
            a::TRAIN_LOCATIONS_ENTRY_MOD => {
                if let Some(n) = select_entry(self.train_locations.len(), cvttss2si(v)) {
                    self.train_location_entry = TrainLocationEntry::of_selected(n);
                }
            }
            a::TRAIN_LOCATIONS_TOTAL_NUM => {
                resize_train_locations(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    v,
                );
            }
            _ => {}
        }
    }

    /// `FUN_141266a60`, the combat master's slot 0x20. Its sprite adds read the sprite's id
    /// without the others' test for none (a crash on none, which no recording reaches): none
    /// counts as −1.
    pub(super) fn add_tribe_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::attribute as a;
        match attribute {
            a::HERO_STATUS => {
                self.hero_flag =
                    HeroFlags::from_bits_retain(self.hero_flag.bits().wrapping_add(low8(v)));
            }
            a::SHOWN_PIERCE_ARMOR => {
                self.shown.pierce_armor = self.shown.pierce_armor.plus(DamageAmount::of_effect(v));
            }
            a::MAXIMUM_CHARGE => self.max_charge += Charge::new(v),
            a::RECHARGE_RATE => self.recharge_rate += ChargeRate::new(v),
            a::CHARGE_EVENT => self.charge_event = self.charge_event.plus(low16(v)),
            a::SPECIAL_GRAPHIC => add_sprite(&mut self.special_sprite, v, world),
            a::SPECIAL_ABILITY => {
                self.special_attack =
                    SpecialAttack::of_data(self.special_attack.get().wrapping_add(low8(v)));
            }
            a::IDLE_ATTACK_GRAPHIC => add_sprite(&mut self.idle_attack_sprite, v, world),
            a::HERO_GLOW_GRAPHIC => add_sprite(&mut self.hero_glow_sprite, v, world),
            a::GARRISON_GRAPHIC => add_sprite(&mut self.garrison_sprite, v, world),
            a::RESOURCE_COSTS | a::FOOD_COSTS | a::WOOD_COSTS | a::GOLD_COSTS | a::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = c.plus(low16(v));
                }
            }
            a::TRAIN_TIME => {
                if let Some(t) = self.train_location_mut() {
                    t.time = t.time.plus(low16(v));
                }
            }
            a::TOTAL_MISSILES => self.volley_fire += crate::combat::Firepower::new(v),
            a::MAXIMUM_TOTAL_MISSILES => self.max_volley = self.max_volley.plus(low8(v)),
            a::REGENERATION_RATE => self.regeneration += Regeneration::new(v),
            a::MIN_CONVERSION_TIME_MODIFIER
            | a::MAX_CONVERSION_TIME_MODIFIER
            | a::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) += v;
            }
            a::REGENERATION_HP_PERCENT => {
                self.regeneration +=
                    Regeneration::new(HitPoints::of_master(self.hit_points).get() * v);
            }
            a::INVULNERABILITY_LEVEL => self.invulnerability += InvulnerabilityLevel::new(v),
            // FUN_141533c70 tests the new id; the game takes the sprite from the same table
            a::ATTACK_GRAPHIC_2 => add_sprite(&mut self.attack_sprite2, v, world),
            a::SPAWNING_AREA_WIDTH | a::SPAWNING_AREA_LENGTH | a::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) += Distance::new(v);
            }
            a::TRAIN_LOCATIONS_ENTRY_MOD => {
                let n = cvttss2si(f32::from(self.train_location_entry.get()) + v);
                if let Some(n) = select_entry(self.train_locations.len(), n) {
                    self.train_location_entry = TrainLocationEntry::of_selected(n);
                }
            }
            // the game's: the selected entry plus the amount, not the count
            a::TRAIN_LOCATIONS_TOTAL_NUM => {
                let count = f32::from(self.train_location_entry.get()) + v;
                resize_train_locations(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    count,
                );
            }
            a::ABILITY_BUTTON_ICON_ID
            | a::ABILITY_SHORT_TOOLTIP_ID
            | a::ABILITY_EXTENDED_TOOLTIP_ID
            | a::ABILITY_HOTKEY_ACTION => {}
            _ => self.add_combat(attribute, v, world),
        }
    }

    /// `FUN_141267040`, the combat master's slot 0x28.
    pub(super) fn multiply_tribe_combat(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::SHOWN_PIERCE_ARMOR => {
                self.shown.pierce_armor = self.shown.pierce_armor.times(low16(v));
            }
            a::MAXIMUM_CHARGE => self.max_charge *= v,
            a::RECHARGE_RATE => self.recharge_rate *= v,
            a::RESOURCE_COSTS | a::FOOD_COSTS | a::WOOD_COSTS | a::GOLD_COSTS | a::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = CostAmount::of_data(low16(c.as_f32() * v + ROUND));
                }
            }
            a::TRAIN_TIME => {
                // FUN_140ace5e0 reads 0 past the list; FUN_14126ade0 writes only within it
                if let Some(t) = self.train_location_mut() {
                    t.time = WholeSeconds::of_data(low16(f32::from(t.time.get()) * v + ROUND));
                }
            }
            a::TOTAL_MISSILES => self.volley_fire *= v,
            a::MAXIMUM_TOTAL_MISSILES => {
                self.max_volley = crate::combat::MissileCount::of_data(low8(
                    f32::from(self.max_volley.get()) * v + ROUND,
                ));
            }
            a::REGENERATION_RATE => self.regeneration *= v,
            a::MIN_CONVERSION_TIME_MODIFIER
            | a::MAX_CONVERSION_TIME_MODIFIER
            | a::CONVERSION_CHANCE_MODIFIER => {
                *self.conversion.lane_mut(attribute) *= v;
            }
            a::INVULNERABILITY_LEVEL => self.invulnerability *= v,
            a::SPAWNING_AREA_WIDTH | a::SPAWNING_AREA_LENGTH | a::SPAWNING_AREA_RANDOMNESS => {
                *self.volley_spread.lane_mut(attribute) *= v;
            }
            a::TRAIN_LOCATIONS_ENTRY_MOD => {
                insert_train_location(&mut self.train_locations, &mut self.train_location_entry, v);
            }
            a::TRAIN_LOCATIONS_TOTAL_NUM => {
                let count = self.train_locations.len() as f32 * v;
                resize_train_locations(
                    &mut self.train_locations,
                    &mut self.train_location_entry,
                    count,
                );
            }
            _ => self.multiply_combat(attribute, v, world),
        }
    }

    /// `FUN_14123d310`, the building master's slot 0x18. Its six sprites (85 to 90) go to a copy
    /// of the field (the game's local), which changes nothing; its sounds are the interface's.
    pub(super) fn set_building(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::BUILDING_ICON_OVERRIDE => self.building_facet = low16(v),
            a::GARRISON_TYPE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_type = GarrisonKinds::from_bits_retain(low8(v));
                }
            }
            a::FOUNDATION_TERRAIN => {
                if let Some(b) = self.building.as_mut() {
                    b.foundation_terrain = TerrainId::of_data(low16(v));
                }
            }
            a::GARRISON_HEAL_RATE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_heal_rate = HitPoints::new(v);
                }
            }
            a::CONSTRUCTION_GRAPHIC
            | a::SNOW_GRAPHIC
            | a::DESTRUCTION_GRAPHIC
            | a::DESTRUCTION_RUBBLE_GRAPHIC
            | a::RESEARCHING_GRAPHIC
            | a::RESEARCH_COMPLETED_GRAPHIC
            | a::CONSTRUCTION_SOUND
            | a::CONSTRUCTION_SOUND_EVENT
            | a::TRANSFORM_SOUND
            | a::TRANSFORM_SOUND_EVENT => {}
            _ => self.set_tribe_combat(attribute, v, world),
        }
    }

    /// `FUN_14123d530`, the building master's slot 0x20, as [`Master::set_building`].
    pub(super) fn add_building(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::BUILDING_ICON_OVERRIDE => {
                self.building_facet = self.building_facet.wrapping_add(low16(v));
            }
            a::GARRISON_TYPE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_type = GarrisonKinds::from_bits_retain(
                        b.garrison_type.bits().wrapping_add(low8(v)),
                    );
                }
            }
            a::FOUNDATION_TERRAIN => {
                if let Some(b) = self.building.as_mut() {
                    let was = b.foundation_terrain.map_or(-1, |t| i16::from(t.get()));
                    b.foundation_terrain = TerrainId::of_data(was.wrapping_add(low16(v)));
                }
            }
            a::GARRISON_HEAL_RATE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_heal_rate += HitPoints::new(v);
                }
            }
            a::CONSTRUCTION_GRAPHIC
            | a::SNOW_GRAPHIC
            | a::DESTRUCTION_GRAPHIC
            | a::DESTRUCTION_RUBBLE_GRAPHIC
            | a::RESEARCHING_GRAPHIC
            | a::RESEARCH_COMPLETED_GRAPHIC
            | a::CONSTRUCTION_SOUND
            | a::TRANSFORM_SOUND => {}
            _ => self.add_tribe_combat(attribute, v, world),
        }
    }

    /// `FUN_14123d730`, the building master's slot 0x28.
    pub(super) fn multiply_building(
        &mut self,
        attribute: MasterAttribute,
        v: f32,
        world: &AttributeWorld<'_>,
    ) {
        match (attribute, self.building.as_mut()) {
            (attribute::GARRISON_HEAL_RATE, Some(b)) => {
                b.garrison_heal_rate *= v;
            }
            (attribute::GARRISON_HEAL_RATE, None) => {}
            _ => self.multiply_tribe_combat(attribute, v, world),
        }
    }
}
