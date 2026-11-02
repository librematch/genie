//! The indices of the player's attributes (`player+0x8`'s array of `f32`): the one definition of
//! the attributes the engine reads and writes by number. The first four are the resources
//! (food, wood, stone, gold). A master's attribute types are a different set
//! ([`crate::objects::masters::attribute`]).

use crate::GenieError;
use getset::CopyGetters;
use std::fmt::{Display, Error as FmtError, Formatter};

/// A player's attribute number (`player+0x100`'s array, the `.dat`'s attribute types, an effect's
/// attribute argument): 0 or more. The first four are the resources (food, wood, stone, gold).
/// The simulation names the ones it tells apart (below); any other is a number the data holds
/// ([`AttributeId::numbered`]). A field that holds −1 for none is `Option<AttributeId>`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, CopyGetters)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AttributeId(
    /// Its value, the attribute's place in the player's attributes.
    #[getset(get_copy = "pub")]
    i16,
);

impl AttributeId {
    /// An attribute the code names, in a constant: only this module makes them.
    const fn named(value: i16) -> Self {
        assert!(0 <= value, "a negative attribute number");
        Self(value)
    }

    /// An attribute the code names by its number, where the game's own number is the name (the
    /// attributes the engine reads once, and the tests').
    #[must_use]
    pub const fn numbered(value: i16) -> Self {
        Self::named(value)
    }

    /// Its place in a table by attribute number (the player's attributes).
    #[must_use]
    pub const fn index(self) -> usize {
        self.0.unsigned_abs() as usize
    }

    /// The attribute `by` places on: a table of attributes that run by player or by kind from this
    /// one (the trickle rates, the gathered totals).
    ///
    /// # Errors
    ///
    /// A number past the `i16` range.
    pub fn offset(self, by: usize) -> Result<Self, GenieError> {
        i16::try_from(by)
            .ok()
            .and_then(|by| self.0.checked_add(by))
            .map(Self)
            .ok_or_else(|| {
                GenieError::invalid_input("an attribute number past the table")
                    .with("attribute", self.0)
                    .with("offset", by)
            })
    }

    /// The attribute of a table place (a number of `usize`, as a command or a loop holds it).
    #[must_use]
    pub fn of_index(index: usize) -> Option<Self> {
        i16::try_from(index).ok().and_then(Self::of_data)
    }

    /// Whether it is one of the four resources (food, wood, stone, gold).
    #[must_use]
    pub const fn is_resource(self) -> bool {
        self.0 <= 3
    }

    /// The attribute a data field names, none for a negative value (−1 in the `.dat`).
    #[must_use]
    pub fn of_data(value: i16) -> Option<Self> {
        (0 <= value).then_some(Self(value))
    }

    /// The attribute of a raw field that holds −1 for none (a unit's load `+0x308`, a task's
    /// resources).
    ///
    /// # Errors
    ///
    /// A negative value other than −1, which no attribute is.
    pub fn of_field(value: i16) -> Result<Option<Self>, GenieError> {
        (value != -1).then(|| Self::try_from(value)).transpose()
    }

    /// A food load: food (0) or one of the foods a unit carries (15 to 17); a gatherer keeps a
    /// load of one for another.
    #[must_use]
    pub const fn is_food_load(self) -> bool {
        self.0 == 0 || (15 <= self.0 && self.0 <= 17)
    }

    /// The player's resource of a load (`FUN_140fdd4f0`'s first part): the kinds 0 to 3, 9 and
    /// 56 themselves, the foods 15 to 17 food, any other none.
    #[must_use]
    pub const fn load_resource(self) -> Option<Self> {
        match self.0 {
            0..=3 | 9 | 0x38 => Some(self),
            15..=17 => Some(FOOD),
            _ => None,
        }
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(attribute: Option<Self>) -> i16 {
        match attribute {
            Some(a) => a.0,
            None => -1,
        }
    }
}

impl Display for AttributeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        self.0.fmt(f)
    }
}

impl genie_diff::Diff for AttributeId {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

impl TryFrom<i16> for AttributeId {
    type Error = GenieError;

    /// The attribute of value `value`.
    ///
    /// # Errors
    ///
    /// A negative value, which no attribute is.
    fn try_from(value: i16) -> Result<Self, GenieError> {
        Self::of_data(value)
            .ok_or_else(|| GenieError::unsupported("a negative attribute").with("attribute", value))
    }
}

/// Food: the first of the four resources (food, wood, stone, gold: 0 to 3).
pub const FOOD: AttributeId = AttributeId::named(0);
/// Wood.
pub const WOOD: AttributeId = AttributeId::named(1);
/// Stone.
pub const STONE: AttributeId = AttributeId::named(2);
/// Gold: a sale gives it and a purchase costs it, the treason costs it.
pub const GOLD: AttributeId = AttributeId::named(3);
/// The four resources in their order: food, wood, stone, gold.
pub const RESOURCES: [AttributeId; 4] = [FOOD, WOOD, STONE, GOLD];
/// The current age (`+0x30`, `player+0x100`), as the `i16` game mode 16's QUEUE, a new soldier's
/// unit AI (`FUN_140c12300`) and a command compare; the mode's bound is `DAT_14330eb8c`.
pub const CURRENT_AGE: AttributeId = AttributeId::named(6);
/// The population headroom (`+0x308` 4): a building's held amount gives it, and a cost check
/// skips it (`FUN_141618db0` with its fifth argument 1).
pub const HEADROOM: AttributeId = AttributeId::named(4);
/// The population.
pub const POPULATION: AttributeId = AttributeId::named(11);
/// The units owned.
pub const UNITS_OWNED: AttributeId = AttributeId::named(19);
/// The kills.
pub const KILLS: AttributeId = AttributeId::named(20);
/// The explored share of the map, in percent.
pub const EXPLORED: AttributeId = AttributeId::named(22);
/// The civilian population.
pub const CIVILIAN_POPULATION: AttributeId = AttributeId::named(37);
/// The military population.
pub const MILITARY_POPULATION: AttributeId = AttributeId::named(40);
/// The razings.
pub const RAZINGS: AttributeId = AttributeId::named(43);
/// The kill ratio: kills less losses.
pub const KILL_RATIO: AttributeId = AttributeId::named(44);
/// The survival to the finish, which a defeat clears.
pub const SURVIVAL: AttributeId = AttributeId::named(45);
/// The town centre is not available (0 lets a builder place one, `FUN_141625a20`).
pub const NO_TOWN_CENTRE: AttributeId = AttributeId::named(48);
/// The gold counter, which gold with a statistic grows (`FUN_141616820`).
pub const GOLD_COUNTER: AttributeId = AttributeId::named(49);
/// The research cost modifier, as the `i16` the cost code compares.
pub const RESEARCH_COST_MODIFIER: AttributeId = AttributeId::named(85);
/// The kills' value of the other players.
pub const KILLS_VALUE_OTHER: AttributeId = AttributeId::named(152);
/// The razings' value of the other players.
pub const RAZINGS_VALUE_OTHER: AttributeId = AttributeId::named(153);
/// The kills of the other players.
pub const KILLS_OTHER: AttributeId = AttributeId::named(154);
/// The razings of the other players.
pub const RAZINGS_OTHER: AttributeId = AttributeId::named(155);
/// The units' value.
pub const UNITS_VALUE: AttributeId = AttributeId::named(164);
/// The buildings' value.
pub const BUILDINGS_VALUE: AttributeId = AttributeId::named(165);
/// The first of the gathered totals of food, wood and stone (166 to 168).
pub const GATHERED: AttributeId = AttributeId::named(166);
/// The gathered total of gold.
pub const GOLD_GATHERED: AttributeId = AttributeId::named(169);
/// The kills' value.
pub const KILLS_VALUE: AttributeId = AttributeId::named(170);
/// The razings' value.
pub const RAZINGS_VALUE: AttributeId = AttributeId::named(172);
/// The town centres a player may have (`FUN_141625a20`).
pub const TOWN_CENTRE_LIMIT: AttributeId = AttributeId::named(218);
/// The first of the trickles a minute of food, wood, stone and gold (230 to 233).
pub const TRICKLES: AttributeId = AttributeId::named(230);
/// The relics the player holds (7): a research paid in them (`FUN_1413d0e70`).
pub const RELICS_HELD: AttributeId = AttributeId::named(7);
/// The units queued (80).
pub const QUEUED: AttributeId = AttributeId::named(80);
/// The units in production (81).
pub const IN_PRODUCTION: AttributeId = AttributeId::named(81);
/// The population cap (32).
pub const POPULATION_CAP: AttributeId = AttributeId::named(32);
/// The food the farms hold (Farm Food Amount, `player+0x100` at `+0x120`: 36), which
/// `set_state(6)` of a farm's Make reads and an effect's change grows (`FUN_141615270`).
pub const FARM_FOOD_AMOUNT: AttributeId = AttributeId::named(36);
/// The fish traps' food (Fish Trap Food Amount, `player+0x100` at `+0x2c0`: 88), which
/// `set_state(6)` of any other Make reads.
pub const FISH_TRAP_FOOD: AttributeId = AttributeId::named(88);
/// The technologies researched (0x15).
pub const TECHS_RESEARCHED: AttributeId = AttributeId::named(0x15);
/// The resources spent on research (0x63).
pub const RESEARCH_SPENT: AttributeId = AttributeId::named(0x63);
/// The attribute that scales research times above 0 (`+0x2b0`: 86).
pub const RESEARCH_TIME_MODIFIER: AttributeId = AttributeId::named(86);
/// The attribute naming the world's effect a player gets after each research (`+0x868`: 269).
pub const WORLD_EFFECT: AttributeId = AttributeId::named(269);
/// The buildings' paid costs (247), which the combat setup adds for a building with a train
/// location (`FUN_14121df60`).
pub const BUILDING_COST: AttributeId = AttributeId::named(247);
/// The paid cost of the player's units and buildings together (98): a unit's or a building's
/// completion adds it, as to [`UNITS_VALUE`] or [`BUILDINGS_VALUE`], its death takes it back.
pub const TOTAL_VALUE: AttributeId = AttributeId::named(98);
/// The monasteries the player holds (52): their completion adds 1, their death takes it.
pub const MONASTERIES: AttributeId = AttributeId::named(52);
/// The wonders the player holds (42).
pub const WONDERS: AttributeId = AttributeId::named(42);
/// The castles the player holds (134).
pub const CASTLES: AttributeId = AttributeId::named(134);
/// The castles the player built (173: their completion adds 1, their death takes nothing).
pub const CASTLES_BUILT: AttributeId = AttributeId::named(173);
/// The wonders the player built (174).
pub const WONDERS_BUILT: AttributeId = AttributeId::named(174);
/// The construction rate modifier (195), which a builder's work reads (at the player's attributes
/// `+0x618`).
pub const CONSTRUCTION_RATE: AttributeId = AttributeId::named(195);
/// Where a castle's or a wonder's paid cost goes rather than to 98 and 165 (184,
/// `FUN_1411dbb70`).
pub const CASTLE_WONDER_COST: AttributeId = AttributeId::named(184);
/// The giver's tribute given (53).
pub const TRIBUTE_GIVEN: AttributeId = AttributeId::named(53);
/// The giver's tribute paid, with the fee (175).
pub const TRIBUTE_PAID: AttributeId = AttributeId::named(175);
/// The giver's tribute to each player (450 plus the receiver's number).
pub const TRIBUTE_TO: AttributeId = AttributeId::named(450);
/// The receiver's tribute received (171).
pub const TRIBUTE_RECEIVED: AttributeId = AttributeId::named(171);
/// The receiver's tribute from each player (475 plus the giver's number).
pub const TRIBUTE_FROM: AttributeId = AttributeId::named(475);
/// The kills of each player's units (300 plus the victim's player).
pub const KILLS_OF: AttributeId = AttributeId::named(300);
/// The kills by each player (325 plus the killer's player), on the victim's owner.
pub const KILLED_BY: AttributeId = AttributeId::named(325);
/// The value of the kills of each player's units (400 plus the victim's player).
pub const KILLS_VALUE_OF: AttributeId = AttributeId::named(400);
/// The razings of each player's buildings (350 plus the owner's player).
pub const RAZINGS_OF: AttributeId = AttributeId::named(350);
/// The razings by each player (375 plus the razer's player), on the owner.
pub const RAZED_BY: AttributeId = AttributeId::named(375);
/// The value of the razings of each player's buildings (425 plus the owner's player).
pub const RAZINGS_VALUE_OF: AttributeId = AttributeId::named(425);
/// The owner's attribute of a pasture's food, which its livestock share.
pub const PASTURE_FOOD: AttributeId = AttributeId::named(288);
/// The owner's attribute of the livestock a pasture holds.
pub const PASTURE_LIVESTOCK: AttributeId = AttributeId::named(289);
/// The owner's attribute of the workers a pasture's livestock take.
pub const PASTURE_WORKERS: AttributeId = AttributeId::named(290);
/// A relic, as a monk carries it (`+0x308` 0x39): the monk tasks for carried relics; a unit
/// carrying one never attacks.
pub const RELIC: AttributeId = AttributeId::named(57);
/// The ore (0x38): a commodity the market's price reads know (none of its commands trades it),
/// and a load a drop-off takes as itself beside the four resources and the trade goods
/// (`FUN_140fdd4f0`).
pub const ORE: AttributeId = AttributeId::named(0x38);
/// The share of a trade's worth the market keeps (78).
pub const MARKET_FEE: AttributeId = AttributeId::named(78);
/// A load of meat: a hunter's, which a drop-off orders the hunt for again.
pub const MEAT: AttributeId = AttributeId::named(15);
/// A load of a farm's food: a farmer's, and what a Make of a farm holds.
pub const FARM_FOOD: AttributeId = AttributeId::named(16);
/// The food a minute of the livestock in the player's mills.
pub const MILL_FOOD: AttributeId = AttributeId::named(254);

/// A villager's death by a hunted animal (a prey or predator attacker).
pub const VILLAGER_KILLED_BY_ANIMAL: AttributeId = AttributeId::named(227);
/// A villager's death by a player of type 1.
pub const VILLAGER_KILLED_BY_TYPE_1: AttributeId = AttributeId::named(229);
/// A villager's death by a player of type 2.
pub const VILLAGER_KILLED_BY_TYPE_2: AttributeId = AttributeId::named(226);
/// A villager's death by a player of type 3.
pub const VILLAGER_KILLED_BY_TYPE_3: AttributeId = AttributeId::named(228);

#[cfg(test)]
mod tests {
    use super::{AttributeId, FARM_FOOD, FOOD, GOLD, MEAT, POPULATION, WOOD};
    use crate::ErrorKind;

    /// The four resources come first, a negative value is no attribute.
    #[test]
    fn the_resources_are_the_first_four_and_negative_value_is_none() {
        assert!(FOOD.is_resource() && GOLD.is_resource());
        assert!(!POPULATION.is_resource());
        assert_eq!(AttributeId::of_data(-1), None);
        assert_eq!(AttributeId::of_data(11), Some(POPULATION));
        assert_eq!(AttributeId::field(Some(GOLD)), 3);
        assert_eq!(AttributeId::field(None), -1);
        assert!(AttributeId::try_from(-2).is_err());
    }

    /// A field's −1 is none, a value of 0 or more an attribute, another negative one an error.
    #[test]
    fn field_minus_one_is_none_and_another_negative_value_an_error() {
        assert_eq!(AttributeId::of_field(-1), Ok(None));
        assert_eq!(AttributeId::of_field(16), Ok(Some(FARM_FOOD)));
        assert!(AttributeId::of_field(-2).is_err_and(|e| e.kind() != ErrorKind::Io));
    }

    /// The food loads are food and the kinds 15 to 17; a load's resource is itself for 0 to 3,
    /// 9 and 56, food for 15 to 17, none for any other.
    #[test]
    fn load_resource_is_itself_food_or_none() {
        for n in [0, 15, 16, 17] {
            assert!(AttributeId::numbered(n).is_food_load(), "{n}");
        }
        for n in [1, 2, 3, 14, 18] {
            assert!(!AttributeId::numbered(n).is_food_load(), "{n}");
        }
        assert_eq!(WOOD.load_resource(), Some(WOOD));
        assert_eq!(
            AttributeId::numbered(56).load_resource().map(|a| a.get()),
            Some(56)
        );
        assert_eq!(MEAT.load_resource(), Some(FOOD));
        assert_eq!(AttributeId::numbered(17).load_resource(), Some(FOOD));
        assert_eq!(AttributeId::numbered(4).load_resource(), None);
    }

    #[test]
    fn an_offset_past_the_range_is_an_error_not_panic() {
        assert_eq!(FOOD.offset(3).unwrap(), GOLD);
        assert!(AttributeId::numbered(i16::MAX).offset(1).is_err());
        assert!(FOOD.offset(usize::MAX).is_err());
    }
}
