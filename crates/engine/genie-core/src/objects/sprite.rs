//! An object's active sprite: the frame of its sprite (a `.dat` graphic) and, for a graphic that
//! animates, the clock that advances it. The save holds it (the object's active sprite list). An
//! attack lands on a frame of its sprite ([`crate::combat::attack`]).
//!
//! The list's node factory (`FUN_1408dc940`) makes an animated node
//! (`RGE_Active_Animated_Sprite`, vtable `0x142f751b0`) for a graphic with sequence bit 0 and a
//! static one (`RGE_Active_Sprite`, vtable `0x142f75128`) for the others; a new sprite is a new
//! node, so the graphic tells the two apart.
//!
//! 158041, animated: start `FUN_1413b2ef0` (with `FUN_1413b3490`), update (slot 5)
//! `FUN_1413b5070`, the frame's reach test (slot 11) `FUN_1413b2d70`, the frame reset (slot 9)
//! `FUN_1413b4ff0`. Static: start `FUN_1413b3490`, update `FUN_1413b5330` (none), reach test
//! `FUN_1413b2d90` (always), frame reset `FUN_1413b5030`, looped (slot 12) `FUN_1413b38e0`
//! (never).

use crate::GenieError;
use genie_model::objects::sprite::graphic;

pub use self::graphic::Graphic;
pub(crate) use self::graphic::top_speed;
use crate::combat::abilities::Look;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::{at_most_or_unordered, below_or_unordered};
use crate::math::image::sprite::EPSILON as SPRITE_EPSILON;
use crate::math::rng::Well512;
use crate::movement::MoveStats;
use crate::movement::Speed;
use crate::objects::SpeedMultiplier;
use crate::objects::SpriteFlags;
use crate::objects::SpriteId;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::{AnimateState, Frame};
use crate::world::Seconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;
use genie_diff::Diff;

/// The gap below which a speed multiplier counts as none (`DAT_142eb0c60`).
const EPSILON: f32 = SPRITE_EPSILON;

/// A sprite the data file does not have.
pub(crate) const NO_SPRITE: GenieError =
    GenieError::unsupported("a sprite the data file does not have");

/// Slot 0x68, `new_sprite` (a combat object's `FUN_141217000`, the moving object's
/// `FUN_141158620`, the static object's `FUN_141196580`): the object shows `graphic`. The sprite it
/// shows already stays as it runs; another starts a new instance (`FUN_1413b2ef0`, which draws
/// from the Sprite stream `rng`). A moving object's top speed (`stats`) is the one it reads with
/// `graphic` from then on ([`top_speed`]): the game reads the current sprite at each read. The one
/// writer of a sprite change and of [`MoveStats::speed`], besides a change of the base.
///
/// The combat object's own steps are the renderer's (its overlay sprite `+0xa05`, the hero
/// attachments, the facet of its attached sprites); the moving object's facet (`+0xe`,
/// `FUN_14114d6c0`) is derived where it is read ([`crate::objects::facing`]).
pub(crate) fn new_sprite(
    sprite: &mut Option<ActiveSprite>,
    stats: Option<&mut MoveStats>,
    graphic: Graphic,
    rng: &mut Well512,
    sprite_tuning: SpriteTuning,
) {
    if !sprite.is_some_and(|active_sprite| active_sprite.graphic.id == graphic.id) {
        *sprite = Some(ActiveSprite::start(graphic, rng, sprite_tuning));
    }
    if let Some(stats) = stats {
        stats.show(Some(&graphic));
    }
}

/// The active sprite (the save's `RGE_Active_Animated_Sprite`, memory `+0x24`..`+0x3b`; a static
/// one, the save's `RGE_Active_Sprite`, has only the frame, and the clock's fields stay at rest).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct ActiveSprite {
    /// The graphic.
    pub(crate) graphic: Graphic,
    /// `+0x24`, `frame`.
    pub(crate) frame: Frame,
    /// `+0x28`, `animate_interval`: one frame's time at the last speed.
    pub(crate) interval: Seconds,
    /// `+0x2c`, `animate_last`: the time since the last frame.
    pub(crate) last: Seconds,
    /// `+0x30`, `last_frame`: the frame before the last advance.
    pub(crate) last_frame: Frame,
    /// `+0x34`, `frame_changed`; `+0x35`, `frame_looped`.
    pub(crate) changed: bool,
    /// `+0x35`, `frame_looped`.
    pub(crate) looped: bool,
    /// `+0x36`, `animate_flag`: 0 stopped, 1 playing, 2 in the replay delay.
    pub(crate) animate: AnimateState,
    /// `+0x38`, `lastSpeedValue`.
    pub(crate) speed: Speed,
}

impl ActiveSprite {
    /// A new sprite instance (`FUN_1413b2ef0`): its first frame and interval; a sprite with a
    /// random first frame draws it from the Sprite stream twice (`FUN_1413b3490`, then here). A
    /// static one ([`ActiveSprite::start_static`]) draws it once.
    pub fn start(graphic: Graphic, sprite_rng: &mut Well512, sprite_tuning: SpriteTuning) -> Self {
        if !graphic.animated() {
            return Self::start_static(graphic, sprite_rng);
        }
        let last = graphic.frames - Frame::ONE;
        if graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) {
            // FUN_1413b3490's draw, which the second one overrides
            let _ = sprite_rng.rand15();
        }
        let mut active_sprite = Self {
            graphic,
            frame: graphic.first_frame(),
            interval: graphic.frame_duration,
            last: Seconds::ZERO,
            last_frame: graphic.first_frame(),
            changed: false,
            looped: false,
            animate: AnimateState::Playing,
            speed: Speed::ZERO,
        };
        if f64::from(active_sprite.interval.get()) < 0.001 {
            active_sprite.interval = Seconds::new(sprite_tuning.shortest);
        }
        if graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) {
            let frame_duration = graphic.frame_duration.get();
            if 0.0 < frame_duration {
                let draw = sprite_rng.rand15();
                let elapsed = (draw as i32 as f32
                    * (graphic.frames.as_f32() * frame_duration + graphic.replay_delay.get()))
                    / 32767.0;
                let frame = Frame::of_data((elapsed / frame_duration) as i32 as i16);
                active_sprite.frame = frame;
                if graphic.frames <= frame {
                    active_sprite.frame = last;
                    active_sprite.interval = Seconds::new(elapsed - last.as_f32() * frame_duration);
                }
            } else {
                let draw = sprite_rng.rand15() as i32;
                let frame =
                    Frame::of_data((draw * i32::from(graphic.frames.get()) / 0x7fff) as i16);
                active_sprite.frame = frame.min(last);
            }
            active_sprite.last_frame = active_sprite.frame;
        }
        active_sprite
    }

    /// A static sprite instance (`FUN_1413b3490`): its first frame, or one the Sprite stream
    /// draws for a sprite with a random first frame.
    fn start_static(graphic: Graphic, sprite_rng: &mut Well512) -> Self {
        let last = graphic.frames - Frame::ONE;
        let mut frame = graphic.first_frame();
        if graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME) {
            let draw = sprite_rng.rand15() as i32;
            frame = Frame::of_data(((draw * i32::from(graphic.frames.get())) / 0x7fff) as i16);
            frame = frame.min(last);
        }
        Self::at_rest(graphic, frame)
    }

    /// A static sprite at `frame` (the save's `RGE_Active_Sprite`), its clock at rest.
    #[must_use]
    pub const fn at_rest(graphic: Graphic, frame: Frame) -> Self {
        Self {
            graphic,
            frame,
            interval: Seconds::ZERO,
            last: Seconds::ZERO,
            last_frame: frame,
            changed: false,
            looped: false,
            animate: AnimateState::Stopped,
            speed: Speed::ZERO,
        }
    }

    /// Slot 9 with `0x7fff` (`FUN_1413b4ff0`): back to the first frame, not looped; a static
    /// sprite (`FUN_1413b5030`) only takes the frame.
    pub fn reset_frame(&mut self) {
        if !self.graphic.animated() {
            self.frame = self.graphic.first_frame();
            return;
        }
        self.frame = self.graphic.first_frame();
        self.looped = false;
        self.last_frame = self.frame;
    }

    /// Slot 9 with `frame` other than `0x7fff` (`FUN_1413b4ff0`): the frame and the last frame
    /// take it (and `+0x32`, not held), the loop flag stays; a static sprite (`FUN_1413b5030`)
    /// only takes the frame.
    pub const fn set_frame(&mut self, frame: Frame) {
        self.frame = frame;
        if self.graphic.animated() {
            self.last_frame = frame;
        }
    }

    /// Slot 11 (`FUN_1413b2d70`): the last advance passed `frame` or looped; a static sprite
    /// (`FUN_1413b2d90`) has reached every frame.
    #[must_use]
    pub fn reached(&self, frame: Frame) -> bool {
        if !self.graphic.animated() {
            return true;
        }
        self.changed && (self.looped || (frame <= self.frame && self.last_frame < frame))
    }

    /// The interval of one frame at `speed` (at least 0.001).
    fn interval_at(&self, speed: Speed, sprite_tuning: SpriteTuning) -> Seconds {
        let graphic = &self.graphic;
        let mut frame_duration = graphic.frame_duration.get();
        // COMISS/JA: only an ordered 1e-7 above the multiplier skips it
        if at_most_or_unordered(EPSILON, graphic.speed_multiplier.get()) {
            frame_duration *= graphic.speed_multiplier.get();
        }
        if Speed::ZERO < speed {
            frame_duration /= speed.get();
        }
        Seconds::new(if frame_duration < sprite_tuning.shortest {
            sprite_tuning.shortest
        } else {
            frame_duration
        })
    }

    /// One update of `dt` seconds at the object's animation speed (`FUN_1413b5070`, the object's
    /// `+0x7b8`): an animated sprite's time runs, and its whole intervals advance it as many
    /// frames (backwards for such a sprite); past its last frame it loops, after its replay delay,
    /// or stops on it when played once.
    pub fn update(&mut self, dt: Seconds, speed: Speed, sprite_tuning: SpriteTuning) {
        // a static sprite's update (`FUN_1413b5330`) does nothing
        if !self.graphic.animated() || self.animate == AnimateState::Stopped {
            return;
        }
        self.changed = false;
        self.looped = false;
        let graphic = self.graphic;
        let last = graphic.frames - Frame::ONE;
        let start = if self.frame > last {
            last
        } else if self.frame < Frame::ZERO {
            Frame::ZERO
        } else {
            self.frame
        };
        self.frame = start;
        if !graphic.animated() {
            return;
        }
        let time = dt + self.last;
        self.last = time;
        let interval = if self.speed == speed {
            self.interval
        } else {
            let seconds = self.interval_at(speed, sprite_tuning);
            self.interval = seconds;
            self.speed = speed;
            seconds
        };
        // COMISS/JC at 0x1413b5149: an unordered time or interval returns too
        if below_or_unordered(time.get(), interval.get()) {
            return;
        }
        let forward = !graphic.backwards();
        self.changed = true;
        self.last_frame = start;
        // CVTTSS2SI EDX, MOVSX EAX, DX (0x1413b5177)
        let steps = Frame::of_data(cvttss2si(time.get() / interval.get()) as i16);
        self.last = time - Seconds::new(steps.as_f32() * interval.get());
        if Speed::ZERO < speed || graphic.speed_multiplier == SpeedMultiplier::ZERO {
            let frame = start + if forward { steps } else { -steps };
            self.frame = frame;
            self.looped = frame < Frame::ZERO || last < frame;
        }
        self.interval = self.interval_at(speed, sprite_tuning);
        self.speed = speed;
        let mut stop = self.looped;
        if self.looped {
            let mut frame;
            // COMISS/JBE at 0x1413b521f: an unordered delay loops at once
            if at_most_or_unordered(graphic.replay_delay.get(), 0.0)
                || self.animate == AnimateState::Delayed
            {
                self.animate = AnimateState::Playing;
                frame = self.frame % graphic.frames;
                if frame < Frame::ZERO {
                    frame = frame + graphic.frames;
                }
            } else {
                let mut at = self.frame;
                if at < Frame::ZERO {
                    at = last - at;
                }
                stop = false;
                self.looped = false;
                self.animate = AnimateState::Delayed;
                self.interval = Seconds::new(
                    graphic.replay_delay.get()
                        - (at - graphic.frames).as_f32() * graphic.frame_duration.get(),
                );
                frame = if forward { Frame::ZERO } else { last };
            }
            self.frame = frame;
            if frame == self.last_frame {
                let next = frame + if forward { Frame::ONE } else { -Frame::ONE };
                self.frame = next;
                if next < Frame::ZERO || last < next {
                    stop = self.looped;
                    self.frame = if forward { Frame::ZERO } else { last };
                }
            }
        }
        if stop && graphic.flags.contains(SpriteFlags::ONCE) {
            self.animate = AnimateState::Stopped;
            self.frame = if forward {
                last.max(Frame::ZERO)
            } else {
                Frame::ZERO
            };
        }
    }
}

/// `FUN_1412028a0`: the carrying sprite (`+0x58`) of the master's first trade task (action type
/// 0x6f on master 0x54), None without one.
#[must_use]
pub fn trade_sprite(master: &Master, roles: &Roles) -> Option<SpriteId> {
    master
        .action_part()
        .tasks
        .iter()
        // A trade cart's class (`FUN_14106a940`, decoded 0x13).
        // The trade task whose carrying sprite a trade cart with goods shows (`FUN_1412028a0`: action
        // type 0x6f, `object_id` 0x54).
        .find(|task| {
            task.action_kind == ActionKind::TRADE
                && roles
                    .of_some_master(task.object_id)
                    .contains(MasterRoles::TRADE_TARGET)
        })
        .and_then(|task| task.sprites.carrying)
}

/// Whether an object carries goods (`FUN_141188160`: an amount above 0 in its resource store,
/// `+0x7a0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cargo {
    /// It does.
    Goods,
    /// It carries none.
    Empty,
}

/// `FUN_14106a940`: a trade cart (its master's class 19) that carries goods (`cargo`).
#[must_use]
pub fn loaded_cart(master: &Master, cargo: Cargo, roles: &Roles) -> bool {
    roles
        .of_class(master.class)
        .contains(ClassRoles::TRADE_CART)
        && cargo == Cargo::Goods
}

/// Slot 0x798 of a combat object (`FUN_1412029f0`, types 70 and 80): a loaded trade cart's
/// trade sprite ([`trade_sprite`]), else its terrain look's standing sprite (`+0x8a9`, `+0x960`,
/// [`crate::combat::abilities::Look`]), else its master's (`+0x470`).
#[must_use]
pub fn standing(
    master: &Master,
    look: Option<&Look>,
    cargo: Cargo,
    roles: &Roles,
) -> Option<SpriteId> {
    if loaded_cart(master, cargo, roles) {
        return trade_sprite(master, roles);
    }
    look.map_or_else(|| master.standing_sprite, |look| look.standing)
}

/// Slot 0x7c8 of a combat object (`FUN_1412030b0`): a loaded trade cart's trade sprite, else
/// its terrain look's walking sprite (`+0x968`), else its master's (`+0x748`).
#[must_use]
pub fn walking(
    master: &Master,
    look: Option<&Look>,
    cargo: Cargo,
    roles: &Roles,
) -> Option<SpriteId> {
    if loaded_cart(master, cargo, roles) {
        return trade_sprite(master, roles);
    }
    look.map_or_else(|| master.moving_part().move_sprite, |look| look.walking)
}

/// Slot 0x7d0 of a combat object (`FUN_1412034e0`): its terrain look's running sprite
/// (`+0x978`), else its master's (`+0x750`).
#[must_use]
pub fn running(master: &Master, look: Option<&Look>) -> Option<SpriteId> {
    look.map_or_else(|| master.moving_part().run_sprite, |look| look.running)
}

/// Slot 0x898 of a combat object (`FUN_141202830`): its terrain look's attack sprite (`+0x970`),
/// else its master's (`+0x7f8`, `fight_sprite_id`).
#[must_use]
pub fn attacking(master: &Master, look: Option<&Look>) -> Option<SpriteId> {
    look.map_or_else(|| master.combat_part().attack_sprite, |look| look.attacking)
}

/// Slot 0x7a0 of a combat object (`FUN_1412027d0`): its terrain look's dying sprite (`+0x980`),
/// else its master's (`+0x480`).
#[must_use]
pub fn dying(master: &Master, look: Option<&Look>) -> Option<SpriteId> {
    look.map_or_else(|| master.death_sprite, |look| look.dying)
}

/// Slot 0x7a8 of a combat object (`FUN_141203700`): its terrain look's undead sprite (`+0x988`),
/// else its master's (`+0x488`).
#[must_use]
pub fn undead(master: &Master, look: Option<&Look>) -> Option<SpriteId> {
    look.map_or_else(|| master.undead_sprite, |look| look.undead)
}

/// The sprite a building shows once dead (`RGE_Static_Object::update`, `0x14118b26d`): one
/// whose build points, truncated (`CVTTSS2SI`, then as a short), fall short of its master's
/// build time (`FUN_140ace5e0` with 0) shows its master's destruction sprite (`+0x960`), else
/// its master's death sprite (`+0x480`, not the look's); a built one its slot 0x7a0's
/// ([`dying`]).
#[must_use]
pub fn building_death(
    master: &Master,
    look: Option<&Look>,
    build_points: Seconds,
) -> Option<SpriteId> {
    let points = cvttss2si(build_points.get()).to_le_bytes();
    if i16::from_le_bytes([points[0], points[1]]) < master.creatable_part().build_time.get() {
        let destruction = master
            .building
            .as_ref()
            .and_then(|building_master| building_master.destruction_sprite);
        return destruction.or(master.death_sprite);
    }
    dying(master, look)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::gathering::hunt::tests::walker;
    use crate::movement::Speed;
    use crate::objects::FacetCount;
    use crate::objects::masters::ActionPart;
    use crate::objects::masters::CombatPart;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::MasterKind;
    use crate::objects::masters::MovingPart;
    use crate::objects::masters::TaskSprites;
    #[cfg(test)]
    use crate::objects::masters::{BuildingMaster, Task};
    use crate::test_ids::master_id;
    use crate::testing::class;
    use crate::testing::roles;
    use crate::testing::tuning;
    use crate::world::WholeSeconds;

    /// Build 158041's Lumberjack Female (Attack), graphic 1239: 45 frames of 0.0444 s, animated.
    const CHOP: Graphic = Graphic {
        id: SpriteId::named(1239),
        frames: Frame::of_data(45),
        facets: FacetCount::of_data(8),
        speed_multiplier: SpeedMultiplier::ZERO,
        frame_duration: Seconds::new(0.044_444_446),
        replay_delay: Seconds::ZERO,
        flags: SpriteFlags::ANIMATED.union(SpriteFlags::TURNS),
    };

    #[test]
    fn the_chop_reaches_its_frame_22_after_second_of_20_ms_updates() {
        // MarvinBlue/426048731's 9181 from update 1008 (the tick oracle's trace): frame 0 after
        // the first update (time 0.02), frame 1 after the third (time 0.0155...)
        let mut active_sprite = ActiveSprite::start(CHOP, &mut Well512::default(), tuning().sprite);
        active_sprite.update(Seconds::new(0.02), Speed::new(1.0), tuning().sprite);
        assert_eq!(
            (active_sprite.frame, active_sprite.changed),
            (Frame::of_data(0), false)
        );
        active_sprite.update(Seconds::new(0.02), Speed::new(1.0), tuning().sprite);
        active_sprite.update(Seconds::new(0.02), Speed::new(1.0), tuning().sprite);
        assert_eq!(active_sprite.frame, Frame::of_data(1));
        assert!((active_sprite.last - Seconds::new(0.015_555)).get().abs() < 1e-5);
        let mut updates = 3;
        while !active_sprite.reached(Frame::of_data(22)) {
            active_sprite.update(Seconds::new(0.02), Speed::new(1.0), tuning().sprite);
            updates += 1;
        }
        // update 1057, the 49th
        assert_eq!((updates, active_sprite.frame), (49, Frame::of_data(22)));
    }

    /// Slot 0x68: a new sprite starts its instance (a random first frame draws), the one shown
    /// keeps its frame; a moving object's top speed is its base times the shown sprite's factor
    /// above 0 after every call, a static object has none.
    #[test]
    fn new_sprite_starts_its_instance_and_the_top_speed_follows_it() {
        let walking = Graphic {
            id: SpriteId::named(1),
            speed_multiplier: SpeedMultiplier::of_data(1.25),
            flags: SpriteFlags::ANIMATED | SpriteFlags::RANDOM_FIRST_FRAME,
            ..CHOP
        };
        let mut stats = walker().stats;
        stats.base = Speed::new(0.8);
        stats.speed = Speed::ZERO;
        let mut rng = Well512::default();
        let mut sprite = Some(ActiveSprite::start(CHOP, &mut rng, tuning().sprite));
        let before = rng.clone();
        new_sprite(
            &mut sprite,
            Some(&mut stats),
            walking,
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(
            sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(1))
        );
        assert_ne!(rng, before);
        assert_eq!(stats.speed, Speed::new(1.0));
        // the sprite shown stays as it runs, and the speed is still the one it reads with it
        if let Some(active_sprite) = sprite.as_mut() {
            active_sprite.frame = Frame::of_data(3);
        }
        stats.speed = Speed::new(0.5);
        let drawn = rng.clone();
        new_sprite(
            &mut sprite,
            Some(&mut stats),
            walking,
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(
            (
                sprite.map(|active_sprite| active_sprite.frame),
                &rng,
                stats.speed
            ),
            (Some(Frame::of_data(3)), &drawn, Speed::new(1.0))
        );
        // a factor of 0 or NaN reads the base alone
        new_sprite(
            &mut sprite,
            Some(&mut stats),
            CHOP,
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(stats.speed, Speed::new(0.8));
        let nan = Graphic {
            id: SpriteId::named(2),
            speed_multiplier: SpeedMultiplier::of_data(f32::NAN),
            ..CHOP
        };
        new_sprite(
            &mut sprite,
            Some(&mut stats),
            nan,
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(stats.speed, Speed::new(0.8));
        // an object that does not move shows it alone
        let mut still = None;
        new_sprite(&mut still, None, walking, &mut rng, tuning().sprite);
        assert_eq!(
            still.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(1))
        );
    }

    /// `FUN_1413b5070`'s compares are COMISS: a NaN speed multiplier still multiplies (JA at
    /// 0x1413b5112 skips only for an ordered 1e-7 above it), and the NaN interval then returns
    /// before a step (JC at 0x1413b5149).
    #[test]
    fn nan_speed_multiplier_holds_the_frame() {
        let graphic = Graphic {
            speed_multiplier: SpeedMultiplier::of_data(f32::NAN),
            ..CHOP
        };
        let mut active_sprite =
            ActiveSprite::start(graphic, &mut Well512::default(), tuning().sprite);
        active_sprite.update(Seconds::new(1.0), Speed::new(1.0), tuning().sprite);
        assert_eq!(
            (active_sprite.frame, active_sprite.changed),
            (Frame::of_data(0), false)
        );
    }

    /// `CVTTSS2SI EDX` then `MOVSX EAX, DX` (0x1413b5177): a quotient past the i32 range steps
    /// 0 frames (0x80000000's low word), where a saturating cast would step back one.
    #[test]
    fn step_count_past_the_i32_range_steps_no_frame() {
        let mut active_sprite = ActiveSprite::start(CHOP, &mut Well512::default(), tuning().sprite);
        active_sprite.update(Seconds::new(3.0e8), Speed::new(1.0), tuning().sprite);
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.changed,
                active_sprite.looped
            ),
            (Frame::of_data(0), true, false)
        );
    }

    /// `FUN_1413b5070` on four frames of 0.25 s: a frame outside the sprite starts from its
    /// nearer end; past its end a sprite with a replay delay waits it out (state 2, the interval
    /// the delay less the frames beyond the end) from its first frame, a backwards one from its
    /// last; without one it loops by the frame count (a backwards one from below 0 too); a
    /// sprite played once stops on its end; the frame it left is never shown again at once, so
    /// a one-frame loop ends looped.
    #[test]
    fn sprite_past_its_end_waits_its_delay_loops_or_stops() {
        let four = |flags, replay_delay| Graphic {
            id: SpriteId::named(1),
            frames: Frame::of_data(4),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ZERO,
            frame_duration: Seconds::new(0.25),
            replay_delay,
            flags,
        };
        let after = |graphic: Graphic, frame: Option<i16>, dt: f32| {
            let mut active_sprite =
                ActiveSprite::start(graphic, &mut Well512::default(), tuning().sprite);
            if let Some(wanted) = frame {
                active_sprite.frame = Frame::of_data(wanted);
            }
            active_sprite.update(Seconds::new(dt), Speed::new(1.0), tuning().sprite);
            active_sprite
        };
        assert_eq!(
            after(four(SpriteFlags::ANIMATED, Seconds::ZERO), Some(9), 0.1).frame,
            Frame::of_data(3)
        );
        assert_eq!(
            after(four(SpriteFlags::ANIMATED, Seconds::ZERO), Some(-2), 0.1).frame,
            Frame::of_data(0)
        );
        let active_sprite = after(four(SpriteFlags::ANIMATED, Seconds::new(0.5)), None, 1.0);
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.animate,
                active_sprite.interval,
                active_sprite.looped
            ),
            (
                Frame::of_data(1),
                AnimateState::of_data(2),
                Seconds::new(0.5),
                false
            )
        );
        let active_sprite = after(
            four(
                SpriteFlags::ANIMATED | SpriteFlags::BACKWARDS,
                Seconds::new(0.5),
            ),
            None,
            1.0,
        );
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.animate,
                active_sprite.interval
            ),
            (
                Frame::of_data(2),
                AnimateState::of_data(2),
                Seconds::new(0.5)
            )
        );
        let active_sprite = after(
            four(
                SpriteFlags::ANIMATED | SpriteFlags::BACKWARDS,
                Seconds::ZERO,
            ),
            None,
            1.25,
        );
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.animate,
                active_sprite.looped
            ),
            (Frame::of_data(2), AnimateState::of_data(1), true)
        );
        let active_sprite = after(
            four(SpriteFlags::ANIMATED | SpriteFlags::ONCE, Seconds::ZERO),
            None,
            1.0,
        );
        assert_eq!(
            (active_sprite.frame, active_sprite.animate),
            (Frame::of_data(3), AnimateState::of_data(0))
        );
        let active_sprite = after(
            four(
                SpriteFlags::ANIMATED | SpriteFlags::ONCE | SpriteFlags::BACKWARDS,
                Seconds::ZERO,
            ),
            None,
            1.0,
        );
        assert_eq!(
            (active_sprite.frame, active_sprite.animate),
            (Frame::of_data(0), AnimateState::of_data(0))
        );
        let one = Graphic {
            frames: Frame::of_data(1),
            ..four(SpriteFlags::ANIMATED, Seconds::ZERO)
        };
        let active_sprite = after(one, None, 0.25);
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.looped,
                active_sprite.animate
            ),
            (Frame::of_data(0), true, AnimateState::of_data(1))
        );
    }

    /// The replay delay's test is COMISS/JBE (0x1413b521f): a NaN delay loops at once.
    #[test]
    fn nan_replay_delay_loops_at_once() {
        let graphic = Graphic {
            frames: Frame::of_data(2),
            replay_delay: Seconds::new(f32::NAN),
            ..CHOP
        };
        let mut active_sprite =
            ActiveSprite::start(graphic, &mut Well512::default(), tuning().sprite);
        active_sprite.update(Seconds::new(0.1), Speed::new(1.0), tuning().sprite);
        assert_eq!(active_sprite.animate, AnimateState::of_data(1));
    }

    #[test]
    fn static_sprite_stays_on_its_frame_and_has_reached_every_frame() {
        // build 158041's town center sprite 3241 does not animate (sequence bit 0 clear)
        let still = Graphic {
            id: SpriteId::named(3241),
            frames: Frame::of_data(1),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ZERO,
            frame_duration: Seconds::ZERO,
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::NONE,
        };
        let mut rng = Well512::default();
        let mut active_sprite = ActiveSprite::start(still, &mut rng, tuning().sprite);
        assert_eq!(rng, Well512::default());
        active_sprite.update(Seconds::new(0.5), Speed::new(1.0), tuning().sprite);
        assert_eq!(
            (
                active_sprite.frame,
                active_sprite.changed,
                active_sprite.looped
            ),
            (Frame::of_data(0), false, false)
        );
        assert!(
            active_sprite.reached(Frame::of_data(0)) && active_sprite.reached(Frame::of_data(7))
        );
        // a random first frame draws once, not twice as an animated one does
        let mut once = Well512::default();
        let _ = once.rand15();
        let mut rng = Well512::default();
        let _ = ActiveSprite::start(
            Graphic {
                flags: SpriteFlags::RANDOM_FIRST_FRAME,
                ..still
            },
            &mut rng,
            tuning().sprite,
        );
        assert_eq!(rng, once);
    }

    /// The barracks (12) of build 158041: standing 2575, dying 2573, its foundation's
    /// destruction 389, a build time of 50; walking, running and attack sprites for the slots.
    fn barracks() -> Master {
        Master {
            id: master_id(12),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            standing_sprite: Some(SpriteId::named(2575)),
            death_sprite: Some(SpriteId::named(2573)),
            undead_sprite: None,
            building: Some(BuildingMaster {
                destruction_sprite: Some(SpriteId::named(389)),
                construction_sprite: None,
                ..BuildingMaster::default()
            }),
            creatable: Some(CreatablePart {
                build_time: WholeSeconds::of_data(50),
                ..CreatablePart::default()
            }),
            combat: Some(CombatPart {
                attack_sprite: Some(SpriteId::named(12)),
                ..CombatPart::default()
            }),
            moving: Some(MovingPart {
                move_sprite: Some(SpriteId::named(10)),
                run_sprite: Some(SpriteId::named(11)),
                ..MovingPart::default()
            }),
            ..Master::default()
        }
    }

    fn look() -> Look {
        Look {
            standing: Some(SpriteId::named(1)),
            walking: Some(SpriteId::named(2)),
            attacking: Some(SpriteId::named(3)),
            running: None,
            dying: Some(SpriteId::named(5)),
            undead: Some(SpriteId::named(6)),
        }
    }

    #[test]
    fn combat_object_slots_show_its_look_else_its_master_s() {
        let master = barracks();
        assert_eq!(
            standing(&master, None, Cargo::Empty, &roles()),
            Some(SpriteId::named(2575))
        );
        assert_eq!(
            walking(&master, None, Cargo::Empty, &roles()),
            Some(SpriteId::named(10))
        );
        assert_eq!(running(&master, None), Some(SpriteId::named(11)));
        assert_eq!(attacking(&master, None), Some(SpriteId::named(12)));
        assert_eq!(dying(&master, None), Some(SpriteId::named(2573)));
        assert_eq!(undead(&master, None), None);
        let look = look();
        assert_eq!(
            standing(&master, Some(&look), Cargo::Empty, &roles()),
            Some(SpriteId::named(1))
        );
        assert_eq!(
            walking(&master, Some(&look), Cargo::Empty, &roles()),
            Some(SpriteId::named(2))
        );
        // the look's null sprite is its own, not the master's
        assert_eq!(running(&master, Some(&look)), None);
        assert_eq!(attacking(&master, Some(&look)), Some(SpriteId::named(3)));
        assert_eq!(dying(&master, Some(&look)), Some(SpriteId::named(5)));
        assert_eq!(undead(&master, Some(&look)), Some(SpriteId::named(6)));
    }

    #[test]
    fn loaded_trade_cart_shows_its_trade_task_carrying_sprite() {
        let trade = Task {
            action_kind: ActionKind::TRADE,
            object_id: Some(master_id(0x54)),
            sprites: TaskSprites::of_data([1, 2, 3, 4]),
            ..Task::default()
        };
        let other = Task {
            object_id: Some(master_id(0x55)),
            sprites: TaskSprites::of_data([9; 4]),
            ..trade
        };
        let cart = Master {
            class: class::TRADE_CART,
            standing_sprite: Some(SpriteId::named(20)),
            action: Some(ActionPart {
                tasks: vec![other, trade],
                ..ActionPart::default()
            }),
            moving: Some(MovingPart {
                move_sprite: Some(SpriteId::named(21)),
                ..MovingPart::default()
            }),
            ..Master::default()
        };
        assert_eq!(
            standing(&cart, None, Cargo::Goods, &roles()),
            Some(SpriteId::named(4))
        );
        assert_eq!(
            walking(&cart, Some(&look()), Cargo::Goods, &roles()),
            Some(SpriteId::named(4))
        );
        assert_eq!(
            standing(&cart, None, Cargo::Empty, &roles()),
            Some(SpriteId::named(20))
        );
        assert_eq!(
            walking(&cart, None, Cargo::Empty, &roles()),
            Some(SpriteId::named(21))
        );
        // without a trade task, none
        let bare = Master {
            action: Some(ActionPart {
                tasks: Vec::new(),
                ..cart.action.clone().unwrap_or_default()
            }),
            ..cart
        };
        assert_eq!(standing(&bare, None, Cargo::Goods, &roles()), None);
        // another class's goods do not count
        let villager = Master {
            class: class::VILLAGER,
            ..bare
        };
        assert_eq!(
            standing(&villager, None, Cargo::Goods, &roles()),
            Some(SpriteId::named(20))
        );
    }

    /// records/425984813: a barracks that dies a foundation plays its destruction sprite (389),
    /// not its death sprite (2573).
    #[test]
    fn building_that_dies_before_it_is_built_shows_its_destruction_sprite() {
        let master = barracks();
        assert_eq!(
            building_death(&master, None, Seconds::new(12.5)),
            Some(SpriteId::named(389))
        );
        assert_eq!(
            building_death(&master, None, Seconds::new(49.9)),
            Some(SpriteId::named(389))
        );
        assert_eq!(
            building_death(&master, None, Seconds::new(50.0)),
            Some(SpriteId::named(2573))
        );
        assert_eq!(
            building_death(&master, Some(&look()), Seconds::new(50.0)),
            Some(SpriteId::named(5))
        );
        // a NaN is the integer indefinite, whose short is 0
        assert_eq!(
            building_death(&master, None, Seconds::new(f32::NAN)),
            Some(SpriteId::named(389))
        );
        // the short wraps: 65586 build points are 50
        assert_eq!(
            building_death(&master, None, Seconds::new(65586.0)),
            Some(SpriteId::named(2573))
        );
        // without a destruction sprite its master's death sprite, never the look's
        let plain = Master {
            building: None,
            ..barracks()
        };
        assert_eq!(
            building_death(&plain, Some(&look()), Seconds::ZERO),
            Some(SpriteId::named(2573))
        );
    }
}
