//! What each unit-AI type cares for: the classes its searches look for, their radii and its
//! idle timeout.

use crate::map::Distance;
use crate::math::image::unit_ai::{
    RUN_DRAW_SCALE as UNIT_AI_RUN_DRAW_SCALE, RUN_EPSILON as UNIT_AI_RUN_EPSILON,
    RUN_ONE as UNIT_AI_RUN_ONE,
};
use crate::math::rng::Well512;
use crate::objects::masters::ClassId;
use crate::objects::masters::GatherGroup;
use crate::players::PlayerKind;
use crate::unit_ai::AiAction;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::{HUNTED_SIGHT_GROWTH, Surroundings};
use crate::world::Millis;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;

/// `runAwayFromAttackers` (`FUN_140c5e900`): attackers closer together than this
/// (`DAT_142eb0c60`) give a random direction, each part `2 r / RUN_DRAW_SCALE - RUN_ONE`
/// (`DAT_142eb69cc`, `DAT_14330eb8c`).
pub(super) const RUN_EPSILON: f32 = UNIT_AI_RUN_EPSILON;
pub(super) const RUN_DRAW_SCALE: f32 = UNIT_AI_RUN_DRAW_SCALE;
pub(super) const RUN_ONE: f32 = UNIT_AI_RUN_ONE;

/// The classes a hunted animal cares about (`+0x190`, the constructor's list).
pub const HUNTED_CARED: [ClassId; 28] = classes([
    0, 4, 26, 24, 6, 25, 17, 10, 12, 35, 23, 36, 28, 18, 13, 44, 46, 43, 45, 47, 50, 51, 54, 55,
    56, 57, 19, 59,
]);

/// The classes a lion cares about (`+0x190`, `FUN_140c11bd0`'s list; the game's `+0x6b4` of 1,
/// not met, adds 47, 18, 13 and 55).
pub const LION_CARED: [ClassId; 18] = classes([
    4, 0, 26, 24, 6, 25, 12, 35, 23, 36, 28, 44, 46, 45, 50, 56, 57, 9,
]);

/// The classes a predator (`TribePreditorUnitAIModule`, factory type 3) cares about (`+0x190`,
/// `FUN_140c11da0`'s list).
pub const PREDATOR_CARED: [ClassId; 2] = classes([58, 4]);

/// The classes a priest (factory type 9) cares about (`+0x190`, `FUN_140c11e30`'s 30 entries).
pub const PRIEST_CARED: [ClassId; 30] = classes([
    3, 0, 4, 26, 24, 6, 25, 17, 10, 12, 35, 23, 36, 28, 18, 13, 44, 46, 43, 45, 47, 50, 51, 54, 55,
    56, 57, 19, 52, 42,
]);

/// The classes a civilian cares about (`+0x190`, `TribeCivilianUnitAIModule`'s constructor
/// `FUN_140c115d0`).
pub const CIVILIAN_CARED: [ClassId; 39] = classes([
    3, 7, 8, 9, 10, 15, 17, 18, 32, 33, 27, 49, 39, 52, 58, 63, 0, 4, 26, 24, 6, 35, 25, 12, 23,
    36, 28, 22, 44, 46, 45, 47, 50, 52, 55, 56, 57, 19, 61,
]);

/// What the civilian's constructor gives the base (`FUN_140c2e080` with 7): `+0x1e8`, which its
/// search adds to its radius (`FUN_140c63e20`).
pub const CIVILIAN_SEARCH_GROWTH: Distance = Distance::new(7.0);

/// The civilian's search radius (`FUN_140c63e20`): its line of sight plus a tile, at least its
/// search radius, plus [`CIVILIAN_SEARCH_GROWTH`].
#[must_use]
pub fn civilian_search_radius(los: Distance, search_radius: Distance) -> Distance {
    let sight = los + Distance::new(1.0);
    let radius = if sight <= search_radius {
        search_radius
    } else {
        sight
    };
    radius + CIVILIAN_SEARCH_GROWTH
}

/// The civilian's slot 0x1b0 (`FUN_140c1c4f0`): the gather group of a target class (wood for
/// 15, forage for 7, fish for 5, 31 and 33, stone for 8, gold for 32 and 63), none (−1) for any
/// other.
#[must_use]
pub fn civilian_kind(class: ClassId, roles: &Roles) -> Option<GatherGroup> {
    {
        let held = roles.of_class(class);
        if held
            .intersects(ClassRoles::OCEAN_FISH | ClassRoles::DEEP_SEA_FISH | ClassRoles::SHORE_FISH)
        {
            Some(GatherGroup::FISH)
        } else if held.contains(ClassRoles::FORAGE_BUSH) {
            Some(GatherGroup::FORAGE)
        } else if held.contains(ClassRoles::STONE_MINE) {
            Some(GatherGroup::STONE)
        } else if held.contains(ClassRoles::TREE) {
            Some(GatherGroup::WOOD)
        } else if held.intersects(ClassRoles::GOLD_MINE | ClassRoles::GOLD_FISH) {
            Some(GatherGroup::GOLD)
        } else {
            None
        }
    }
}

/// The classes a soldier cares about (`+0x190`, `TribeSoldierUnitAIModule`'s constructor
/// `FUN_140c12300`; a computer player's adds 10).
pub const SOLDIER_CARED: [ClassId; 39] = classes([
    0, 3, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 27, 22, 20, 2, 21, 44, 46, 43, 45, 47,
    50, 51, 52, 53, 54, 55, 39, 56, 57, 19, 59, 49, 42, 61,
]);

/// A computer player's soldier's classes: [`SOLDIER_CARED`], then 10 (`FUN_140c12300`).
pub const SOLDIER_CARED_COMPUTER: [ClassId; 40] = classes([
    0, 3, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 27, 22, 20, 2, 21, 44, 46, 43, 45, 47,
    50, 51, 52, 53, 54, 55, 39, 56, 57, 19, 59, 49, 42, 61, 10,
]);

/// The classes a building cares about (`+0x190`, `TribeBuildingUnitAIModule`'s constructor
/// `FUN_140c11470`, its 39 entries in this order): the soldier's set, 52 moved behind 59.
pub const BUILDING_CARED: [ClassId; 39] = classes([
    0, 3, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 27, 22, 20, 2, 21, 44, 46, 43, 45, 47,
    50, 51, 53, 54, 55, 56, 57, 19, 59, 52, 49, 39, 42, 61,
]);

/// The classes an artifact cares about (`+0x190`, `TribeArtifactUnitAIModule`'s constructor
/// `FUN_140c11340`: the 0x84 bytes it copies from `DAT_142ef90f0` on, then 61).
pub const ARTIFACT_CARED: [ClassId; 33] = classes([
    0, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 22, 20, 2, 21, 44, 46, 43, 45, 47, 50, 51,
    53, 54, 55, 56, 57, 19, 59, 61,
]);

/// The classes a tower cares about (`+0x190`, `TribeTowerUnitAIModule`'s constructor
/// `FUN_140c12960`): the soldier's and 10.
pub const TOWER_CARED: [ClassId; 40] = classes([
    0, 3, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 27, 22, 20, 2, 21, 44, 46, 43, 45, 47,
    50, 51, 52, 53, 54, 55, 39, 56, 57, 19, 59, 49, 10, 42, 61,
]);

/// The classes a siege unit cares about (`+0x190`, `TribeSiegeUnitAIModule`'s constructor
/// `FUN_140c12040`: the soldier's first 28, then 53, 54, 55, 39, 56, 57, 19, 59, 39, 49, 42, 61:
/// 39 twice).
pub const SIEGE_CARED: [ClassId; 40] = classes([
    0, 3, 4, 26, 24, 6, 35, 25, 17, 12, 23, 36, 28, 18, 13, 27, 22, 20, 2, 21, 44, 46, 43, 45, 47,
    50, 51, 52, 53, 54, 55, 39, 56, 57, 19, 59, 39, 49, 42, 61,
]);

/// The classes a trade boat (class 2) cares about (`+0x190`, `FUN_140c12ab0`: the four of
/// `DAT_142ef9020`, then 21, 53 and 52).
pub const TRADE_SHIP_CARED: [ClassId; 7] = classes([3, 22, 20, 2, 21, 53, 52]);

/// The classes a trade cart (any other class of the trade ship's type) cares about (`+0x190`,
/// `FUN_140c12ab0`: the four of `DAT_142ef91f0`, then 19).
pub const TRADE_CART_CARED: [ClassId; 5] = classes([3, 17, 18, 52, 19]);

/// The classes a fishing ship cares about (`+0x190`, `FUN_140c11860`: the four of each of
/// `DAT_142ef9020`, `DAT_142ef9130` and `DAT_142ef9160`, then 49).
pub const FISHING_CARED: [ClassId; 13] = classes([3, 22, 20, 2, 21, 5, 31, 33, 53, 52, 27, 39, 49]);

/// The classes a unit AI of factory type `ai_kind` cares about (its constructor's `+0x190` list),
/// which its searches read: the trade AI's by its unit's `class` (a boat's or a cart's), the
/// soldier's with 10 for a computer player's (`FUN_140c12300` asks the owner when the AI is made;
/// taken as its owner now), the lion's without the four the world's `+0x6b4` of 1 adds.
pub(crate) fn cared_classes(
    ai_kind: UnitAiKind,
    class: Option<ClassId>,
    player_kind: PlayerKind,
    roles: &Roles,
) -> &'static [ClassId] {
    match ai_kind {
        UnitAiKind::Hunted => &HUNTED_CARED,
        UnitAiKind::Predator => &PREDATOR_CARED,
        // the boar's (`FUN_140c11720`), the ranged units' (`FUN_140c11f80`, `FUN_140c11f50`) and
        // the livestock's (`FUN_140c11d70`) constructors give none: the base's (`FUN_140c2e080`
        // with 0) alone
        UnitAiKind::Boar | UnitAiKind::Ranged | UnitAiKind::Ranged2 | UnitAiKind::Livestock => &[],
        UnitAiKind::Lion => &LION_CARED,
        UnitAiKind::Priest => &PRIEST_CARED,
        UnitAiKind::Civilian => &CIVILIAN_CARED,
        UnitAiKind::Building => &BUILDING_CARED,
        UnitAiKind::Transport => &TRANSPORT_CARED,
        UnitAiKind::SiegeTower => &SIEGE_TOWER_CARED,
        // The trade boat's class (`-0xeaa` raw): its cared classes and its trade partner are not the
        // trade cart's.
        UnitAiKind::Trade if roles.of_some_class(class).contains(ClassRoles::TRADE_BOAT) => {
            &TRADE_SHIP_CARED
        }
        UnitAiKind::Trade => &TRADE_CART_CARED,
        UnitAiKind::Fishing => &FISHING_CARED,
        UnitAiKind::Artifact => &ARTIFACT_CARED,
        UnitAiKind::Tower => &TOWER_CARED,
        UnitAiKind::Soldier if player_kind.is_computer() => &SOLDIER_CARED_COMPUTER,
        UnitAiKind::Soldier => &SOLDIER_CARED,
        UnitAiKind::Siege => &SIEGE_CARED,
    }
}

/// What a search of a unit AI of `ai_kind` covers (`FUN_140c63e20`), its unit of `class`, line
/// of sight `los` and search radius (slot 0x2c8): its type's [`cared_classes`] and its radius
/// ([`search_radius`]), the civilian's and the fishing ship's grown by their `+0x1e8`
/// ([`civilian_search_radius`]).
pub(crate) fn search_area(
    ai_kind: UnitAiKind,
    class: Option<ClassId>,
    los: Distance,
    radius: Distance,
    player_kind: PlayerKind,
    roles: &Roles,
) -> (&'static [ClassId], Distance) {
    let classes = cared_classes(ai_kind, class, player_kind, roles);
    let radius = if matches!(ai_kind, UnitAiKind::Civilian | UnitAiKind::Fishing) {
        civilian_search_radius(los, radius)
    } else {
        search_radius(los, radius)
    };
    (classes, radius)
}

/// The action values that need a live target (the target block's mask `0x4002080d` over the
/// value − `0x259`).
pub(super) const LIVE_TARGET_ACTIONS: [AiAction; 6] = [
    AiAction::Defend,
    AiAction::Heal,
    AiAction::Convert,
    AiAction::Follow,
    AiAction::Repair,
    AiAction::OnTarget,
];

/// The radius of the search `bestUnitToAttack` runs (`FUN_140c63e20`): the larger of the unit's
/// search radius (slot 0x2c8) and its line of sight plus 1, plus `+0x1e8`, which no save holds (0
/// in every unit probed).
pub(super) fn attack_search_radius(at: &Surroundings) -> Distance {
    search_radius(at.los, at.search_radius)
}

/// A unit AI's search radius (`FUN_140c63e20`'s, slot 0x2c8 against the line of sight): its
/// master's line of sight plus [`HUNTED_SIGHT_GROWTH`], or its search radius if that is as far.
pub(crate) fn search_radius(los: Distance, search_radius: Distance) -> Distance {
    let sight = los + HUNTED_SIGHT_GROWTH;
    if sight <= search_radius {
        search_radius
    } else {
        sight
    }
}

/// `UpdateAdjustedIdleTimeout` (`0x140c41e00`): `r % (base / 2) + 3 * base / 4`, one AI draw.
pub(super) fn adjusted_idle_timeout(base: Millis, ai: &mut Well512) -> Millis {
    let base = base.get();
    Millis::new(ai.rand15() % (base >> 1) + ((base * 3) >> 2))
}

/// The classes a transport ship cares about (`+0x190`, `TribeTransportShipUnitAIModule`'s
/// constructor `FUN_140c12c30`).
pub const TRANSPORT_CARED: [ClassId; 10] = classes([3, 22, 20, 2, 21, 53, 52, 27, 39, 49]);

/// The classes a siege tower cares about (`+0x190`, `TribeSiegeTowerUnitAIModule`'s constructor
/// `FUN_140c11fb0`).
pub const SIEGE_TOWER_CARED: [ClassId; 2] = classes([27, 39]);

/// A table of classes from the game's numbers, in the order its search takes them (a unit AI's
/// cared list, the fishing ship's foundations).
pub(crate) const fn classes<const N: usize>(values: [i16; N]) -> [ClassId; N] {
    let mut out = [ClassId::of_master(-1); N];
    let mut index = 0;
    while index < N {
        out[index] = ClassId::named(values[index]);
        index += 1;
    }
    out
}
