//! The master ids (`i16`, [`crate::objects::masters::Master::id`]): the one definition of the
//! masters the engine names, by the `.dat`'s unit id. The obfuscated build stores an id as its
//! value plus `0x609c`; the values here are the decoded ones. A master the engine tests by id
//! without a known name is `ID_n` or `MASTER_n`.

use crate::objects::masters::MasterId;

/// The dock (`.dat` DOCK), whose footprint a reach test gives more range (raw `0x60c9`,
/// `FUN_140fa0370`).
pub const ID_45: MasterId = MasterId::named(45);
/// The farm (`0x32`): a mill's queue buys it, and BUILD places it as the rice farm on mangrove or
/// rice farm terrain.
pub const FARM: MasterId = MasterId::named(50);
/// The mill (`0x44`), whose livestock makes food (the tracker's list `FUN_14104f960`).
pub const MILL: MasterId = MasterId::named(68);
/// The house (`0x46`), whose update takes the walls' branch.
pub const HOUSE: MasterId = MasterId::named(70);
/// The castle (`0x52`), whose death lowers its owner's attribute 134 (`FUN_1411dbb70`).
pub const CASTLE: MasterId = MasterId::named(82);
/// The male villager (`0x53`): the master QUEUE draws a sex for, and the one the generic
/// villager (`0x712`) becomes first (`FUN_1411cbc40`).
pub const VILLAGER_MALE: MasterId = MasterId::named(83);
/// The market (raw `0x60f0`), whose own trade units the work command never retakes and whose
/// sight the AI players' market lists follow.
pub const MARKET: MasterId = MasterId::named(84);
/// The monastery (raw `0x6104`), whose relics give goods every second and whose death lowers its
/// owner's attribute 52 (`FUN_1411dbb70`).
pub const MONASTERY: MasterId = MasterId::named(104);
/// The town centre (raw `0x6109`).
pub const TOWN_CENTRE: MasterId = MasterId::named(109);
/// The builder (`0x76`) the train locations name for every villager.
pub const BUILDER: MasterId = MasterId::named(118);
/// The fish trap (raw `0x6163`), which only the fishing ships gather from.
pub const FISH_TRAP: MasterId = MasterId::named(199);
/// The Wonder (`0x114`, raw `0x61b0`), whose death lowers its owner's attribute 42
/// (`FUN_1411dbb70`) and whose construction rate the player's attribute raises at most to
/// `DAT_142ed66b4`.
pub const WONDER: MasterId = MasterId::named(276);
/// The relic (`0x11d`).
pub const RELIC: MasterId = MasterId::named(285);
/// The female villager (`0x125`).
pub const VILLAGER_FEMALE: MasterId = MasterId::named(293);
/// The king (`0x1b2`).
pub const KING: MasterId = MasterId::named(434);
/// A town centre (`0x1bc`), whose tracker list the town centre counts read after 109's.
pub const TOWN_CENTRE_444: MasterId = MasterId::named(444);
/// The scout cavalry (`.dat` SCOUT, raw `0x625c`), which the task finder treats apart.
pub const ID_448: MasterId = MasterId::named(448);
/// The light cavalry (`.dat` LTCAV, raw `0x62be`), which the task finder treats apart.
pub const ID_546: MasterId = MasterId::named(546);
/// A town centre (`0x26d`): a delete also tells the world's `+0x820`, a builder may place it and
/// its availability counts the town centres standing.
pub const TOWN_CENTRE_621: MasterId = MasterId::named(621);
/// The siege tower (`.dat` SIEGTWR, raw `0x64ed`), which the task finder treats apart.
pub const ID_1105: MasterId = MasterId::named(1105);
/// The mule cart (`.dat` MULECART, class 3, `0x710`, raw `0x67ac`), which answers the town bell besides the villagers and
/// which a building's watch leaves out.
pub const MASTER_1808: MasterId = MasterId::named(1808);
/// The mangrove pasture (`.dat` `PASTURE_MANGROVE`, `0x765`): a completed building's stack unit on
/// mangrove or rice terrain, and a pasture a gather and a hunt treat apart.
pub const PASTURE_MANGROVE: MasterId = MasterId::named(1893);
/// The pasture (`.dat` `PASTURE`, 1897), which a gather and a hunt treat apart with
/// [`PASTURE_MANGROVE`].
pub const PASTURE: MasterId = MasterId::named(1897);
/// The pasture master (`0x761`) whose foundation keeps its snow and whose stack unit depends on
/// the rice terrain under it.
pub const PASTURE_1889: MasterId = MasterId::named(1889);
/// Master 6 (raw `0x60a2`): a target of it takes the group's class list only
/// (`FUN_14100c6c0`).
pub const ID_6: MasterId = MasterId::named(6);
/// Master 7 (raw `0x60a3`), as [`ID_6`].
pub const ID_7: MasterId = MasterId::named(7);
/// Master 282 (raw `0x61b6`), one of the group's special units' masters (`FUN_14100c6c0`).
pub const ID_282: MasterId = MasterId::named(282);
/// Master 329 (raw `0x61e5`), as [`ID_282`].
pub const ID_329: MasterId = MasterId::named(329);
/// Master 330 (raw `0x61e6`), as [`ID_282`].
pub const ID_330: MasterId = MasterId::named(330);
/// Master 556 (raw `0x62c8`), as [`ID_282`].
pub const ID_556: MasterId = MasterId::named(556);
/// Master 0, as the `.dat` numbers it.
pub const ID_0: MasterId = MasterId::named(0);
/// Master 35, as the `.dat` numbers it.
pub const ID_35: MasterId = MasterId::named(35);
/// Master 42, as the `.dat` numbers it.
pub const ID_42: MasterId = MasterId::named(42);
/// Master 73, as the `.dat` numbers it.
pub const ID_73: MasterId = MasterId::named(73);
/// Master 331, as the `.dat` numbers it.
pub const ID_331: MasterId = MasterId::named(331);
/// Master 357, as the `.dat` numbers it.
pub const ID_357: MasterId = MasterId::named(357);
/// Master 422, as the `.dat` numbers it.
pub const ID_422: MasterId = MasterId::named(422);
/// Master 527, as the `.dat` numbers it.
pub const ID_527: MasterId = MasterId::named(527);
/// Master 528, as the `.dat` numbers it.
pub const ID_528: MasterId = MasterId::named(528);
/// Master 548, as the `.dat` numbers it.
pub const ID_548: MasterId = MasterId::named(548);
/// Master 559, as the `.dat` numbers it.
pub const ID_559: MasterId = MasterId::named(559);
/// Master 814, as the `.dat` numbers it.
pub const ID_814: MasterId = MasterId::named(814);
/// Master 826, as the `.dat` numbers it.
pub const ID_826: MasterId = MasterId::named(826);
/// Master 854, as the `.dat` numbers it.
pub const ID_854: MasterId = MasterId::named(854);
/// Master 1104, as the `.dat` numbers it.
pub const ID_1104: MasterId = MasterId::named(1104);
/// Master 1187, as the `.dat` numbers it.
pub const ID_1187: MasterId = MasterId::named(1187);
/// Master 1188, as the `.dat` numbers it.
pub const ID_1188: MasterId = MasterId::named(1188);
/// Master 1258, as the `.dat` numbers it.
pub const ID_1258: MasterId = MasterId::named(1258);
/// Master 1744, as the `.dat` numbers it.
pub const ID_1744: MasterId = MasterId::named(1744);
/// Master 1746, as the `.dat` numbers it.
pub const ID_1746: MasterId = MasterId::named(1746);
/// Master 1894, as the `.dat` numbers it.
pub const ID_1894: MasterId = MasterId::named(1894);
/// Master 1898, as the `.dat` numbers it.
pub const ID_1898: MasterId = MasterId::named(1898);
/// Master 2133, as the `.dat` numbers it.
pub const ID_2133: MasterId = MasterId::named(2133);
/// Master 2135, as the `.dat` numbers it.
pub const ID_2135: MasterId = MasterId::named(2135);
/// Master 2172, as the `.dat` numbers it.
pub const ID_2172: MasterId = MasterId::named(2172);
/// Master 1663 (`0x67f`), whose upgrades move the tracker's list of the master they replace.
pub const ID_1663: MasterId = MasterId::named(1663);
/// Master 1697 (`0x6a1`), likewise.
pub const ID_1697: MasterId = MasterId::named(1697);
/// Master 2134, between 2133 and 2135 of the self-destructing attackers.
pub const ID_2134: MasterId = MasterId::named(2134);
