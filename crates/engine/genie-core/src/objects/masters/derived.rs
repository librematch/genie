//! The values the slice derives from a master after a change ([`Master::follow`]): the
//! sprites' speed and facets, a moving unit's values.

use crate::map::Distance;
use crate::map::terrain::Terrain;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::facing::SpriteFacets;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::{Master, master_type};
use crate::objects::sprite::top_speed;
use crate::world::WholeSeconds;

/// A sprite's speed factor (the `.dat` graphic's `speed_multiplier`): the master's speed times
/// it when above 0, else the master's speed (as the masters' loader derives
/// [`MoveStats`]' speeds).
pub(super) fn sprite_speed(base: Speed, sprite: Option<SpriteId>, graphics: &Graphics) -> Speed {
    top_speed(base, sprite.and_then(|s| graphics.get(s)))
}

impl Master {
    /// Whether its objects move: a moving master (types 30 and up) whose speed (`+0x740`, the
    /// master's slot 0xe8, [`MoveStats::base`]) is above 0 (`FUN_14123bb20`'s test).
    #[must_use]
    pub fn moves(&self) -> bool {
        self.moving.is_some_and(|m| Speed::ZERO < m.base)
    }

    /// The facets the facing routines read of a unit of this master ([`MoveStats::facets`]):
    /// those of its standing sprites, its moving ones (types 30 and up), its attack sprite (50
    /// and up) and its tasks' sprites, when every one the graphics hold has the same count and
    /// bit 1 (as the masters' loader derives them).
    #[must_use]
    pub fn sprite_facets(&self, graphics: &Graphics) -> Option<SpriteFacets> {
        let mut sprites = vec![self.standing_sprite, self.standing_sprite_2];
        if master_type::MOVING <= self.kind {
            sprites.extend([self.move_sprite, self.run_sprite]);
        }
        if master_type::RGE_COMBAT <= self.kind {
            sprites.push(self.attack_sprite);
        }
        for t in &self.tasks {
            sprites.extend(t.sprites.ids());
        }
        let mut seen = sprites
            .into_iter()
            .flatten()
            .filter_map(|id| graphics.get(id))
            .map(|g| SpriteFacets {
                count: g.facets,
                turns: g.flags.contains(SpriteFlags::TURNS),
            });
        let first = seen.next()?;
        seen.all(|f| f == first).then_some(first)
    }

    /// What a unit moving with this master reads after attribute `attribute` changed
    /// (`stats`, the master's [`Master::moving`] or a unit's copy): in the game the unit reads its
    /// master, so its radii, speeds, turn time, terrain restriction (its mask from `terrain`,
    /// kept without one), fast turn, weapon range and facets follow the master's values.
    pub fn follow_moving(
        &self,
        stats: &mut MoveStats,
        attribute: MasterAttribute,
        graphics: &Graphics,
        terrain: Option<&Terrain>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::SIZE_X | a::SIZE_Y | a::SIZE_Z => {
                stats.radius = self.radius.with_height(self.height.get());
            }
            a::MOVEMENT_SPEED | a::STANDING_GRAPHIC | a::WALKING_GRAPHIC | a::RUNNING_GRAPHIC => {
                stats.base = self.max_speed;
                let run = self.run_sprite.or(self.move_sprite);
                stats.speed = sprite_speed(self.max_speed, run, graphics);
            }
            a::ROTATION_SPEED => stats.turn_time = self.turn_speed,
            a::TERRAIN_RESTRICTION_ID => {
                // a restriction below 0 has no mask; the stats keep theirs
                if let Some(r) = self.terrain_restriction {
                    stats.restriction = r;
                    if let Some(t) = terrain {
                        stats.restriction_mask = t.restriction_mask(r);
                    }
                }
            }
            a::UNIT_TRAIT | a::TRAIT_PIECE => {
                stats.fast_turn = self.object_flags.contains(ObjectFlags::SHIP);
            }
            a::MAXIMUM_RANGE => stats.weapon_range = self.weapon_range,
            _ => {}
        }
        if matches!(
            attribute,
            a::STANDING_GRAPHIC
                | a::STANDING_GRAPHIC_2
                | a::WALKING_GRAPHIC
                | a::RUNNING_GRAPHIC
                | a::ATTACK_GRAPHIC
        ) {
            stats.facets = self.sprite_facets(graphics);
        }
    }

    /// The values the slice derives from this master's after attribute `attribute` changed: its
    /// height test, its missile's flight, whether it has damage sprites and a special graphic,
    /// its train locations' ids and first time, and what a unit moving with it reads
    /// ([`Master::follow_moving`]).
    pub fn follow(
        &mut self,
        attribute: MasterAttribute,
        graphics: &Graphics,
        terrain: Option<&Terrain>,
    ) {
        use crate::objects::masters::attribute as a;
        match attribute {
            a::SIZE_X | a::SIZE_Y | a::SIZE_Z => {
                self.tall = Distance::ZERO < self.height;
                if let Some(m) = self.missile_stats.as_mut() {
                    m.radius = self.radius.with_height(self.height.get());
                }
            }
            a::MOVEMENT_SPEED => {
                if let Some(m) = self.missile_stats.as_mut() {
                    m.speed = self.max_speed;
                }
            }
            a::DAMAGE_GRAPHICS_ENTRY_MOD | a::DAMAGE_GRAPHICS_TOTAL_NUM => {
                self.damage_sprites = !self.damage_graphics.is_empty();
            }
            a::SPECIAL_GRAPHIC => self.special_graphic = self.special_sprite.is_some(),
            a::TRAIN_LOCATION
            | a::TRAIN_TIME
            | a::TRAIN_LOCATIONS_ENTRY_MOD
            | a::TRAIN_LOCATIONS_TOTAL_NUM => {
                self.trained_at = self
                    .train_locations
                    .iter()
                    .filter_map(|t| t.location)
                    .collect();
                self.build_time = self
                    .train_locations
                    .first()
                    .map_or(WholeSeconds::ZERO, |t| t.time);
            }
            _ => {}
        }
        if let Some(mut stats) = self.moving.take() {
            self.follow_moving(&mut stats, attribute, graphics, terrain);
            self.moving = Some(stats);
        }
    }
}
