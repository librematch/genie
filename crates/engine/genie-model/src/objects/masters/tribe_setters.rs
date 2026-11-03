//! The TRIBE combat and building masters' setters: the costs, the charge, the sprites and the
//! train locations.

use crate::combat::Charge;
use crate::combat::ChargeRate;
use crate::layer::AttributeRoles;
use crate::map::TerrainId;
use crate::objects::HitPoints;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::ChargeEvent;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::CostAmount;
use crate::objects::masters::GarrisonKinds;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::ROUND;
use crate::objects::masters::TrainLocationEntry;
use crate::objects::masters::add_sprite;
use crate::objects::masters::cost_resource;
use crate::objects::masters::id_of;
use crate::objects::masters::low8;
use crate::objects::masters::low16;
use crate::objects::masters::resize_train_locations;
use crate::objects::masters::select_entry;
use crate::objects::masters::set_sprite;
use crate::players::Amount;
use crate::production::HotkeyId;
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
        self.creatable
            .iter_mut()
            .flat_map(|p| p.cost.iter_mut())
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HERO_STATUS
            | MasterAttribute::SHOWN_PIERCE_ARMOR
            | MasterAttribute::SECONDARY_PROJECTILE_UNIT
            | MasterAttribute::SPECIAL_ABILITY
            | MasterAttribute::TOTAL_MISSILES
            | MasterAttribute::MAXIMUM_TOTAL_MISSILES
            | MasterAttribute::REGENERATION_RATE
            | MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER
            | MasterAttribute::FORMATION_CATEGORY
            | MasterAttribute::REGENERATION_HP_PERCENT
            | MasterAttribute::ATTACK_PRIORITY
            | MasterAttribute::INVULNERABILITY_LEVEL
            | MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS => {
                let hit_points = self.hit_points;
                if let Some(p) = self.creatable.as_mut() {
                    p.set_value(attribute, v, hit_points);
                }
            }
            MasterAttribute::TRAIN_LOCATION
            | MasterAttribute::TRAIN_BUTTON
            | MasterAttribute::HOTKEY_ID
            | MasterAttribute::TRAIN_TIME
            | MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD
            | MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => self.set_train_attribute(attribute, v),
            MasterAttribute::MAXIMUM_CHARGE
            | MasterAttribute::RECHARGE_RATE
            | MasterAttribute::CHARGE_EVENT
            | MasterAttribute::CHARGE_TYPE
            | MasterAttribute::CHARGE_PROJECTILE_UNIT => self.set_charge(attribute, v),
            MasterAttribute::SPECIAL_GRAPHIC
            | MasterAttribute::IDLE_ATTACK_GRAPHIC
            | MasterAttribute::HERO_GLOW_GRAPHIC
            | MasterAttribute::GARRISON_GRAPHIC
            | MasterAttribute::ATTACK_GRAPHIC_2 => self.set_tribe_sprite(attribute, v, world),
            MasterAttribute::RESOURCE_COSTS
            | MasterAttribute::FOOD_COSTS
            | MasterAttribute::WOOD_COSTS
            | MasterAttribute::GOLD_COSTS
            | MasterAttribute::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = CostAmount::of_data(low16(v));
                }
            }
            MasterAttribute::POPULATION => {
                // the unpaid entries take the negated amount; the storage of population (4) the
                // amount, of resources 11 and 19 its negation
                let negated = -v;
                for c in self
                    .creatable
                    .iter_mut()
                    .flat_map(|p| p.cost.iter_mut())
                    .filter(|c| c.attribute.is_some() && !c.paid)
                {
                    c.amount = CostAmount::of_data(low16(negated));
                }
                for s in &mut self.storage {
                    let held = world.roles.of_some_attribute(s.0);
                    if held.contains(AttributeRoles::HEADROOM) {
                        s.1 = Amount::new(v);
                    } else if held.contains(AttributeRoles::MAGNITUDE_STORAGE) {
                        s.1 = Amount::new(negated);
                    }
                }
            }
            MasterAttribute::ABILITY_BUTTON_ICON_ID
            | MasterAttribute::ABILITY_SHORT_TOOLTIP_ID
            | MasterAttribute::ABILITY_EXTENDED_TOOLTIP_ID
            | MasterAttribute::ABILITY_HOTKEY_ACTION => {}
            _ => self.set_combat(attribute, v, world),
        }
    }

    /// [`Master::set_tribe_combat`]'s charge (`FUN_1412661d0`): its most, its recharge rate, its
    /// event, its type and its projectile.
    pub(super) fn set_charge(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::MAXIMUM_CHARGE => {
                if let Some(p) = self.creatable.as_mut() {
                    p.max_charge = Charge::new(v);
                }
            }
            MasterAttribute::RECHARGE_RATE => {
                if let Some(p) = self.creatable.as_mut() {
                    p.recharge_rate = ChargeRate::new(v);
                }
            }
            MasterAttribute::CHARGE_EVENT => {
                if let Some(p) = self.creatable.as_mut() {
                    p.charge_event = ChargeEvent::of_data(low16(v));
                }
            }
            MasterAttribute::CHARGE_TYPE => {
                if let Some(p) = self.creatable.as_mut() {
                    p.charge_kind = ChargeKind::of_data(low16(v));
                }
            }
            MasterAttribute::CHARGE_PROJECTILE_UNIT => {
                if let Some(p) = self.creatable.as_mut() {
                    p.charge_projectile = id_of(cvttss2si(v));
                }
            }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::SPECIAL_GRAPHIC => {
                if let Some(p) = self.creatable.as_mut() {
                    set_sprite(&mut p.special_sprite, cvttss2si(v), world);
                }
            }
            MasterAttribute::IDLE_ATTACK_GRAPHIC => {
                if let Some(p) = self.creatable.as_mut() {
                    set_sprite(&mut p.idle_attack_sprite, cvttss2si(v), world);
                }
            }
            MasterAttribute::HERO_GLOW_GRAPHIC => {
                if let Some(p) = self.creatable.as_mut() {
                    set_sprite(&mut p.hero_glow_sprite, cvttss2si(v), world);
                }
            }
            MasterAttribute::GARRISON_GRAPHIC => {
                if let Some(p) = self.creatable.as_mut() {
                    set_sprite(&mut p.garrison_sprite, cvttss2si(v), world);
                }
            }
            MasterAttribute::ATTACK_GRAPHIC_2 => {
                set_sprite(&mut self.attack_sprite2, cvttss2si(v), world);
            }
            _ => {}
        }
    }

    /// [`Master::set_tribe_combat`]'s train locations (`FUN_1412661d0`): the selected entry's
    /// location, button, hotkey and time, the selection (at most the last) and the count.
    pub(super) fn set_train_attribute(&mut self, attribute: MasterAttribute, v: f32) {
        use crate::math::conversions::cvttss2si;
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::TRAIN_LOCATION => {
                if let Some(t) = self.train_location_mut() {
                    t.location = MasterId::of_data(low16(v));
                }
            }
            MasterAttribute::TRAIN_BUTTON => {
                if let Some(t) = self.train_location_mut() {
                    t.button = LocationButton::of_data(low8(v));
                }
            }
            MasterAttribute::HOTKEY_ID => {
                if let Some(t) = self.train_location_mut() {
                    t.hotkey = HotkeyId::of_data(cvttss2si(v));
                }
            }
            MasterAttribute::TRAIN_TIME => {
                if let Some(t) = self.train_location_mut() {
                    t.time = WholeSeconds::of_data(low16(v));
                }
            }
            MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD => {
                if let Some(p) = self.creatable.as_mut()
                    && let Some(n) = select_entry(p.train_locations.len(), cvttss2si(v))
                {
                    p.train_location_entry = TrainLocationEntry::of_selected(n);
                }
            }
            MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                if let Some(p) = self.creatable.as_mut() {
                    resize_train_locations(&mut p.train_locations, &mut p.train_location_entry, v);
                }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::HERO_STATUS
            | MasterAttribute::SHOWN_PIERCE_ARMOR
            | MasterAttribute::MAXIMUM_CHARGE
            | MasterAttribute::RECHARGE_RATE
            | MasterAttribute::CHARGE_EVENT
            | MasterAttribute::SPECIAL_GRAPHIC
            | MasterAttribute::SPECIAL_ABILITY
            | MasterAttribute::IDLE_ATTACK_GRAPHIC
            | MasterAttribute::HERO_GLOW_GRAPHIC
            | MasterAttribute::GARRISON_GRAPHIC
            | MasterAttribute::TOTAL_MISSILES
            | MasterAttribute::MAXIMUM_TOTAL_MISSILES
            | MasterAttribute::REGENERATION_RATE
            | MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER
            | MasterAttribute::REGENERATION_HP_PERCENT
            | MasterAttribute::INVULNERABILITY_LEVEL
            | MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS
            | MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD
            | MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                let hit_points = self.hit_points;
                if let Some(p) = self.creatable.as_mut() {
                    p.add_value(attribute, v, hit_points, world);
                }
            }
            MasterAttribute::RESOURCE_COSTS
            | MasterAttribute::FOOD_COSTS
            | MasterAttribute::WOOD_COSTS
            | MasterAttribute::GOLD_COSTS
            | MasterAttribute::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = c.plus(low16(v));
                }
            }
            MasterAttribute::TRAIN_TIME => {
                if let Some(t) = self.train_location_mut() {
                    t.time = t.time.plus(low16(v));
                }
            }
            MasterAttribute::ATTACK_GRAPHIC_2 => add_sprite(&mut self.attack_sprite2, v, world),
            MasterAttribute::ABILITY_BUTTON_ICON_ID
            | MasterAttribute::ABILITY_SHORT_TOOLTIP_ID
            | MasterAttribute::ABILITY_EXTENDED_TOOLTIP_ID
            | MasterAttribute::ABILITY_HOTKEY_ACTION => {}
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::SHOWN_PIERCE_ARMOR
            | MasterAttribute::MAXIMUM_CHARGE
            | MasterAttribute::RECHARGE_RATE
            | MasterAttribute::TOTAL_MISSILES
            | MasterAttribute::MAXIMUM_TOTAL_MISSILES
            | MasterAttribute::REGENERATION_RATE
            | MasterAttribute::MIN_CONVERSION_TIME_MODIFIER
            | MasterAttribute::MAX_CONVERSION_TIME_MODIFIER
            | MasterAttribute::CONVERSION_CHANCE_MODIFIER
            | MasterAttribute::INVULNERABILITY_LEVEL
            | MasterAttribute::SPAWNING_AREA_WIDTH
            | MasterAttribute::SPAWNING_AREA_LENGTH
            | MasterAttribute::SPAWNING_AREA_RANDOMNESS
            | MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD
            | MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                if let Some(p) = self.creatable.as_mut() {
                    p.multiply_value(attribute, v);
                }
            }
            MasterAttribute::RESOURCE_COSTS
            | MasterAttribute::FOOD_COSTS
            | MasterAttribute::WOOD_COSTS
            | MasterAttribute::GOLD_COSTS
            | MasterAttribute::STONE_COSTS => {
                for c in self.costs_of(attribute) {
                    *c = CostAmount::of_data(low16(c.as_f32() * v + ROUND));
                }
            }
            MasterAttribute::TRAIN_TIME => {
                // FUN_140ace5e0 reads 0 past the list; FUN_14126ade0 writes only within it
                if let Some(t) = self.train_location_mut() {
                    t.time = WholeSeconds::of_data(low16(f32::from(t.time.get()) * v + ROUND));
                }
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::BUILDING_ICON_OVERRIDE => {
                if let Some(b) = self.building.as_mut() {
                    b.facet = low16(v);
                }
            }
            MasterAttribute::GARRISON_TYPE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_type = GarrisonKinds::from_bits_retain(low8(v));
                }
            }
            MasterAttribute::FOUNDATION_TERRAIN => {
                if let Some(b) = self.building.as_mut() {
                    b.foundation_terrain = TerrainId::of_data(low16(v));
                }
            }
            MasterAttribute::GARRISON_HEAL_RATE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_heal_rate = HitPoints::new(v);
                }
            }
            MasterAttribute::CONSTRUCTION_GRAPHIC
            | MasterAttribute::SNOW_GRAPHIC
            | MasterAttribute::DESTRUCTION_GRAPHIC
            | MasterAttribute::DESTRUCTION_RUBBLE_GRAPHIC
            | MasterAttribute::RESEARCHING_GRAPHIC
            | MasterAttribute::RESEARCH_COMPLETED_GRAPHIC
            | MasterAttribute::CONSTRUCTION_SOUND
            | MasterAttribute::CONSTRUCTION_SOUND_EVENT
            | MasterAttribute::TRANSFORM_SOUND
            | MasterAttribute::TRANSFORM_SOUND_EVENT => {}
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
        use crate::objects::masters::MasterAttribute;
        match attribute {
            MasterAttribute::BUILDING_ICON_OVERRIDE => {
                if let Some(b) = self.building.as_mut() {
                    b.facet = b.facet.wrapping_add(low16(v));
                }
            }
            MasterAttribute::GARRISON_TYPE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_type = GarrisonKinds::from_bits_retain(
                        b.garrison_type.bits().wrapping_add(low8(v)),
                    );
                }
            }
            MasterAttribute::FOUNDATION_TERRAIN => {
                if let Some(b) = self.building.as_mut() {
                    let was = b.foundation_terrain.map_or(-1, |t| i16::from(t.get()));
                    b.foundation_terrain = TerrainId::of_data(was.wrapping_add(low16(v)));
                }
            }
            MasterAttribute::GARRISON_HEAL_RATE => {
                if let Some(b) = self.building.as_mut() {
                    b.garrison_heal_rate += HitPoints::new(v);
                }
            }
            MasterAttribute::CONSTRUCTION_GRAPHIC
            | MasterAttribute::SNOW_GRAPHIC
            | MasterAttribute::DESTRUCTION_GRAPHIC
            | MasterAttribute::DESTRUCTION_RUBBLE_GRAPHIC
            | MasterAttribute::RESEARCHING_GRAPHIC
            | MasterAttribute::RESEARCH_COMPLETED_GRAPHIC
            | MasterAttribute::CONSTRUCTION_SOUND
            | MasterAttribute::TRANSFORM_SOUND => {}
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
            (MasterAttribute::GARRISON_HEAL_RATE, Some(b)) => {
                b.garrison_heal_rate *= v;
            }
            (MasterAttribute::GARRISON_HEAL_RATE, None) => {}
            _ => self.multiply_tribe_combat(attribute, v, world),
        }
    }
}
