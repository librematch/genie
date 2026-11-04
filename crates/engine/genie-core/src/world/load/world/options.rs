//! The game options the simulation reads ([`settings`]) from the header's `GameOptions`.

use crate::world::settings::{FarmMode, OptionFlags, Settings};
use crate::world::{
    Age, ContentMode, GameMode, ResourceLevel, SubModeFlags, TreatyLength, Visibility,
};
use genie_save::generated::world::GameOptions;

/// The game options the simulation reads ([`Settings`]) from the header's
/// `GameOptions`, as `FUN_141305310` loads them.
#[must_use]
pub const fn settings(game_options: &GameOptions) -> Settings {
    Settings {
        game_mode: GameMode::of_data(game_options.game_mode),
        ending_age: Age::of_data(game_options.ending_age),
        visibility_kind: Visibility::of_data(game_options.visibility_kind),
        sub_game_mode_flag: SubModeFlags::from_bits_retain(game_options.sub_game_mode_flag),
        farm_mode: FarmMode::of_option(game_options.walkable_farms),
        selected_content_mode: ContentMode::of_data(game_options.selected_content_mode),
        treaty_length: TreatyLength::of_minutes(game_options.treaty_length),
        starting_age: Age::of_data(game_options.starting_age),
        resource_level: ResourceLevel::of_data(game_options.resource_level),
        flags: option(
            OptionFlags::FULL_TECH_TREE,
            game_options.full_tech_tree_enabled,
        )
        .union(option(
            OptionFlags::VILLAGER_FORCE_DROP,
            game_options.villager_force_drop,
        ))
        .union(option(
            OptionFlags::CHEATS_ENABLED,
            game_options.cheats_enabled,
        ))
        .union(option(OptionFlags::TEAMS_LOCKED, game_options.teams_locked))
        .union(option(OptionFlags::TURBO, game_options.turbo_enabled))
        .union(option(
            OptionFlags::SHARED_EXPLORATION,
            game_options.shared_exploration,
        ))
        .union(option(OptionFlags::ANTIQUITY, game_options.antiquity_mode)),
    }
}

/// `flag` when the option's byte is not 0.
const fn option(flag: OptionFlags, byte: u8) -> OptionFlags {
    if byte != 0 {
        flag
    } else {
        OptionFlags::empty()
    }
}
