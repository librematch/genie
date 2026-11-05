//! Slot 0x90, a unit's or a building's change of master: its actions cleared or kept, its
//! hit points, sight, place, sprites, powerups and volleys taken from the new master.

use crate::combat::powerup::reset_powerup_values;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::math::rng::Well512;
use crate::movement::MoveState;
use crate::movement::MoveStats;
use crate::movement::move_to::MoveTo;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectState;
use crate::objects::actions::Action;
use crate::objects::layers::Combat;
use crate::objects::layers::MotionMut;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::sprite::Graphic;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::objects::unit::Unit;
use crate::orders::damage_share;
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::vision::SightLayer;
use crate::world::Seconds;
use crate::world::World;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;
use crate::world::layer::generated::Tuning;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// What a master change does with the unit's actions (slot 0x90's third argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActionsOnChange {
    /// 0, a command's or a building's change: the action list is cleared (`FUN_140a98620`), and
    /// the unit takes its new master's standing sprite.
    Clear,
    /// 1, a hit point transform's (`FUN_141204450`): the list stays, each action taking the new
    /// master's sprites (`FUN_140a979b0`, [`World::retask_actions`]), and the unit takes the new
    /// standing sprite only without an action (slot 0x430, `FUN_14106d0e0`).
    Keep,
}

/// What the unit shows after a master change ([`switch_master`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShownOnChange {
    /// The new master's standing sprite (slot 0x68).
    Standing,
    /// The sprite it shows (its actions kept, [`ActionsOnChange::Keep`]).
    Current,
}

impl World {
    /// A building's slot 0x90 (`FUN_1411e6d90`, with `actions` its third argument): its master
    /// change ([`World::change_master`]), then in state 2 its build points (`+0xc50`) take the
    /// new master's build time (its first train location's time, 0 without one). The slice holds
    /// them in the building's construction record.
    pub(crate) fn transform_building(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        new: MasterId,
        actions: ActionsOnChange,
    ) -> GenieResult<()> {
        self.change_master(player, idx, new, actions)?;
        let build_time = self.master(player, new)?.creatable_part().build_time;
        let unit = self.players.live_mut(player, idx)?;
        if unit.life.state == ObjectState::Alive
            && let Some(construction) = self.parts.building_mut(unit.id)?.construction.as_mut()
        {
            construction.progress = Seconds::of_master(build_time);
        }
        Ok(())
    }

    /// Slot 0x90 (`FUN_141225b70`, with `actions` its third argument): the unit changes its
    /// master. Its action list is cleared, or each action takes the new master's sprites
    /// ([`ActionsOnChange`]); in a state below 3 (`FUN_1411a6a70`) the old master's storage
    /// leaves its player's attributes and the new one's comes in (`FUN_1411a56c0`, flag 2), its
    /// hit points keep their share of the new master's (whole, at least 1), its sight goes and
    /// the new master's comes (slots 0x1d8, 0x1d0), its place and obstruction record take the
    /// new master's (`FUN_141055b40`, `FUN_14118c540`) and it moves with the new master's
    /// values; its powerups go ([`reset_powerup_values`], and a sight powerup through its
    /// sight) and in state 2 its volleys (`+0xa00`) are the new master's volley fire; it takes
    /// the new master's standing sprite (slot 0x68) unless it keeps actions. Its per-master
    /// counts, facet, class registry (`DAT_143f69170`) and particles are not held.
    pub(crate) fn change_master(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        new: MasterId,
        actions: ActionsOnChange,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let old = unit.master(self.master_tables(), player)?.clone();
        let next = self.master(player, new)?.clone();
        let shown = match actions {
            ActionsOnChange::Clear => {
                self.clear_actions(player, idx)?;
                ShownOnChange::Standing
            }
            ActionsOnChange::Keep => {
                self.retask_actions(player, idx, &old, &next)?;
                let id = self.players.live(player, idx)?.id;
                if self.parts.actions_of(id).list.is_empty() {
                    ShownOnChange::Standing
                } else {
                    ShownOnChange::Current
                }
            }
        };
        let world = SwitchWorld {
            spatial: &mut self.spatial,
            pathing: &mut self.pathing,
            graphics: &self.model.graphics,
            sprite: self.streams.get_mut(Stream::Sprite),
        };
        let unit = self.players.live_mut(player, idx)?;
        let (combat, motion) = self.parts.combat_and_motion_mut(unit.id)?;
        let switched = switch_master(
            (unit, combat, motion),
            &old,
            &next,
            world,
            shown,
            &self.model.roles,
            &self.model.tuning,
        )?;
        let owner = self.players.player_mut(player)?;
        for &(kind, amount) in &switched.leaving {
            owner.add_attribute(kind, amount)?;
        }
        if switched.promoted {
            // the tracker's promotion (FUN_14104fb20): last in its new key master's list and
            // class's
            let id = self.players.live(player, idx)?.id;
            self.track_promote(player, id, old.id, new)?;
        }
        let owner = self.players.player_mut(player)?;
        for &(kind, amount) in &switched.coming {
            owner.add_attribute(kind, amount)?;
        }
        Ok(())
    }

    /// `FUN_140a979b0` with the new master `next` and no tasks (slot 0x90 with 1): each action of
    /// object `idx`'s list, the head first, takes `next`'s sprites through its slot 0x50, its
    /// unit still of `old`: an Attack's ([`Arms::take_master`], `FUN_140a7f800`), a `MoveTo`'s
    /// ([`MoveToAction::take_master`], `FUN_140aa1fb0`), whose slot 0x60 (`0x140aa1f90`) then
    /// shows its sprite in state 4 (the unit's slot 0x68; a null sprite shows `old`'s standing
    /// one, as the `MoveTo`'s state 4 does). Every other action's slot 0x50 is empty
    /// (`0x140a782d0`).
    ///
    /// # Errors
    ///
    /// An object the world does not hold live.
    ///
    /// [`Arms::take_master`]: crate::combat::attack::Arms::take_master
    /// [`MoveToAction::take_master`]: crate::movement::move_to::MoveToAction::take_master
    pub(crate) fn retask_actions(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        old: &Master,
        next: &Master,
    ) -> GenieResult<()> {
        let id = self.players.live(player, idx)?.id;
        let graphics = &self.model.graphics;
        let standing = graphics.resolve(old.standing_sprite);
        let mut shows = Vec::new();
        for action in &mut self.parts.actions.of_mut(id)?.list {
            match action {
                Action::Attack(attack) => attack.arms.take_master(next, None, graphics),
                Action::MoveTo(move_to) => {
                    move_to.take_master(old, next, None, graphics);
                    if move_to.state == MoveTo::Moving {
                        shows.push(move_to.sprite.or(standing));
                    }
                }
                _ => {}
            }
        }
        let sprite_tuning = self.model.tuning.sprite;
        let unit = self.players.live_mut(player, idx)?;
        for graphic in shows.into_iter().flatten() {
            show_on_change(
                &mut unit.sprite,
                self.parts.motion.move_stats_mut(id),
                graphic,
                self.streams.get_mut(Stream::Sprite),
                sprite_tuning,
            );
        }
        Ok(())
    }
}

/// Slot 0x68 in a master change (`FUN_141217000`) of a unit showing `sprite` with the movement
/// `stats`: the unit shows `graphic` ([`new_sprite`]). The slice holds sprites from an attack on:
/// a unit whose sprite it does not hold keeps none, but the new instance still draws from the
/// Sprite stream `rng`.
fn show_on_change(
    sprite: &mut Option<ActiveSprite>,
    stats: Option<&mut MoveStats>,
    graphic: Graphic,
    rng: &mut Well512,
    sprite_tuning: SpriteTuning,
) {
    if sprite.is_some() {
        new_sprite(sprite, stats, graphic, rng, sprite_tuning);
    } else {
        let _ = ActiveSprite::start(graphic, rng, sprite_tuning);
    }
}

/// What a master change ([`switch_master`]) changes of the world besides its unit: the place and
/// its records, the pathing, and the sprites with the Sprite stream.
pub(crate) struct SwitchWorld<'world> {
    pub(crate) spatial: &'world mut Spatial,
    pub(crate) pathing: &'world mut Pathing,
    pub(crate) graphics: &'world Graphics,
    pub(crate) sprite: &'world mut Well512,
}

/// What [`switch_master`] leaves to its owner, in the game's order: the old master's storage
/// leaving its player's attributes, the tracker's promotion (`FUN_14104fb20`), the new master's
/// storage coming in.
#[derive(Debug, Default)]
pub(crate) struct Switched {
    pub(crate) leaving: Vec<(AttributeId, Amount)>,
    pub(crate) promoted: bool,
    pub(crate) coming: Vec<(AttributeId, Amount)>,
}

/// Slot 0x90's unit part (`FUN_141225b70` with 0) of a master change from `old` to `next` of `unit`
/// with its combat layer `combat` and its motion layer `motion`, its action list cleared by the
/// caller: in a state below 3 (`FUN_1411a6a70`) the old master's storage leaves its player's
/// attributes and the new one's comes in (`FUN_1411a56c0`, flag 2: the owner's, [`Switched`]), its
/// hit points keep their share of the new master's (whole, at least 1), its sight goes and the new
/// master's comes (slots 0x1d8, 0x1d0), its place and obstruction record take the new master's
/// (`FUN_141055b40`, `FUN_14118c540`) and it moves with the new master's values; it takes the new
/// master's standing sprite (slot 0x68). Then its powerups go ([`reset_powerup_values`], and a
/// sight powerup through its sight) and in state 2 its volleys (`+0xa00`) are the new master's
/// volley fire. Its per-master counts, facet, class registry (`DAT_143f69170`) and particles are
/// not held.
///
/// # Errors
///
/// A change of its damage sprites, a moving unit's new master that does not move and a standing
/// sprite the data file lacks.
pub(crate) fn switch_master(
    (unit, combat, mut motion): (&mut Unit, &mut Combat, MotionMut<'_>),
    old: &Master,
    next: &Master,
    world: SwitchWorld<'_>,
    shown: ShownOnChange,
    roles: &Roles,
    tuning: &Tuning,
) -> GenieResult<Switched> {
    let (sprite_tuning, visibility_tuning) = (tuning.sprite, tuning.visibility);
    let mut switched = Switched::default();
    let new = next.id;
    if unit.life.state < ObjectState::Dying {
        for &(kind, amount, flag) in &old.storage {
            if let Some(kind) = kind
                && matches!(flag, 2 | 16 | 32)
            {
                switched.leaving.push((kind, -amount));
            }
        }
        let new_hp = HitPoints::of_master(next.hit_points).get();
        let share = unit.life.hp.get() / HitPoints::of_master(old.hit_points).get() * new_hp;
        let hp = if share > new_hp {
            new_hp
        } else if 1.0 <= share {
            share
        } else {
            1.0
        };
        let old_hp = unit.life.hp;
        unit.life.hp = HitPoints::new(f32::from(hp as i32 as i16));
        // FUN_1411859a0: the damage share picks the damage sprites; one that stays keeps them
        // (the object's `+0xd`, not held, taken to be the old master's share)
        if (old.has_damage_sprites() || next.has_damage_sprites())
            && (damage_share(old_hp, old.hit_points, old.class, roles)?
                != damage_share(unit.life.hp, next.hit_points, next.class, roles)?)
        {
            return Err(GenieError::unsupported(
                "a master change that changes the damage sprites (FUN_1411859a0)",
            ));
        }
        unit.life.master_id = new;
        let moving = MoveState::of_mover(motion.mover());
        if let Some(mover) = motion.mover_mut() {
            mover.stats = next.move_stats.ok_or(GenieError::unsupported(
                "a moving unit's new master does not move",
            ))?;
            // the top speed the unit reads with the sprite it shows, from the new master's base
            if let Some(shown) = unit
                .sprite
                .as_ref()
                .map(|active_sprite| &active_sprite.graphic)
            {
                mover.stats.show(Some(shown));
            }
        }
        let id = unit.id;
        if let Some(mut placed) = world.spatial.get(id).copied() {
            placed.radius = next.radius;
            placed.class = next.class;
            placed.master = new;
            placed.obstruction_kind = next.obstruction_kind;
            placed.buildable_on = next.buildable_on;
            placed.tall = next.is_tall();
            placed.moves = next.moves();
            if unit.life.inside.is_some() {
                // inside another (`+0x2f0`) the unit has no tile (`+0x2e8` 0: no sight slots) and
                // is in no list nor the obstruction manager: its record takes the new master's
                // values for its way out
                world.spatial.set_inside_record(id, placed);
                if let Some(visibility) = world.spatial.sight.as_mut() {
                    visibility.seer_master(id, new, next.line_of_sight);
                }
            } else {
                if let Some(obstruction) = placed.obstruction.as_mut() {
                    obstruction.kind = next.obstruction_kind;
                    obstruction.class = next.obstruction_class;
                }
                world.pathing.removed(id);
                world.spatial.sight_master(
                    id,
                    next.radius,
                    next.class,
                    new,
                    next.line_of_sight,
                    SightLayer {
                        roles,
                        visibility_tuning,
                    },
                )?;
                world.spatial.change_master(id, placed);
                world.pathing.moved(id, &placed, moving);
            }
        }
        // slot 0x68 with the new master's standing sprite (`+0x470`, FUN_141217000): a new
        // instance unless the unit shows it already. The slice holds sprites from an attack
        // on; a unit whose sprite it does not hold shows its old master's. Without the
        // graphics (no data file) the Sprite stream is not modelled. A unit that keeps its
        // actions and has one keeps its sprite (`FUN_1411a6a70`'s slot 0x430 test).
        if shown == ShownOnChange::Standing
            && !world.graphics.is_empty()
            && old.standing_sprite != next.standing_sprite
        {
            let graphic = next
                .standing_sprite
                .and_then(|sprite_id| world.graphics.get(sprite_id).copied())
                .ok_or(GenieError::unsupported(
                    "a new master's standing sprite the data file does not have",
                ))?;
            show_on_change(
                &mut unit.sprite,
                motion.move_stats_mut(),
                graphic,
                world.sprite,
                sprite_tuning,
            );
        }
        switched.promoted = true;
        for &(kind, amount, flag) in &next.storage {
            if let Some(kind) = kind
                && flag == 2
            {
                switched.coming.push((kind, amount));
            }
        }
    }
    reset_powerup_values(combat, tuning.powerup);
    world.spatial.take_sight_powerup(
        unit.id,
        unit.life.inside,
        SightLayer {
            roles,
            visibility_tuning,
        },
    )?;
    // in state 2 the volleys (`+0xa00`) are the new master's volley fire, the shares of the
    // units inside dropped
    if unit.life.state == ObjectState::Alive {
        combat.firepower = next.creatable_part().volley_fire;
    }
    Ok(switched)
}
