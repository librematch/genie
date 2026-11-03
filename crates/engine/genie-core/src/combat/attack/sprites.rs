//! The swing's and the arrival's sprites, the attack timer and the unit's turn to its target.

use crate::GenieError;
use crate::combat::abilities::Look;
use crate::combat::attack::Arms;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::Graphics;
use crate::objects::SpriteId;
use crate::objects::facing::{face, turn_toward};
use crate::objects::layers::Combat;
use crate::objects::layers::MotionMut;
use crate::objects::masters::{Master, MasterId, Masters};
use crate::objects::sprite::new_sprite;
use crate::objects::sprite::{ActiveSprite, Graphic};
use crate::world::Millis;
use crate::world::NO_STANDING_SPRITE;
use crate::world::Seconds;
use crate::world::WorldTime;
use crate::world::layer::generated::AttackTuning;
use crate::world::layer::generated::SpriteTuning;

/// The sprites a swing of a unit (its terrain look `look`, its master `master`) takes before its
/// Attack's own ([`SwingSprites`]): its look's attack sprite while it shows its look, its
/// master's special graphic.
#[must_use]
pub fn swing_sprites(look: Option<&Look>, master: &Master, graphics: &Graphics) -> SwingSprites {
    let g = |id: Option<SpriteId>| id.and_then(|s| graphics.get(s).copied());
    SwingSprites {
        look: g(look.and_then(|l| l.attacking)),
        special: master
            .special_graphic
            .then(|| g(master.special_sprite))
            .flatten(),
    }
}

/// An Attack's draw (its swing's second sprite) where the caller holds no Object stream
/// ([`crate::combat::attack::Ground::object_rng`]).
pub(super) const NO_OBJECT_STREAM: GenieError =
    GenieError::unsupported("a hit without the Object stream");

/// The sprites a swing takes before the Attack's own ([`Attack::set_swing`]).
///
/// [`Attack::set_swing`]: crate::combat::attack::Attack::set_swing
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwingSprites {
    /// The unit's terrain look's attack sprite (`+0x970`, `FUN_14107cc20`) while it shows its
    /// look (`+0x8a9`).
    pub look: Option<Graphic>,
    /// Its master's special graphic (`+0x8e8`, a `TRIBE_Master_Combat_Object`'s).
    pub special: Option<Graphic>,
}

/// `FUN_141089820`: the attack timer of a unit's combat layer `combat` becomes `t` and its reload world time the world's
/// time `now` plus `t` in whole milliseconds (slot 0x248).
pub(super) fn set_attack_timer(
    combat: &mut Combat,
    now: WorldTime,
    t: Seconds,
    attack_tuning: &AttackTuning,
) {
    combat.attack_timer = t;
    combat.reload_worldtime = now.after(Millis::new(
        (t.get() * attack_tuning.reload_ms) as i64 as u32,
    ));
}

/// `FUN_14120c0a0`'s test of `master`: it has a volley missile or a charge projectile its player
/// has (`masters`), so its hits go through its volley even without a missile.
pub(crate) fn volleys(master: &Master, masters: &Masters) -> bool {
    let has = |id: Option<MasterId>| id.and_then(|id| masters.get(id)).is_some();
    has(master.volley_missile) || has(master.charge_projectile)
}

/// Slot 0x68 (`FUN_141217000`): the unit's sprite becomes `g` unless it is already; a new sprite
/// starts a new instance (`FUN_1413b2ef0`).
/// [`Attack::arrived_sprite`] for a caller that holds the unit's sprite and body: the idle sprite
/// of `arms` (slot 0x68), the animation speed `+0x7b8` cleared.
///
/// [`Attack::arrived_sprite`]: crate::combat::attack::Attack::arrived_sprite
pub(crate) fn show_arrived(
    arms: &Arms,
    sprite: &mut Option<ActiveSprite>,
    mut motion: MotionMut<'_>,
    rng: &mut Well512,
    sprite_tuning: SpriteTuning,
) -> Result<(), GenieError> {
    let g = arms.idle_sprite.ok_or(NO_STANDING_SPRITE)?;
    new_sprite(sprite, motion.move_stats_mut(), g, rng, sprite_tuning);
    motion.set_anim_speed(Speed::ZERO);
    Ok(())
}

/// `FUN_1413b9f00` on the object's sprite list (`+0x148`, genie's: the unit's one sprite): the
/// first instance of graphic `g` (a node's `+0x8`) goes back to its first frame (its slot 9 with
/// `0x7fff`, [`ActiveSprite::reset_frame`]); false when the list holds none.
pub(crate) fn reset_sprite(sprite: &mut Option<ActiveSprite>, g: Graphic) -> bool {
    match sprite.as_mut() {
        Some(sp) if sp.graphic.id == g.id => {
            sp.reset_frame();
            true
        }
        _ => false,
    }
}

/// `FUN_141162db0`: the unit at `at` turns to face `to` with its sprites' facets, at once for a
/// unit without a turn time ([`crate::objects::facing::face`]), else a facet a turn time
/// ([`crate::objects::facing::turn_toward`] by the world time); returns whether it faces. The facets are
/// its current sprite's ([`Mover::facets`]).
pub(crate) fn face_target(
    mover: &mut Mover,
    at: Position,
    to: Position,
    world_time: WorldTime,
) -> Result<bool, GenieError> {
    let facets = mover.facets().ok_or(GenieError::unsupported(
        "a unit facing a target whose sprites differ in their facets",
    ))?;
    let (at, to) = (at.ground(), to.ground());
    if mover.stats.turn_time == Seconds::ZERO {
        mover.angle = face(at, mover.angle, to, Some(facets));
        return Ok(true);
    }
    let (angle, last, faces) = turn_toward(
        at,
        mover.angle,
        to,
        Some(facets),
        mover.stats.turn_time,
        mover.last_turn,
        world_time,
    );
    (mover.angle, mover.last_turn) = (angle, last);
    Ok(faces)
}
