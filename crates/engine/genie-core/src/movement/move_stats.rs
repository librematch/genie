//! What a moving master's motion reads of its master and its sprites (FD-046: the model's part of the movement).

use crate::map::Distance;
use crate::map::Radii;
use crate::map::{RestrictionId, TerrainMask};
use crate::movement::MoveAlgorithm;
use crate::movement::Speed;
use crate::movement::TurnRate;
use crate::objects::facing::SpriteFacets;
use crate::objects::masters::FlyMode;
use crate::objects::sprite::{Graphic, top_speed};
use crate::world::Seconds;
use genie_diff::Diff;

/// What movement reads of the unit's master.
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MoveStats {
    /// The master's `fly_mode` (`+0x608`): how its place takes the ground's height
    /// ([`Mover::grounded`]).
    pub fly_mode: FlyMode,
    /// The top speed (object slot 65, `maximumSpeed`, `0x141202c70` -> `FUN_14114d1d0`): the
    /// master's speed ([`MoveStats::base`]), times the current sprite's speed factor when that is
    /// above 0. A column derived from the two: the sprite's one transform writes it
    /// ([`crate::objects::sprite::new_sprite`]), and so does a change of the base.
    pub speed: Speed,
    /// The master's speed (`+0x740`, the master's slot 0xe8): the top speed's base.
    pub base: Speed,
    /// Master `+0x774`: the most the unit turns per second ([`turn_rate`]).
    pub turn_rate: TurnRate,
    /// Master `+0x76c` and `+0x770`: the turn radius is `turn_radius × speed / turn_radius_speed`
    /// (0 and the largest float for every master the game copies, [`turn_rate`]).
    pub turn_radius: Distance,
    /// `+0x770`: the speed at which the turn radius is its value.
    pub turn_radius_speed: Speed,
    /// Master `+0x5d8`, `+0x5dc`, `+0x5e0`.
    #[diff(gap = "an own master's radius (a foundation's, a dying annex's) is not modelled")]
    pub radius: Radii,
    /// The master's terrain restriction (`+0x606`) and the terrain classes it lets the unit stand
    /// on (its mask).
    pub restriction: RestrictionId,
    /// The terrain classes the restriction lets it stand on (its mask).
    pub restriction_mask: TerrainMask,
    /// The master's turn time (`+0x758`, the `.dat`'s `rotation_speed`): the seconds between the
    /// facet steps of a unit facing a target; 0 faces at once (`FUN_141162db0`).
    pub turn_time: Seconds,
    /// The master's move algorithm (`+0x768`, the `.dat`'s `move_algorithm`).
    pub move_algorithm: MoveAlgorithm,
    /// Master `+0x780`, `min_collision_size_multiplier`: the walk's query grows moving allies by
    /// the unit's radius times this.
    pub min_collision: f32,
    /// The master's object flags (`+0x654`) have bit 1: the unit turns a hundred times faster.
    pub fast_turn: bool,
    /// The facets the facing routines read of the unit's current sprite: the master's, when
    /// every sprite it shows has the same count and bit 1 (None otherwise, or without sprites).
    pub facets: Option<SpriteFacets>,
    /// The master's weapon range (slot 0x230, `+0x828`; 0 for an object without combat values),
    /// which the drop-off walker's start test reads (`FUN_140fa0110`).
    pub weapon_range: Distance,
}

impl MoveStats {
    /// The top speed with the sprite `shown` (None: none): the master's speed [`MoveStats::base`]
    /// times the sprite's factor ([`crate::objects::sprite::top_speed`]). The one writer of
    /// [`MoveStats::speed`] for a unit: a new sprite ([`crate::objects::sprite::new_sprite`]), a
    /// standing show ([`Mover::shows`]) and a change of the base call it.
    pub(crate) fn show(&mut self, shown: Option<&Graphic>) {
        self.speed = top_speed(self.base, shown);
    }
}
