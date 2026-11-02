//! The attributes an effect changes on a master and on its objects (the game's attribute list:
//! string 12200 plus the number names each).

use getset::CopyGetters;
use std::fmt::{Display, Error, Formatter};

/// An attribute an effect changes on a master (an attribute command's third argument, its low
/// byte: `FUN_140ec2ac0` reads `c & 0xff`). The simulation names the ones it tells apart (below);
/// any other is a number the data holds, which each master type's setter passes to its base's
/// and the last one ignores.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct MasterAttribute(
    /// Its number in the game's attribute list.
    #[getset(get_copy = "pub")]
    u8,
);

impl MasterAttribute {
    /// "Hit Points" (`+0x530`).
    pub const HIT_POINTS: Self = Self(0);

    /// "Line of Sight" (`+0x5d0`).
    pub const LINE_OF_SIGHT: Self = Self(1);

    /// "Garrison Capacity" (`+0x5d4`).
    pub const GARRISON_CAPACITY: Self = Self(2);

    /// "Unit Size X" (`+0x5d8`).
    pub const SIZE_X: Self = Self(3);

    /// "Unit Size Y" (`+0x5dc`).
    pub const SIZE_Y: Self = Self(4);

    /// "Movement Speed" (`+0x740`).
    pub const MOVEMENT_SPEED: Self = Self(5);

    /// "Rotation Speed" (`+0x758`).
    pub const ROTATION_SPEED: Self = Self(6);

    /// "Armor" (`+0x808`'s entries).
    pub const ARMOR: Self = Self(8);

    /// "Attack" (`+0x818`'s entries).
    pub const ATTACK: Self = Self(9);

    /// "Attack Reload Time" (`+0x838`).
    pub const ATTACK_RELOAD_TIME: Self = Self(10);

    /// "Accuracy Percent" (`+0x83e`).
    pub const ACCURACY_PERCENT: Self = Self(11);

    /// "Maximum Range" (`+0x828`).
    pub const MAXIMUM_RANGE: Self = Self(12);

    /// "Work Rate" (`+0x798`).
    pub const WORK_RATE: Self = Self(13);

    /// "Carry Capacity" (`+0x61c`).
    pub const CARRY_CAPACITY: Self = Self(14);

    /// "Base Armor" (`+0x800`).
    pub const BASE_ARMOR: Self = Self(15);

    /// "Projectile Unit" (`+0x83c`).
    pub const PROJECTILE_UNIT: Self = Self(16);

    /// "Building Icon Override" (`+0x982`).
    pub const BUILDING_ICON_OVERRIDE: Self = Self(17);

    /// "Terrain Defense Bonus" (`+0x820`).
    pub const TERRAIN_DEFENSE_BONUS: Self = Self(18);

    /// "Projectile Smart Mode" (`+0x881`).
    pub const PROJECTILE_SMART_MODE: Self = Self(19);

    /// "Minimum Range" (`+0x850`).
    pub const MINIMUM_RANGE: Self = Self(20);

    /// "Amount of 1st Resource Storage" (`+0x610`).
    pub const STORAGE_1: Self = Self(21);

    /// "Blast Width" (`+0x82c`).
    pub const BLAST_WIDTH: Self = Self(22);

    /// "Search Radius" (`+0x794`).
    pub const SEARCH_RADIUS: Self = Self(23);

    /// "Bonus Damage Resistance" (`+0x824`).
    pub const BONUS_DAMAGE_RESISTANCE: Self = Self(24);

    /// "Icon ID" (`+0x5ea`).
    pub const ICON_ID: Self = Self(25);

    /// "Amount of 2nd Resource Storage" (`+0x614`).
    pub const STORAGE_2: Self = Self(26);

    /// "Amount of 3rd Resource Storage" (`+0x618`).
    pub const STORAGE_3: Self = Self(27);

    /// "Fog Visibility" (`+0x605`).
    pub const FOG_VISIBILITY: Self = Self(28);

    /// "Occlusion Mode" (`+0x650`).
    pub const OCCLUSION_MODE: Self = Self(29);

    /// "Garrison Type" (`+0x9bc`).
    pub const GARRISON_TYPE: Self = Self(30);

    /// "Unit Size Z" (`+0x5e0`).
    pub const SIZE_Z: Self = Self(32);

    /// "Can Be Built On": the masters' setters write `+0x62b` with it, as for 45.
    pub const CAN_BE_BUILT_ON: Self = Self(33);

    /// "Foundation Terrain" (`+0x988`).
    pub const FOUNDATION_TERRAIN: Self = Self(34);

    /// "Hero Status" (`+0x8d9`).
    pub const HERO_STATUS: Self = Self(40);

    /// "Frame Delay" (`+0x842`).
    pub const FRAME_DELAY: Self = Self(41);

    /// "Train Location" (the selected train location's `+0x0`).
    pub const TRAIN_LOCATION: Self = Self(42);

    /// "Train Button" (its `+0x4`).
    pub const TRAIN_BUTTON: Self = Self(43);

    /// "Blast Attack Level" (`+0x830`).
    pub const BLAST_ATTACK_LEVEL: Self = Self(44);

    /// "Blast Defense Level" (`+0x62b`).
    pub const BLAST_DEFENSE_LEVEL: Self = Self(45);

    /// "Shown Attack" (`+0x856`).
    pub const SHOWN_ATTACK: Self = Self(46);

    /// "Shown Range" (`+0x858`).
    pub const SHOWN_RANGE: Self = Self(47);

    /// "Shown Melee Armor" (`+0x854`).
    pub const SHOWN_MELEE_ARMOR: Self = Self(48);

    /// "Shown Pierce Armor" (`+0x8b0`).
    pub const SHOWN_PIERCE_ARMOR: Self = Self(49);

    /// "Object Name ID" (`+0xc`).
    pub const OBJECT_NAME_ID: Self = Self(50);

    /// "Short Description ID" (`+0x10`).
    pub const SHORT_DESCRIPTION_ID: Self = Self(51);

    /// "Terrain Restriction ID" (`+0x606`).
    pub const TERRAIN_RESTRICTION_ID: Self = Self(53);

    /// "Unit Trait" (`+0x654`).
    pub const UNIT_TRAIT: Self = Self(54);

    /// "Trait Piece" (`+0x654`'s high half).
    pub const TRAIT_PIECE: Self = Self(56);

    /// "Dead Unit ID" (`+0x5e4`).
    pub const DEAD_UNIT_ID: Self = Self(57);

    /// "Hotkey ID" (the selected train location's `+0x8`).
    pub const HOTKEY_ID: Self = Self(58);

    /// "Maximum Charge" (`+0x918`).
    pub const MAXIMUM_CHARGE: Self = Self(59);

    /// "Recharge Rate" (`+0x91c`).
    pub const RECHARGE_RATE: Self = Self(60);

    /// "Charge Event" (`+0x920`).
    pub const CHARGE_EVENT: Self = Self(61);

    /// "Charge Type" (`+0x922`).
    pub const CHARGE_TYPE: Self = Self(62);

    /// "Combat Ability" (`+0x840`).
    pub const COMBAT_ABILITY: Self = Self(63);

    /// "Attack Dispersion" (`+0x860`).
    pub const ATTACK_DISPERSION: Self = Self(64);

    /// "Secondary Projectile Unit" (`+0x8d4`).
    pub const SECONDARY_PROJECTILE_UNIT: Self = Self(65);

    /// "Blood Unit" (`+0x5e6`).
    pub const BLOOD_UNIT: Self = Self(66);

    /// "Projectile Hit Mode" (`+0x882`).
    pub const PROJECTILE_HIT_MODE: Self = Self(67);

    /// "Projectile Vanish Mode" (`+0x883`).
    pub const PROJECTILE_VANISH_MODE: Self = Self(68);

    /// "Projectile Arc" (`+0x888`).
    pub const PROJECTILE_ARC: Self = Self(69);

    /// "Attack Graphic" (`+0x7f8`).
    pub const ATTACK_GRAPHIC: Self = Self(70);

    /// "Standing Graphic" (`+0x470`).
    pub const STANDING_GRAPHIC: Self = Self(71);

    /// "Standing Graphic 2" (`+0x478`).
    pub const STANDING_GRAPHIC_2: Self = Self(72);

    /// "Dying Graphic" (`+0x480`).
    pub const DYING_GRAPHIC: Self = Self(73);

    /// "Undead Graphic" (`+0x488`).
    pub const UNDEAD_GRAPHIC: Self = Self(74);

    /// "Walking Graphic" (`+0x748`).
    pub const WALKING_GRAPHIC: Self = Self(75);

    /// "Running Graphic" (`+0x750`).
    pub const RUNNING_GRAPHIC: Self = Self(76);

    /// "Special Graphic" (`+0x8e8`).
    pub const SPECIAL_GRAPHIC: Self = Self(77);

    /// "Obstruction Type" (`+0x651`).
    pub const OBSTRUCTION_TYPE: Self = Self(78);

    /// "Blockage Class" (`+0x652`).
    pub const BLOCKAGE_CLASS: Self = Self(79);

    /// "Selection Effect" (`+0x684`).
    pub const SELECTION_EFFECT: Self = Self(80);

    /// "Special Ability" (`+0x8d8`).
    pub const SPECIAL_ABILITY: Self = Self(81);

    /// "Idle Attack Graphic" (`+0x910`).
    pub const IDLE_ATTACK_GRAPHIC: Self = Self(82);

    /// "Hero Glow Graphic" (`+0x908`).
    pub const HERO_GLOW_GRAPHIC: Self = Self(83);

    /// "Garrison Graphic" (`+0x8f0`).
    pub const GARRISON_GRAPHIC: Self = Self(84);

    /// "Construction Graphic", the first of a building's six sprites (85 to 90, `+0x950` to
    /// `+0x978`).
    pub const CONSTRUCTION_GRAPHIC: Self = Self(85);

    /// "Snow Graphic".
    pub const SNOW_GRAPHIC: Self = Self(86);

    /// "Destruction Graphic".
    pub const DESTRUCTION_GRAPHIC: Self = Self(87);

    /// "Destruction Rubble Graphic".
    pub const DESTRUCTION_RUBBLE_GRAPHIC: Self = Self(88);

    /// "Researching Graphic".
    pub const RESEARCHING_GRAPHIC: Self = Self(89);

    /// "Research Completed Graphic", the last of them.
    pub const RESEARCH_COMPLETED_GRAPHIC: Self = Self(90);

    /// "Damage Graphic" (the selected damage sprite's sprite).
    pub const DAMAGE_GRAPHIC: Self = Self(91);

    /// "Selection Sound", the first of the sounds 92 to 99.
    pub const SELECTION_SOUND: Self = Self(92);

    /// "Damage Sound Event", the last of them.
    pub const DAMAGE_SOUND_EVENT: Self = Self(99);

    /// "Resource Costs" (each paid cost entry, `+0x880`..`+0x890`).
    pub const RESOURCE_COSTS: Self = Self(100);

    /// "Train Time" (the selected train location's `+0x2`).
    pub const TRAIN_TIME: Self = Self(101);

    /// "Total Missiles" (`+0x8c0`).
    pub const TOTAL_MISSILES: Self = Self(102);

    /// "Food Costs" (the cost entries of resource 0).
    pub const FOOD_COSTS: Self = Self(103);

    /// "Wood Costs" (resource 1).
    pub const WOOD_COSTS: Self = Self(104);

    /// "Gold Costs" (resource 3).
    pub const GOLD_COSTS: Self = Self(105);

    /// "Stone Costs" (resource 2).
    pub const STONE_COSTS: Self = Self(106);

    /// "Maximum Total Missiles" (`+0x8c4`).
    pub const MAXIMUM_TOTAL_MISSILES: Self = Self(107);

    /// "Garrison Heal Rate" (`+0x9c0`).
    pub const GARRISON_HEAL_RATE: Self = Self(108);

    /// "Regeneration Rate" (`+0x8b4`).
    pub const REGENERATION_RATE: Self = Self(109);

    /// "Population" (the unpaid cost entries and the storage of resources 4, 11 and 19).
    pub const POPULATION: Self = Self(110);

    /// "Minimum Conversion Time Modifier" (`+0x934`).
    pub const MIN_CONVERSION_TIME_MODIFIER: Self = Self(111);

    /// "Maximum Conversion Time Modifier" (`+0x938`).
    pub const MAX_CONVERSION_TIME_MODIFIER: Self = Self(112);

    /// "Conversion Chance Modifier" (`+0x93c`).
    pub const CONVERSION_CHANCE_MODIFIER: Self = Self(113);

    /// "Formation Category" (`+0x8bc`).
    pub const FORMATION_CATEGORY: Self = Self(114);

    /// "Area Damage" (`+0x834`).
    pub const AREA_DAMAGE: Self = Self(115);

    /// "Damage Reflection" (`+0x864`).
    pub const DAMAGE_REFLECTION: Self = Self(118);

    /// "Friendly Fire Damage" (`+0x868`).
    pub const FRIENDLY_FIRE_DAMAGE: Self = Self(119);

    /// "Regeneration HP Percent" (`+0x8b4` from the hit points).
    pub const REGENERATION_HP_PERCENT: Self = Self(120);

    /// "Ability Button Icon ID" (`+0x8da`), the first of the ability's interface values (121
    /// to 124).
    pub const ABILITY_BUTTON_ICON_ID: Self = Self(121);

    /// "Ability Short Tooltip ID".
    pub const ABILITY_SHORT_TOOLTIP_ID: Self = Self(122);

    /// "Ability Extended Tooltip ID".
    pub const ABILITY_EXTENDED_TOOLTIP_ID: Self = Self(123);

    /// "Ability Hotkey Action" (`+0x8e4`), the last of them.
    pub const ABILITY_HOTKEY_ACTION: Self = Self(124);

    /// "Charge Projectile Unit" (`+0x928`).
    pub const CHARGE_PROJECTILE_UNIT: Self = Self(125);

    /// "Available Unit Flag" (`+0x5f0`).
    pub const AVAILABLE_UNIT_FLAG: Self = Self(126);

    /// "Disabled Unit Flag" (`+0x5f1`).
    pub const DISABLED_UNIT_FLAG: Self = Self(127);

    /// "Attack Priority" (`+0x92c`).
    pub const ATTACK_PRIORITY: Self = Self(128);

    /// "Invulnerability Level" (`+0x930`).
    pub const INVULNERABILITY_LEVEL: Self = Self(129);

    /// "Garrison Firepower" (`+0x870`).
    pub const GARRISON_FIREPOWER: Self = Self(130);

    /// "Attack Graphic 2" (`+0x878`).
    pub const ATTACK_GRAPHIC_2: Self = Self(131);

    /// "Command Sound", the first of the sounds 132 to 139.
    pub const COMMAND_SOUND: Self = Self(132);

    /// "Command Sound Event".
    pub const COMMAND_SOUND_EVENT: Self = Self(133);

    /// "Move Sound".
    pub const MOVE_SOUND: Self = Self(134);

    /// "Move Sound Event".
    pub const MOVE_SOUND_EVENT: Self = Self(135);

    /// "Construction Sound".
    pub const CONSTRUCTION_SOUND: Self = Self(136);

    /// "Construction Sound Event".
    pub const CONSTRUCTION_SOUND_EVENT: Self = Self(137);

    /// "Transform Sound".
    pub const TRANSFORM_SOUND: Self = Self(138);

    /// "Transform Sound Event", the last of them.
    pub const TRANSFORM_SOUND_EVENT: Self = Self(139);

    /// "Run Pattern" (`+0x7b9`).
    pub const RUN_PATTERN: Self = Self(140);

    /// "Interface Kind" (`+0x62f`).
    pub const INTERFACE_KIND: Self = Self(141);

    /// "Combat Level" (`+0x62c`).
    pub const COMBAT_LEVEL: Self = Self(142);

    /// "Interaction Mode" (`+0x62d`).
    pub const INTERACTION_MODE: Self = Self(143);

    /// "Minimap Mode" (`+0x62e`).
    pub const MINIMAP_MODE: Self = Self(144);

    /// "Trailing Unit" (`+0x75e`).
    pub const TRAILING_UNIT: Self = Self(145);

    /// "Trail Mode" (`+0x760`).
    pub const TRAIL_MODE: Self = Self(146);

    /// "Trail Density" (`+0x764`).
    pub const TRAIL_DENSITY: Self = Self(147);

    /// "Projectile Graphic Displacement X" (`+0x844`; Y and Z follow).
    pub const DISPLACEMENT_X: Self = Self(148);

    /// "Projectile Graphic Displacement Y" (`+0x848`).
    pub const DISPLACEMENT_Y: Self = Self(149);

    /// "Projectile Graphic Displacement Z" (`+0x84c`).
    pub const DISPLACEMENT_Z: Self = Self(150);

    /// "Projectile Spawning Area Width" (`+0x8c8`; the length and the randomness follow).
    pub const SPAWNING_AREA_WIDTH: Self = Self(151);

    /// "Projectile Spawning Area Length" (`+0x8cc`).
    pub const SPAWNING_AREA_LENGTH: Self = Self(152);

    /// "Projectile Spawning Area Randomness" (`+0x8d0`).
    pub const SPAWNING_AREA_RANDOMNESS: Self = Self(153);

    /// "Damage Graphics Entry Mod" (`+0x731`).
    pub const DAMAGE_GRAPHICS_ENTRY_MOD: Self = Self(154);

    /// "Damage Graphics Total Num" (`+0x638`'s count).
    pub const DAMAGE_GRAPHICS_TOTAL_NUM: Self = Self(155);

    /// "Damage Graphic Percent" (the selected damage sprite's `+0x8`).
    pub const DAMAGE_GRAPHIC_PERCENT: Self = Self(156);

    /// "Damage Graphic Apply Mode" (its `+0x9`).
    pub const DAMAGE_GRAPHIC_APPLY_MODE: Self = Self(157);

    /// "Train Locations Entry Mod" (`+0x940`).
    pub const TRAIN_LOCATIONS_ENTRY_MOD: Self = Self(158);

    /// "Train Locations Total Num" (`+0x898`'s count).
    pub const TRAIN_LOCATIONS_TOTAL_NUM: Self = Self(159);

    /// The attribute of an effect command's third argument: its low byte, as the game reads it.
    #[must_use]
    pub const fn of_effect(argument: i16) -> Self {
        Self(argument.to_le_bytes()[0])
    }

    /// The attribute of number `value`, for the tests and the data's numbers.
    #[must_use]
    pub const fn numbered(value: u8) -> Self {
        Self(value)
    }

    /// A sprite attribute whose change shows on the objects after it (`FUN_140fde290`: 70 to 77,
    /// 82 to 91 and 131).
    #[must_use]
    pub const fn shows_sprite(self) -> bool {
        matches!(self.0, 70..=77 | 82..=91 | 131)
    }
}

impl Display for MasterAttribute {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use crate::objects::masters::MasterAttribute;

    /// An attribute command's third argument names the attribute by its low byte.
    #[test]
    fn an_effect_argument_names_the_attribute_by_its_low_byte() {
        assert_eq!(MasterAttribute::of_effect(9), MasterAttribute::ATTACK);
        assert_eq!(MasterAttribute::of_effect(0x109), MasterAttribute::ATTACK);
        assert_eq!(MasterAttribute::of_effect(-1).get(), 0xff);
    }

    /// The sprite attributes 70 to 77, 82 to 91 and 131 show on the objects, their neighbours
    /// do not.
    #[test]
    fn the_sprite_attributes_show_on_the_objects() {
        for n in (70..=77).chain(82..=91).chain([131]) {
            assert!(MasterAttribute::numbered(n).shows_sprite(), "{n}");
        }
        for n in [69, 78, 81, 92, 130, 132] {
            assert!(!MasterAttribute::numbered(n).shows_sprite(), "{n}");
        }
    }
}
