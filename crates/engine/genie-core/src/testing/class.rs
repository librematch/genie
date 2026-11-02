//! The classes genie-core's tests name: build 158041's `.dat` values, as the tests' masters carry
//! them. The engine names no class (FD-046-D43): it asks the build layer's roles and slots.

use crate::objects::masters::ClassId;

/// The class of a master that has none (−1; [`ClassId::of_master`]).
pub const NO_CLASS: ClassId = ClassId::of_master(-1);
/// Artifacts (relics' neighbours that no blast reaches).
pub const ARTIFACT: ClassId = ClassId::named(1);
/// The trade boat (`-0xeaa` raw): its cared classes and its trade partner are not the cart's.
pub const TRADE_BOAT: ClassId = ClassId::named(2);
/// Buildings, which a unit's garrison task names.
pub const BUILDING: ClassId = ClassId::named(3);
/// Villagers (`-0xea8` raw).
pub const VILLAGER: ClassId = ClassId::named(4);
/// The prey animals (`-0xea3` raw).
pub const PREY: ClassId = ClassId::named(9);
/// The predators (`-0xea2` raw), whose unit AI's slot 0x1f0 a failed Attack calls.
pub const PREDATOR: ClassId = ClassId::named(10);
/// Siege weapons (`-0xe9f` raw, the short `0xd`).
pub const SIEGE: ClassId = ClassId::named(13);
/// Trees.
pub const TREE: ClassId = ClassId::named(15);
/// Monks (`-0xe9a` raw), whose stop leaves the state position.
pub const MONK: ClassId = ClassId::named(18);
/// The trade cart (decoded `0x13`).
pub const TRADE_CART: ClassId = ClassId::named(19);
/// The class whose busy units skip the idle check (`-0xe98` raw).
pub const TRANSPORT: ClassId = ClassId::named(20);
/// The fishing ship (decoded 21, `0x15`).
pub const FISHING_SHIP: ClassId = ClassId::named(21);
/// The warships (`-0xe96` raw), whose attackers make a computer's unit warn its group.
pub const WARSHIP: ClassId = ClassId::named(22);
/// Walls (decoded 27, `0x1b`).
pub const WALL: ClassId = ClassId::named(27);
/// Gates (decoded 39), whose annexes stay in their owner's watched list.
pub const GATE: ClassId = ClassId::named(39);
/// Relics (`.dat` class 42, `0x2a`, `-0xe82` raw): a delete spares them.
pub const RELIC: ClassId = ClassId::named(42);
/// Monks carrying a relic (`.dat` class 43, `-0xe81` raw), which the task finder treats apart.
pub const CLASS_43: ClassId = ClassId::named(43);
/// Farms and a pasture's livestock (decoded 49, `0x31`).
pub const FARM: ClassId = ClassId::named(49);
/// Packed siege units (`.dat` class 51, `-0xe79` raw: the packed trebuchet and mangonel, master type
/// 80): they block a placement though desolid, and their ghosts stay watched.
pub const CLASS_51: ClassId = ClassId::named(51);
/// Towers.
pub const TOWER: ClassId = ClassId::named(52);
/// Boarding boats (`.dat` class 53, `-0xe77` raw), which the task finder treats apart.
pub const CLASS_53: ClassId = ClassId::named(53);
/// Unpacked siege units (`.dat` class 54, `-0xe76` raw: the trebuchet, the bombard cannon): they
/// block a placement though desolid, and their ghosts stay watched.
pub const CLASS_54: ClassId = ClassId::named(54);
/// Scorpions and the like (`.dat` class 55, `-0xe75` raw), which the task finder treats apart.
pub const CLASS_55: ClassId = ClassId::named(55);
/// Livestock (decoded 58, `0x3a`, `-0xe72` raw), which goes to Gaia when killed.
pub const LIVESTOCK: ClassId = ClassId::named(58);
/// The king's class (`-0xe71` raw).
pub const KING: ClassId = ClassId::named(59);
/// Class 12 (`-0xea0` raw): a group's command on an attack task of a target of it lists the
/// group's special units (`FUN_14100c6c0`).
pub const CLASS_12: ClassId = ClassId::named(12);
/// Class 24 (`-0xe94` raw), listed as [`CLASS_12`] is.
pub const CLASS_24: ClassId = ClassId::named(24);
/// Class 35 (`-0xe89` raw): a target the group's command lists the units by formation category
/// for (`FUN_14100c6c0`).
pub const CLASS_35: ClassId = ClassId::named(35);
/// Class 46 (`-0xe7e` raw): the units the group's special units list takes first
/// (`FUN_14100c6c0`).
pub const CLASS_46: ClassId = ClassId::named(46);
/// Class 47 (`-0xe7d` raw), listed as [`CLASS_12`] is.
pub const CLASS_47: ClassId = ClassId::named(47);
/// Class 57 (`-0xe73` raw), listed as [`CLASS_12`] is.
pub const CLASS_57: ClassId = ClassId::named(57);
/// Class 61 (`-0xe6f` raw), listed as [`CLASS_35`] is.
pub const CLASS_61: ClassId = ClassId::named(61);
/// Class 0, as the `.dat` numbers it.
pub const CLASS_0: ClassId = ClassId::named(0);
/// Class 5, as the `.dat` numbers it.
pub const CLASS_5: ClassId = ClassId::named(5);
/// Class 6, as the `.dat` numbers it.
pub const CLASS_6: ClassId = ClassId::named(6);
/// Class 7, as the `.dat` numbers it.
pub const CLASS_7: ClassId = ClassId::named(7);
/// Class 8, as the `.dat` numbers it.
pub const CLASS_8: ClassId = ClassId::named(8);
/// Class 11, as the `.dat` numbers it.
pub const CLASS_11: ClassId = ClassId::named(11);
/// Class 14, as the `.dat` numbers it.
pub const CLASS_14: ClassId = ClassId::named(14);
/// Class 16, as the `.dat` numbers it.
pub const CLASS_16: ClassId = ClassId::named(16);
/// Class 17, as the `.dat` numbers it.
pub const CLASS_17: ClassId = ClassId::named(17);
/// Class 23, as the `.dat` numbers it.
pub const CLASS_23: ClassId = ClassId::named(23);
/// Class 26, as the `.dat` numbers it.
pub const CLASS_26: ClassId = ClassId::named(26);
/// Class 28, as the `.dat` numbers it.
pub const CLASS_28: ClassId = ClassId::named(28);
/// Class 29, as the `.dat` numbers it.
pub const CLASS_29: ClassId = ClassId::named(29);
/// Class 30, as the `.dat` numbers it.
pub const CLASS_30: ClassId = ClassId::named(30);
/// Class 31, as the `.dat` numbers it.
pub const CLASS_31: ClassId = ClassId::named(31);
/// Class 32, as the `.dat` numbers it.
pub const CLASS_32: ClassId = ClassId::named(32);
/// Class 33, as the `.dat` numbers it.
pub const CLASS_33: ClassId = ClassId::named(33);
/// Class 34, as the `.dat` numbers it.
pub const CLASS_34: ClassId = ClassId::named(34);
/// Class 36, as the `.dat` numbers it.
pub const CLASS_36: ClassId = ClassId::named(36);
/// Class 38, as the `.dat` numbers it.
pub const CLASS_38: ClassId = ClassId::named(38);
/// Class 40, as the `.dat` numbers it.
pub const CLASS_40: ClassId = ClassId::named(40);
/// Class 41, as the `.dat` numbers it.
pub const CLASS_41: ClassId = ClassId::named(41);
/// Class 44, as the `.dat` numbers it.
pub const CLASS_44: ClassId = ClassId::named(44);
/// Class 45, as the `.dat` numbers it.
pub const CLASS_45: ClassId = ClassId::named(45);
/// Class 48, as the `.dat` numbers it.
pub const CLASS_48: ClassId = ClassId::named(48);
/// Class 50, as the `.dat` numbers it.
pub const CLASS_50: ClassId = ClassId::named(50);
/// Class 56, as the `.dat` numbers it.
pub const CLASS_56: ClassId = ClassId::named(56);
/// Class 60, as the `.dat` numbers it.
pub const CLASS_60: ClassId = ClassId::named(60);
/// Class 62, as the `.dat` numbers it.
pub const CLASS_62: ClassId = ClassId::named(62);
/// Class 63, as the `.dat` numbers it.
pub const CLASS_63: ClassId = ClassId::named(63);
/// Class 64, as the `.dat` numbers it.
pub const CLASS_64: ClassId = ClassId::named(64);
/// Class 25, as the `.dat` numbers it.
pub const CLASS_25: ClassId = ClassId::named(25);
/// Class 37, as the `.dat` numbers it.
pub const CLASS_37: ClassId = ClassId::named(37);

/// A table of classes from the game's numbers (the unit AIs' cared lists): each a class the
/// code names.
#[must_use]
pub const fn classes<const N: usize>(values: [i16; N]) -> [ClassId; N] {
    let mut out = [NO_CLASS; N];
    let mut i = 0;
    while i < N {
        out[i] = ClassId::named(values[i]);
        i += 1;
    }
    out
}
