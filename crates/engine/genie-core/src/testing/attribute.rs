//! The player attributes genie-core's tests name: build 158041's numbers, as the tests' players
//! hold them, by the names the engine's constants had (FD-046-D48: the engine reads them from
//! the build layer's slots and roles).

use crate::players::attribute::AttributeId;

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
/// The faith a priest holds (34): the combat setup's attribute kind at 100.
pub const FAITH: AttributeId = AttributeId::named(34);
/// The salvage decay rate (65): the combat setup's decay timer takes it times 1000.
pub const SALVAGE_DECAY_RATE: AttributeId = AttributeId::named(65);
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
/// The civilisation name override (262, `cAttributeCivNameOverride`): its store names the
/// player's civilisation by the string of its value.
pub const CIV_NAME_OVERRIDE: AttributeId = AttributeId::named(262);
/// The attribute naming the world's effect a player gets after each research (`+0x868`: 269).
pub const WORLD_EFFECT: AttributeId = AttributeId::named(269);
/// The buildings' paid costs (247), which the combat setup adds for a building with a train
/// location (`FUN_14121df60`).
pub const BUILDING_COST: AttributeId = AttributeId::named(247);
/// The paid cost of the player's units and buildings together (98): a unit's or a building's
/// completion adds it, as to `UNITS_VALUE` or `BUILDINGS_VALUE`, its death takes it back.
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
pub const VILLAGER_KILLED_BY_KIND_1: AttributeId = AttributeId::named(229);
/// A villager's death by a player of type 2.
pub const VILLAGER_KILLED_BY_KIND_2: AttributeId = AttributeId::named(226);
/// A villager's death by a player of type 3.
pub const VILLAGER_KILLED_BY_KIND_3: AttributeId = AttributeId::named(228);
/// A task whose `attribute_type4` is this (`0x124`) stops its target's decay under the attribute
/// (`FUN_140ac44b0`).
pub const DECAY_TASK: AttributeId = AttributeId::named(292);
/// The attribute that halves the treason's gold: above 0 the treason costs 200, not 400 (read
/// at `0x1416204d5`).
pub const TREASON_DISCOUNT: AttributeId = AttributeId::named(197);
