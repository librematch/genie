//! The values the slice derives from a master after a change ([`follow`]): the
//! sprites' speed and facets, a moving unit's values.

use crate::map::Distance;
use crate::map::terrain::Terrain;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::ObjectFlags;
use crate::objects::sprite::top_speed;
use crate::world::WholeSeconds;

/// A sprite's speed factor (the `.dat` graphic's `speed_multiplier`): the master's speed times
/// it when above 0, else the master's speed (as the masters' loader derives
/// [`MoveStats`]' speeds).
fn sprite_speed(base: Speed, sprite: Option<SpriteId>, graphics: &Graphics) -> Speed {
    top_speed(base, sprite.and_then(|s| graphics.get(s)))
}

/// What a unit moving with master `master` reads after attribute `attribute` changed
/// (`stats`, the master's [`Master::move_stats`] or a unit's copy): in the game the unit reads its
/// master, so its radii, speeds, turn time, terrain restriction (its mask from `terrain`,
/// kept without one), fast turn, weapon range and facets follow the master's values.
pub fn follow_moving(
    master: &Master,
    stats: &mut MoveStats,
    attribute: MasterAttribute,
    graphics: &Graphics,
    terrain: Option<&Terrain>,
) {
    use crate::objects::masters::MasterAttribute;
    match attribute {
        MasterAttribute::SIZE_X | MasterAttribute::SIZE_Y | MasterAttribute::SIZE_Z => {
            stats.radius = master.radius.with_height(master.height.get());
        }
        MasterAttribute::MOVEMENT_SPEED
        | MasterAttribute::STANDING_GRAPHIC
        | MasterAttribute::WALKING_GRAPHIC
        | MasterAttribute::RUNNING_GRAPHIC => {
            stats.base = master.animated_part().max_speed;
            let run = master
                .moving_part()
                .run_sprite
                .or_else(|| master.moving_part().move_sprite);
            stats.speed = sprite_speed(master.animated_part().max_speed, run, graphics);
        }
        MasterAttribute::ROTATION_SPEED => stats.turn_time = master.moving_part().turn_speed,
        MasterAttribute::TERRAIN_RESTRICTION_ID => {
            // a restriction below 0 has no mask; the stats keep theirs
            if let Some(r) = master.terrain_restriction {
                stats.restriction = r;
                if let Some(t) = terrain {
                    stats.restriction_mask = t.restriction_mask(r);
                }
            }
        }
        MasterAttribute::UNIT_TRAIT | MasterAttribute::TRAIT_PIECE => {
            stats.fast_turn = master.object_flags.contains(ObjectFlags::SHIP);
        }
        MasterAttribute::MAXIMUM_RANGE => stats.weapon_range = master.combat_part().weapon_range,
        _ => {}
    }
    if matches!(
        attribute,
        MasterAttribute::STANDING_GRAPHIC
            | MasterAttribute::STANDING_GRAPHIC_2
            | MasterAttribute::WALKING_GRAPHIC
            | MasterAttribute::RUNNING_GRAPHIC
            | MasterAttribute::ATTACK_GRAPHIC
    ) {
        stats.facets = master.sprite_facets(graphics);
    }
}

/// The values the slice derives from master `master`'s after attribute `attribute` changed: its
/// height test, whether it has damage sprites, its train locations' ids and first time, and what
/// a unit moving with it reads ([`follow_moving`]); a missile's flight reads the master as it
/// is ([`Master::missile_stats`]).
pub fn follow(
    master: &mut Master,
    attribute: MasterAttribute,
    graphics: &Graphics,
    terrain: Option<&Terrain>,
) {
    use crate::objects::masters::MasterAttribute;
    match attribute {
        MasterAttribute::SIZE_X | MasterAttribute::SIZE_Y | MasterAttribute::SIZE_Z => {
            master.tall = Distance::ZERO < master.height;
        }
        MasterAttribute::DAMAGE_GRAPHICS_ENTRY_MOD | MasterAttribute::DAMAGE_GRAPHICS_TOTAL_NUM => {
            master.damage_sprites = !master.damage_graphics.is_empty();
        }
        MasterAttribute::TRAIN_LOCATION
        | MasterAttribute::TRAIN_TIME
        | MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD
        | MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
            if let Some(p) = master.creatable.as_mut() {
                p.trained_at = p
                    .train_locations
                    .iter()
                    .filter_map(|t| t.location)
                    .collect();
                p.build_time = p
                    .train_locations
                    .first()
                    .map_or(WholeSeconds::ZERO, |t| t.time);
            }
        }
        _ => {}
    }
    if let Some(mut stats) = master.move_stats.take() {
        follow_moving(master, &mut stats, attribute, graphics, terrain);
        master.move_stats = Some(stats);
    }
}
