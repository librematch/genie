//! An object's upgrade to another master: slot 0x98 (`FUN_141201a60` for a combat object,
//! `FUN_1411c4660` for a building), down the classes' overrides (`FUN_14107cb70`,
//! `FUN_14106a530`, `FUN_14114cb80`) to the static object's `FUN_141186580`, as the upgrade of
//! its master runs it (`FUN_141657d10`, [`World::upgrade_objects`]); and the damage sprites'
//! check (`FUN_1411859a0`), which the static part runs around the hit points' change.

mod damage;

use crate::GenieResult;
use crate::combat::Charge;
use crate::math::rng::Stream;
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::lifecycle::death_parts::count_class;
use crate::objects::masters::ClassId;
use crate::objects::masters::HpPercent;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::player_master;
use crate::objects::sprite::NO_SPRITE;
use crate::objects::sprite::new_sprite;
use crate::objects::task_change::{Looks, take_tasks};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::technology::upgrade::damage::{damage_share_of, damage_sprites};
use crate::unit_ai::UnitAi;
use crate::unit_ai::unit_ai_kind;
use crate::vision::SightCentre;
use crate::vision::SightLayer;
use crate::vision::SightShape;
use crate::vision::SightTiles;
use crate::vision::visibility::LosSight;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::parts::Part;
use crate::world::slots::PlayerSlotsExt;
use std::mem::take;

/// A master's hit points this near 0 give no share (`ABS(x) <= 1.1920929e-07`).
const NO_HIT_POINTS: f32 = f32::EPSILON;

/// Slot 0xa8 (`FUN_141201670` for a combat object and a building; the base's `FUN_14106a520`
/// answers 1): whether an object of a master of type `kind` in state `state` takes an upgrade
/// (below state 4) rather than a master of its own.
#[must_use]
pub fn upgradable(kind: MasterKind, state: ObjectState) -> bool {
    kind < MasterKind::COMBAT || state < ObjectState::DyingSprite
}

/// `FUN_1415c6aa0` on a player's masters `new` and `old` (by id): the unit's new class needs
/// another unit AI (`FUN_141215a90`) when the classes differ, but for a change that keeps it
/// ([`keeps_unit_ai`]).
fn class_changes(new: Option<&Master>, old: Option<&Master>, roles: &Roles) -> bool {
    match (new, old) {
        (Some(new_master), Some(old_master)) => {
            new_master.class != old_master.class
                && !keeps_unit_ai(roles, new_master.class, old_master.class)
        }
        _ => false,
    }
}

/// Whether a unit whose class changes from `old` to `new` keeps its unit AI (`FUN_1415c6aa0`'s
/// pairs, the new class first: 12 from 47, 18 from 43 and 43 from 18): a class
/// [`ClassRoles::KEEPS_UNIT_AI_TO`] from one [`ClassRoles::KEEPS_UNIT_AI_FROM`], or two classes
/// that are both [`ClassRoles::SHARES_UNIT_AI`].
fn keeps_unit_ai(roles: &Roles, new: ClassId, old: ClassId) -> bool {
    let (new, old) = (roles.of_class(new), roles.of_class(old));
    (new.contains(ClassRoles::KEEPS_UNIT_AI_TO) && old.contains(ClassRoles::KEEPS_UNIT_AI_FROM))
        || (new.contains(ClassRoles::SHARES_UNIT_AI) && old.contains(ClassRoles::SHARES_UNIT_AI))
}

/// Whether an upgraded object keeps its master (the upgrade's modes −1 and 3, whose master then
/// copies the new one) or takes the new master.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum UpgradeMaster {
    /// Its master stays.
    Kept,
    /// The new master replaces it.
    Replaced,
}

impl World {
    /// Slot 0x98 of player `player_id`'s object at `place`, whose master `old` upgrades to `new`;
    /// with [`UpgradeMaster::Kept`] (the upgrade's modes −1 and 3, whose master then copies `new`)
    /// its master stays. In the game's order, outermost class first:
    ///
    /// - a building (`FUN_1411c4660`) showing `old`'s construction sprite shows `new`'s (slot
    ///   0x68); its snow decal swaps between a push and a pop of the Object stream (drawn);
    /// - a combat object (`FUN_141201a60`) takes the change of the missiles per volley
    ///   (`+0xa00`); its garrison decal swaps (drawn);
    /// - a combat object (`FUN_14107cb70`) showing `old`'s attack sprite shows `new`'s; slot
    ///   0x8a8 rebuilds what the object keeps of `new`'s tasks, which the slice reads from the
    ///   master;
    /// - an action object (`FUN_14106a530`): each action takes `new`'s task of its type, class
    ///   and target (`FUN_140a97970`, [`World::actions_take_master`]); with [`UpgradeMaster::Replaced`] a top
    ///   action of a type `new` has no task for stops the object (slot 0x148);
    /// - a moving object (`FUN_14114cb80`) showing `old`'s walking or running sprite shows
    ///   `new`'s;
    /// - the static object (`FUN_141186580`): with [`UpgradeMaster::Replaced`] a combat object leaves its class
    ///   counters ([`crate::objects::lifecycle::death_parts::count_class`] with −1) and a villager's load goes to its
    ///   player (slot 0xf8 with 1 and 1, the foods as food; the statistics are not
    ///   simulation); on the map in state 2 its sight goes and comes with `new`'s line of sight
    ///   (slots 0x1d8 and 0x1d0); its hit points take their share of `new`'s (`old`'s through
    ///   the damage sprites at full hit points first, [`damage_sprites`]); showing `old`'s
    ///   standing, second standing, dying or undead sprite it shows `new`'s; a combat object of
    ///   two masters with charges takes the change of the maximum (`+0x9e0`); in state 2 the
    ///   damage sprites again with `new`; with [`UpgradeMaster::Replaced`] the master is `new` and the class
    ///   counters take it back (+1);
    /// - a combat object whose class changes takes a new unit AI ([`class_changes`],
    ///   `FUN_141215a90`); a building with an adjacent mode turns its facets (`FUN_1411ce890`,
    ///   drawn).
    ///
    /// Not simulation: the particles (`DAT_143e40318`), the AI player's list (`+0x7a8`,
    /// `FUN_141612dd0`, `FUN_14160c400`) and the statistics. An object of an overridden sprite
    /// (`+0x788`) would read the sprites of the master it names.
    ///
    /// # Errors
    ///
    /// A sprite the data file lacks, a load given to a sharing player with allies, and what the
    /// actions' change and the stop do not model.
    pub(crate) fn upgrade_object(
        &mut self,
        player_id: PlayerId,
        place: Place,
        (old, new): (&Master, &Master),
        keep: UpgradeMaster,
    ) -> GenieResult<()> {
        let kind = old.kind;
        if MasterKind::BUILDING <= kind {
            let (from, to) = (
                old.building
                    .as_ref()
                    .and_then(|building_master| building_master.construction_sprite),
                new.building
                    .as_ref()
                    .and_then(|building_master| building_master.construction_sprite),
            );
            self.show_if(place, from, to)?;
        }
        let class_changed = MasterKind::COMBAT <= kind
            && class_changes(
                player_master(&self.model.masters, player_id, new.id),
                player_master(&self.model.masters, player_id, old.id),
                &self.model.roles,
            );
        if MasterKind::COMBAT <= kind {
            let id = self.object_at_mut(place)?.id;
            self.parts.combat_mut(id)?.firepower +=
                new.creatable_part().volley_fire - old.creatable_part().volley_fire;
        }
        if MasterKind::RGE_COMBAT <= kind && new.combat_part().attack_sprite.is_some() {
            self.show_if(
                place,
                old.combat_part().attack_sprite,
                new.combat_part().attack_sprite,
            )?;
        }
        if MasterKind::ACTION <= kind {
            self.upgrade_actions(place, (old, new), keep)?;
        }
        if MasterKind::MOVING <= kind {
            self.upgrade_moving_sprite(place, (old, new))?;
        }
        // the static object's part
        if keep == UpgradeMaster::Replaced {
            self.hand_in_load(player_id, place, old)?;
        }
        self.upgrade_sight(place, (old, new), keep)?;
        self.upgrade_hit_points(place, (old, new))?;
        if keep == UpgradeMaster::Replaced {
            self.object_at_mut(place)?.life.master_id = new.id;
            if MasterKind::COMBAT <= kind {
                count_class(
                    &mut self.players.player_mut(player_id)?.attributes,
                    new,
                    1.0,
                    &self.model.roles,
                );
            }
        }
        if class_changed {
            self.new_unit_ai(player_id, place, new)?;
        }
        Ok(())
    }

    /// The action object's part of an upgrade from `old` to `new` (`FUN_14106a530`): each action
    /// takes `new`'s task of its type, class and target ([`World::actions_take_master`]); without
    /// `keep` a top action of a type `new` has no task for stops the object (slot 0x148).
    fn upgrade_actions(
        &mut self,
        place: Place,
        (old, new): (&Master, &Master),
        keep: UpgradeMaster,
    ) -> GenieResult<()> {
        self.actions_take_master(place, old, new)?;
        if keep == UpgradeMaster::Replaced
            && let Some(top) = self
                .parts
                .actions_of(self.object_at(place)?.id)
                .top_action_kind()
            && !new
                .action_part()
                .tasks
                .iter()
                .any(|task| task.action_kind == top)
            && let Place::Live(player_id, list_index) = place
        {
            self.object_stop(player_id, list_index)?;
        }
        Ok(())
    }

    /// The moving object's part of an upgrade (`FUN_14114cb80`): an object showing `old`'s
    /// walking sprite shows `new`'s (when it has one), else one showing `old`'s running sprite
    /// shows `new`'s.
    fn upgrade_moving_sprite(
        &mut self,
        place: Place,
        (old, new): (&Master, &Master),
    ) -> GenieResult<()> {
        let shows = |sprite_id: Option<SpriteId>| {
            self.object_at(place)
                .ok()
                .and_then(|unit| unit.sprite)
                .is_some_and(|active_sprite| Some(active_sprite.graphic.id) == sprite_id)
        };
        if shows(old.moving_part().move_sprite) {
            if let Some(sprite_id) = new.moving_part().move_sprite {
                self.show(place, sprite_id)?;
            }
        } else if shows(old.moving_part().run_sprite)
            && let Some(sprite_id) = new.moving_part().run_sprite
        {
            self.show(place, sprite_id)?;
        }
        Ok(())
    }

    /// The static object's part of an upgrade from `old` without its master kept
    /// (`FUN_141186580`): a combat object leaves its class counters
    /// ([`crate::objects::lifecycle::death_parts::count_class`] with −1) and a villager's load
    /// goes to its player (slot 0xf8 with 1 and 1, the foods as food: [`World::give_load`]).
    fn hand_in_load(&mut self, player_id: PlayerId, place: Place, old: &Master) -> GenieResult<()> {
        let kind = old.kind;
        if MasterKind::COMBAT <= kind {
            count_class(
                &mut self.players.player_mut(player_id)?.attributes,
                old,
                -1.0,
                &self.model.roles,
            );
        }
        let object_at = self.object_at(place)?;
        if MasterKind::COMBAT <= kind
            // The villager class (4), whose load an upgrade without its master kept goes to its player.
            && self.model.roles.of_class(old.class).contains(ClassRoles::VILLAGER)
            && Amount::ZERO < object_at.life.amount
            && let Some(kind) = object_at.life.attribute_kind
        {
            // the foods a unit carries (15 to 17) as food
            let kind = if kind.is_food_load() {
                self.model.slots.attribute.food
            } else {
                kind
            };
            let amount = object_at.life.amount;
            self.give_load(player_id, kind, amount)?;
            let unit = self.object_at_mut(place)?;
            unit.life.attribute_kind = None;
            unit.life.amount = Amount::ZERO;
        }
        Ok(())
    }

    /// An upgrade's sight (`FUN_141186580`): an object in state 2 on the map, not inside another,
    /// takes its sight off and on again with its line of sight moved by the change from `old`'s to
    /// `new`'s (slots 0x1d8 and 0x1d0); with [`UpgradeMaster::Replaced`] its seer takes `new`'s id.
    fn upgrade_sight(
        &mut self,
        place: Place,
        (old, new): (&Master, &Master),
        keep: UpgradeMaster,
    ) -> GenieResult<()> {
        let id = self.object_at(place)?.id;
        let unit = self.object_at(place)?;
        if unit.life.state == ObjectState::Alive
            && unit.life.inside.is_none()
            && let Some(placed) = self.spatial.get(id).copied()
            && let Some(sight) = self.spatial.sight.as_mut()
        {
            let now = sight.seer(id).map_or(old.line_of_sight, |seer| seer.los);
            let los = (now - old.line_of_sight) + new.line_of_sight;
            sight.sight_off(
                id,
                placed.position,
                placed.radius,
                SightShape::Circle,
                SightCentre::Tile,
                SightLayer {
                    roles: &self.model.roles,
                    visibility_tuning: self.model.tuning.visibility,
                },
            )?;
            let _ = sight.sight_on_los(
                id,
                LosSight::tile_circle(placed.position, placed.radius, SightTiles::of(los)),
                &self.model.roles,
                self.model.tuning.visibility,
            )?;
            if let Some(seer) = sight.seer_mut(id) {
                seer.los = los;
                if keep == UpgradeMaster::Replaced {
                    seer.master = new.id;
                }
            }
        }
        Ok(())
    }

    /// An upgrade's hit points and sprites (`FUN_141186580`): the hit points take their share of
    /// `new`'s (`old`'s damage sprites at full hit points first, in state 2,
    /// [`damage_sprites`]); showing `old`'s standing, second standing, dying or undead sprite
    /// the object shows `new`'s; a combat object of two masters with charges takes the change
    /// of the maximum (`+0x9e0`); in state 2 the damage sprites again with `new`.
    fn upgrade_hit_points(
        &mut self,
        place: Place,
        (old, new): (&Master, &Master),
    ) -> GenieResult<()> {
        let kind = old.kind;
        let unit = self.object_at(place)?;
        let most = HitPoints::of_master(old.hit_points).get();
        let share = if most.abs() <= NO_HIT_POINTS {
            0.0
        } else {
            unit.life.hp.get() / most
        };
        let before = damage_share_of(unit, old, &self.model.roles);
        let state = unit.life.state;
        self.object_at_mut(place)?.life.hp = HitPoints::new(most);
        let mut held = before;
        if state == ObjectState::Alive {
            held = self.damage_sprites_at(place, old, held)?;
        }
        let sprites = [
            (old.standing_sprite, new.standing_sprite),
            (
                old.standing_sprite_2,
                new.standing_sprite_2.or(new.standing_sprite),
            ),
            (old.death_sprite, new.death_sprite),
            (old.undead_sprite, new.undead_sprite),
        ];
        let shown = self
            .object_at(place)?
            .sprite
            .map(|active_sprite| active_sprite.graphic.id);
        let next = sprites
            .iter()
            .find(|&&(from, _)| from.is_some() && shown == from)
            .map(|&(_, to)| to);
        if MasterKind::COMBAT <= kind
            && MasterKind::COMBAT <= new.kind
            && old.creatable_part().charge_kind.charges()
            && new.creatable_part().charge_kind.charges()
        {
            let more = (new.creatable_part().max_charge - old.creatable_part().max_charge).abs();
            if Charge::ZERO < more {
                let id = self.object_at_mut(place)?.id;
                self.parts.combat_mut(id)?.charge += more;
            }
        }
        self.object_at_mut(place)?.life.hp =
            HitPoints::new(HitPoints::of_master(new.hit_points).get() * share);
        if let Some(sprite_id) = next.flatten() {
            self.show(place, sprite_id)?;
        }
        if state == ObjectState::Alive {
            let _ = self.damage_sprites_at(place, new, held)?;
        }
        Ok(())
    }

    /// [`damage_sprites`] on the object at `place` with the world's graphics and streams.
    fn damage_sprites_at(
        &mut self,
        place: Place,
        master: &Master,
        before: HpPercent,
    ) -> GenieResult<HpPercent> {
        let graphics = take(&mut self.model.graphics);
        let mut object = take(self.streams.get_mut(Stream::Object));
        let mut sprite = take(self.streams.get_mut(Stream::Sprite));
        let unit = self.players.at_mut(place)?;
        let motion = self.parts.motion_mut(unit.id);
        let hp_percent = damage_sprites(
            (unit, motion),
            master,
            before,
            (&graphics, &mut object, &mut sprite),
            &self.model.roles,
            self.model.tuning.sprite,
        );
        self.model.graphics = graphics;
        *self.streams.get_mut(Stream::Object) = object;
        *self.streams.get_mut(Stream::Sprite) = sprite;
        hp_percent
    }

    /// Slot 0x68 on the object at `place` with sprite `to` when it shows `from` (both sprites).
    fn show_if(
        &mut self,
        place: Place,
        from: Option<SpriteId>,
        to: Option<SpriteId>,
    ) -> GenieResult<()> {
        let shows = self
            .object_at(place)?
            .sprite
            .is_some_and(|active_sprite| Some(active_sprite.graphic.id) == from);
        if shows && let Some(to) = to {
            self.show(place, to)?;
        }
        Ok(())
    }

    /// Slot 0x68 on the object at `place` with sprite `id`: a new instance unless it shows it
    /// already ([`crate::objects::sprite::new_sprite`] on the Sprite stream). Without the graphics (no
    /// data file) the Sprite stream is not modelled.
    fn show(&mut self, place: Place, id: SpriteId) -> GenieResult<()> {
        if self.model.graphics.is_empty() {
            return Ok(());
        }
        let graphic = self.model.graphics.get(id).copied().ok_or(NO_SPRITE)?;
        let rng = self.streams.get_mut(Stream::Sprite);
        let unit = match place {
            Place::Live(owner, slot) => self.players.live_mut(owner, slot)?,
            Place::Sleeping(player_id, list_index) => {
                self.players.asleep_mut(player_id, list_index)?
            }
        };
        new_sprite(
            &mut unit.sprite,
            self.parts.move_stats_mut(unit.id),
            graphic,
            rng,
            self.model.tuning.sprite,
        );
        Ok(())
    }

    /// `FUN_141215a90` on the object at `place`, whose master became `new`: its unit AI goes and
    /// the factory's type for `new` makes another ([`crate::unit_ai::factory_kind`],
    /// [`crate::unit_ai::UnitAi::constructed`] on the AI stream), none for a class without.
    fn new_unit_ai(&mut self, player_id: PlayerId, place: Place, new: &Master) -> GenieResult<()> {
        let position = self.object_at(place)?.position();
        let made = self.new_object(player_id, new.id, new, position)?;
        let ai = unit_ai_kind(new, new.id, &self.model.roles).map(|unit_ai_kind| {
            UnitAi::constructed(
                unit_ai_kind,
                &made,
                self.streams.get_mut(Stream::Ai),
                &self.model.roles,
            )
        });
        let id = self.object_at(place)?.id;
        self.set_part(id, Part::UnitAi(ai.map(Box::new)))?;
        Ok(())
    }

    /// `FUN_140a97970` -> `FUN_140a78150` on the actions of the object at `place` whose master
    /// `old` gives way to `new` ([`take_tasks`]): each takes the last of `new`'s tasks of its
    /// task's type, class and target, its slot 0x50 and its sub-action's with the two tasks'
    /// sprites; one without a task runs them with none. An object the table holds no layer of
    /// has no actions.
    ///
    /// # Errors
    ///
    /// A place the world does not hold.
    pub(crate) fn actions_take_master(
        &mut self,
        place: Place,
        old: &Master,
        new: &Master,
    ) -> GenieResult<()> {
        let id = self.object_at(place)?.id;
        let Some(actions) = self.parts.actions.get_mut(id) else {
            return Ok(());
        };
        let unit = match place {
            Place::Live(owner, slot) => self.players.live_mut(owner, slot)?,
            Place::Sleeping(owner, slot) => self.players.asleep_mut(owner, slot)?,
        };
        let mut looks = Looks {
            sprite: &mut unit.sprite,
            stats: self.parts.motion.move_stats_mut(id),
            rng: self.streams.get_mut(Stream::Sprite),
            sprite_tuning: self.model.tuning.sprite,
        };
        take_tasks(
            &mut actions.list,
            old,
            new,
            &self.model.graphics,
            &mut looks,
        );
        Ok(())
    }

    /// The object at `place`.
    ///
    /// # Errors
    ///
    /// A place the world does not hold.
    pub(crate) fn object_at(&self, place: Place) -> GenieResult<&Unit> {
        match place {
            Place::Live(owner, slot) => self.players.live(owner, slot),
            Place::Sleeping(player_id, list_index) => self.players.asleep(player_id, list_index),
        }
    }

    /// The object at `place`, to change.
    ///
    /// # Errors
    ///
    /// A place the world does not hold.
    pub(crate) fn object_at_mut(&mut self, place: Place) -> GenieResult<&mut Unit> {
        match place {
            Place::Live(owner, slot) => self.players.live_mut(owner, slot),
            Place::Sleeping(player_id, list_index) => {
                self.players.asleep_mut(player_id, list_index)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(test)]
    use crate::combat::Firepower;
    #[cfg(test)]
    use crate::construction::FoundationId;
    #[cfg(test)]
    use crate::construction::build::tests::started;
    use crate::construction::wait_solidify::WaitSolidifyState;
    #[cfg(test)]
    use crate::construction::wait_solidify::{WAIT_SOLIDIFY, WaitSolidify};
    use crate::garrison::ContainerId;
    use crate::gathering::make::MakeState;
    #[cfg(test)]
    use crate::gathering::make::{MAKE, MakeAction};
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Position;
    #[cfg(test)]
    use crate::map::spatial::{Placed, Spatial};
    use crate::math::rng::Well512;
    use crate::objects::ListIndex;
    use crate::objects::MasterHitPoints;
    use crate::objects::ObjectId;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::WorkRate;
    #[cfg(test)]
    use crate::objects::layers::Combat;
    use crate::objects::layers::MotionMut;
    use crate::objects::lifecycle::Held;
    #[cfg(test)]
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::ActionKind;
    use crate::objects::masters::ActionPart;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ChargeKind;
    use crate::objects::masters::CreatablePart;
    use crate::objects::masters::DamageSpriteMode;
    use crate::objects::masters::MasterId;
    use crate::objects::masters::MovingPart;
    use crate::objects::masters::ObstructionKind;
    use crate::objects::masters::TaskKey;
    use crate::objects::masters::TaskSprites;
    #[cfg(test)]
    use crate::objects::masters::WorkValue;
    use crate::objects::masters::{BuildingMaster, DamageGraphic, Masters};
    #[cfg(test)]
    use crate::objects::sprite::ActiveSprite;
    use crate::objects::sprite::Graphic;
    #[cfg(test)]
    use crate::objects::{FacetCount, Frame, Graphics};
    use crate::ownership::CaptureKind;
    use crate::players::PlayerKind;
    use crate::players::attribute::AttributeId;
    use crate::players::tracker::ListKey;
    use crate::technology::UpgradeMode;
    use crate::test_ids::carrying;
    use crate::test_ids::class_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::opt_master;
    use crate::test_ids::task_id;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::tuning;
    use crate::vision::TileCount;
    use crate::vision::VisionBits;
    #[cfg(test)]
    use crate::vision::visibility::{Seer, Visibility, VisibleMap};
    use crate::world::Millis;
    use crate::world::Seconds;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::tests::forage_world;
    #[cfg(test)]
    use crate::world::{FrameSeed, Player};
    use std::sync::Arc;

    /// An animated graphic of `frames` frames with a random first frame (flags 5).
    const fn graphic(id: i16) -> Graphic {
        Graphic {
            id: SpriteId::named(id),
            frames: Frame::of_data(10),
            facets: FacetCount::of_data(1),
            speed_multiplier: SpeedMultiplier::ONE,
            frame_duration: Seconds::new(0.1),
            replay_delay: Seconds::ZERO,
            flags: SpriteFlags::ANIMATED.union(SpriteFlags::RANDOM_FIRST_FRAME),
        }
    }

    /// A building master `id` of `hp` hit points with three damage sprites of `mode`, its
    /// standing sprite `id + 1000`, its death spawn `dead`.
    fn building(id: i16, hp: i16, mode: DamageSpriteMode, dead: i16) -> Master {
        Master {
            id: master_id(id),
            kind: MasterKind::BUILDING,
            class: class::BUILDING,
            hit_points: MasterHitPoints::of_data(hp),
            standing_sprite: Some(SpriteId::named(id + 1000)),
            death_spawns: [opt_master(dead), None],
            damage_graphics: [25, 50, 75]
                .map(|percent| DamageGraphic {
                    sprite: Some(SpriteId::named(id + 2000 + i16::from(percent))),
                    percent: HpPercent::of_data(percent),
                    apply_mode: mode,
                })
                .to_vec(),
            building: Some(BuildingMaster {
                construction_sprite: Some(SpriteId::named(id + 3000)),
                ..BuildingMaster::default()
            }),
            ..Master::default()
        }
    }

    /// A unit of master `master` in `state` with `hp`.
    fn object(id: i32, master: MasterId, state: ObjectState, hp: HitPoints) -> Unit {
        Unit {
            id: object_id(id),
            life: Lifecycle {
                master_id: master,
                state,
                hp,
                ..Lifecycle::default()
            },
            ..Unit::default()
        }
    }

    /// `FUN_1411859a0`: a master without damage sprites leaves the share and the hit points;
    /// the hit points stay within 0 and the master's (a NaN stays); a share that moves the object
    /// to another entry draws twice from the Object stream for an entry of mode 1, shows the
    /// entry's sprite for mode 2 and the standing sprite again when a mode 2 entry goes, and
    /// does nothing to the simulation for mode 0.
    #[test]
    fn the_damage_sprites_follow_the_share_of_the_hit_points() {
        let graphics: Graphics = [70, 1070, 2095, 2120, 2145]
            .map(|id| (SpriteId::named(id), graphic(id)))
            .into_iter()
            .collect();
        let (mut object, mut sprite) = (Well512::default(), Well512::default());
        let bare = Master {
            hit_points: MasterHitPoints::of_data(100),
            ..Master::default()
        };
        let mut unit = object_of(HitPoints::new(150.0));
        let hp_percent = damage_sprites(
            (&mut unit, MotionMut::new(None)),
            &bare,
            HpPercent::of_data(7),
            (&graphics, &mut object, &mut sprite),
            &roles(),
            tuning().sprite,
        );
        assert_eq!(
            (hp_percent, unit.life.hp),
            (Ok(HpPercent::of_data(7)), HitPoints::new(150.0))
        );
        let placed = building(70, 100, DamageSpriteMode::PLACED, -1);
        let hp_percent = damage_sprites(
            (&mut unit, MotionMut::new(None)),
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
            &roles(),
            tuning().sprite,
        );
        assert_eq!(
            (hp_percent, unit.life.hp),
            (Ok(HpPercent::of_data(0)), HitPoints::new(100.0))
        );
        let mut low = object_of(HitPoints::new(-5.0));
        let hp_percent = damage_sprites(
            (&mut low, MotionMut::new(None)),
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
            &roles(),
            tuning().sprite,
        );
        assert_eq!(
            (hp_percent, low.life.hp),
            (Ok(HpPercent::of_data(100)), HitPoints::ZERO)
        );
        let mut nan = object_of(HitPoints::new(f32::NAN));
        let _ = damage_sprites(
            (&mut nan, MotionMut::new(None)),
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
            &roles(),
            tuning().sprite,
        );
        assert!(nan.life.hp.get().is_nan());
        // mode 0: nothing drawn
        let before = object.clone();
        let mut hurt = object_of(HitPoints::new(40.0));
        assert_eq!(
            damage_sprites(
                (&mut hurt, MotionMut::new(None)),
                &placed,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(object, before);
        // mode 1: two draws of the Object stream
        let scattered = building(70, 100, DamageSpriteMode::SCATTERED, -1);
        let mut expected = object.clone();
        let _ = (expected.rand15(), expected.rand15());
        let mut hurt = object_of(HitPoints::new(40.0));
        assert_eq!(
            damage_sprites(
                (&mut hurt, MotionMut::new(None)),
                &scattered,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(object, expected);
        // the same entry (25 < 30 and 40, below 50): nothing more
        let mut hurt = object_of(HitPoints::new(60.0));
        assert_eq!(
            damage_sprites(
                (&mut hurt, MotionMut::new(None)),
                &scattered,
                HpPercent::of_data(30),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(40))
        );
        assert_eq!(object, expected);
        // mode 2: the entry's sprite, then the standing one when it goes
        let shown = building(70, 100, DamageSpriteMode::SHOWN, -1);
        let mut unit = Unit {
            sprite: Some(ActiveSprite::start(
                graphic(1070),
                &mut sprite,
                tuning().sprite,
            )),
            ..object_of(HitPoints::new(40.0))
        };
        assert_eq!(
            damage_sprites(
                (&mut unit, MotionMut::new(None)),
                &shown,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(
            unit.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(2120))
        );
        unit.life.hp = HitPoints::new(100.0);
        assert_eq!(
            damage_sprites(
                (&mut unit, MotionMut::new(None)),
                &shown,
                HpPercent::of_data(60),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(0))
        );
        assert_eq!(
            unit.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(1070))
        );
    }

    /// `FUN_1411859a0`'s mode 2 on an object whose sprite the slice does not hold: slot 0x68's
    /// new instance is not held, but its start's draws on the Sprite stream are, for the entry's
    /// sprite and for the standing sprite when the entry goes.
    #[test]
    fn shown_damage_sprite_of_object_without_sprite_keeps_only_its_draws() {
        let graphics: Graphics = [70, 1070, 2095, 2120, 2145]
            .map(|id| (SpriteId::named(id), graphic(id)))
            .into_iter()
            .collect();
        let (mut object, mut sprite) = (Well512::default(), Well512::default());
        let shown = building(70, 100, DamageSpriteMode::SHOWN, -1);
        let mut unit = object_of(HitPoints::new(40.0));
        let mut expected = sprite.clone();
        let _ = ActiveSprite::start(graphic(2120), &mut expected, tuning().sprite);
        assert_ne!(expected, Well512::default());
        assert_eq!(
            damage_sprites(
                (&mut unit, MotionMut::new(None)),
                &shown,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!((unit.sprite, &sprite), (None, &expected));
        unit.life.hp = HitPoints::new(100.0);
        let _ = ActiveSprite::start(graphic(1070), &mut expected, tuning().sprite);
        assert_eq!(
            damage_sprites(
                (&mut unit, MotionMut::new(None)),
                &shown,
                HpPercent::of_data(60),
                (&graphics, &mut object, &mut sprite),
                &roles(),
                tuning().sprite
            ),
            Ok(HpPercent::of_data(0))
        );
        assert_eq!((unit.sprite, &sprite), (None, &expected));
        assert_eq!(object, Well512::default());
    }

    fn object_of(hp: HitPoints) -> Unit {
        object(1, ids::HOUSE, ObjectState::Alive, hp)
    }

    /// Slot 0xa8 and `FUN_1415c6aa0`: a combat object or building takes an upgrade below state
    /// 4, any other object always; a class change makes a new unit AI but for the pairs 12/47,
    /// 18/43 and 43/18 (the new class first).
    #[test]
    fn who_takes_an_upgrade_and_who_new_unit_ai() {
        assert!(upgradable(MasterKind::BUILDING, ObjectState::Dying));
        assert!(!upgradable(MasterKind::BUILDING, ObjectState::DyingSprite));
        assert!(!upgradable(MasterKind::COMBAT, ObjectState::Removed));
        assert!(upgradable(MasterKind::ACTION, ObjectState::Removed));
        let master_of = |class| Master {
            class,
            ..Master::default()
        };
        assert!(!class_changes(
            Some(&master_of(class::BUILDING)),
            Some(&master_of(class::BUILDING)),
            &roles()
        ));
        assert!(class_changes(
            Some(&master_of(class::CLASS_6)),
            Some(&master_of(class::CLASS_12)),
            &roles()
        ));
        assert!(!class_changes(
            Some(&master_of(class::CLASS_12)),
            Some(&master_of(class::CLASS_47)),
            &roles()
        ));
        assert!(class_changes(
            Some(&master_of(class::CLASS_47)),
            Some(&master_of(class::CLASS_12)),
            &roles()
        ));
        assert!(!class_changes(
            Some(&master_of(class::MONK)),
            Some(&master_of(class::CLASS_43)),
            &roles()
        ));
        assert!(!class_changes(
            Some(&master_of(class::CLASS_43)),
            Some(&master_of(class::MONK)),
            &roles()
        ));
        assert!(!class_changes(
            None,
            Some(&master_of(class::BUILDING)),
            &roles()
        ));
    }

    /// The Feudal Age's upgrade of the house (70 to 464, mode −1, `FUN_141657d10`): a house in
    /// state 2 keeps its master and its share of the hit points (275 of 550: 450 of 900), and
    /// shows the new standing sprite; a dead one (state 4) and the rubble of its old death spawn
    /// (1403, whose copy differs from 464's 1435) take masters of their own, the dead one also
    /// its death spawn's; the player's master 70 then takes 464's values but its id and class.
    #[test]
    fn an_age_upgrade_keeps_house_share_of_its_hit_points() {
        let old = Master {
            death_spawns: [Some(master_id(1403)), None],
            ..building(70, 550, DamageSpriteMode::PLACED, 1403)
        };
        let new = Master {
            class: class::BUILDING,
            ..building(464, 900, DamageSpriteMode::PLACED, 1435)
        };
        let rubble = |id: i16, copy: i16| Master {
            id: master_id(id),
            copy_id: opt_master(copy),
            kind: MasterKind::STATIC,
            ..Master::default()
        };
        let mut house = object(10, ids::HOUSE, ObjectState::Alive, HitPoints::new(275.0));
        let mut sprites = Well512::default();
        house.sprite = Some(ActiveSprite::start(
            graphic(1070),
            &mut sprites,
            tuning().sprite,
        ));
        let player = Player {
            objects: vec![
                house,
                object(11, ids::HOUSE, ObjectState::DyingSprite, HitPoints::ZERO),
                object(12, master_id(1403), ObjectState::Alive, HitPoints::new(1.0)),
            ]
            .into(),
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        world.model.masters = vec![Arc::new(Masters::new([
            old,
            new,
            rubble(1403, 1403),
            rubble(1435, 1435),
        ]))];
        world.model.graphics = Arc::new(
            [1070, 1464, 3070, 3464]
                .map(|id| (SpriteId::named(id), graphic(id)))
                .into_iter()
                .collect(),
        );
        world
            .upgrade_objects(
                PlayerId::new(0),
                master_id(70),
                master_id(464),
                UpgradeMode::KEEP,
                None,
            )
            .unwrap();
        let [house, dead, rubble] = world.players[0].objects.as_slice() else {
            panic!("three objects");
        };
        assert_eq!(
            (house.life.master_id, house.life.hp),
            (master_id(70), HitPoints::new(450.0))
        );
        assert_eq!(
            house.sprite.map(|active_sprite| active_sprite.graphic.id),
            Some(SpriteId::named(1464))
        );
        assert!(!world.parts.own_master.contains(house.id));
        assert_eq!(
            world
                .parts
                .own_master
                .get(dead.id)
                .map(|master| master.hit_points),
            Some(MasterHitPoints::of_data(550))
        );
        assert_eq!(
            world.parts.dead_master.get(dead.id).map(|master| master.id),
            Some(master_id(1403))
        );
        assert_eq!(
            world
                .parts
                .own_master
                .get(rubble.id)
                .map(|master| master.id),
            Some(master_id(1403))
        );
        let master = world.model.masters[0].get(ids::HOUSE).unwrap();
        assert_eq!(
            (master.id, master.hit_points, master.death_spawns[0]),
            (
                master_id(70),
                MasterHitPoints::of_data(900),
                Some(master_id(1435))
            )
        );
    }

    /// An upgrade without its master kept (mode 0): the objects take the new master, a combat
    /// object leaves its old class's counters and joins the new one's, a villager's load of
    /// berries goes to its player as food, and the missiles per volley take the change.
    #[test]
    fn an_upgrade_to_another_master_moves_the_class_counters_and_the_load() {
        let old = Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            hit_points: MasterHitPoints::of_data(25),
            creatable: Some(CreatablePart {
                volley_fire: Firepower::new(1.0),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        let new = Master {
            id: ids::ID_6,
            kind: MasterKind::COMBAT,
            class: class::CLASS_6,
            hit_points: MasterHitPoints::of_data(50),
            creatable: Some(CreatablePart {
                volley_fire: Firepower::new(3.0),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        let mut object = object(
            10,
            ids::VILLAGER_MALE,
            ObjectState::Alive,
            HitPoints::new(25.0),
        );
        (object.life.attribute_kind, object.life.amount) =
            (Some(AttributeId::named(17)), Amount::new(10.0));
        let player = Player {
            objects: vec![object].into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        )
        .with_combats([(
            object_id(10),
            Combat {
                firepower: Firepower::new(1.0),
                ..Default::default()
            },
        )]);
        world.model.masters = vec![Arc::new(Masters::new([old, new]))];
        world.players[0].attributes[37] = Amount::new(1.0);
        world
            .upgrade_objects(
                PlayerId::new(0),
                master_id(83),
                master_id(6),
                UpgradeMode::WALK,
                None,
            )
            .unwrap();
        let unit = &world.players[0].objects[ListIndex::new(0)];
        assert_eq!(
            (
                unit.life.master_id,
                unit.life.hp,
                world.parts.combat_of(unit.id).firepower
            ),
            (master_id(6), HitPoints::new(50.0), Firepower::new(3.0))
        );
        assert_eq!(unit.life.held(), carrying(None, 0.0));
        let amounts = &world.players[0].attributes;
        assert_eq!((amounts[37], amounts[40]), (Amount::ZERO, Amount::new(1.0)));
        assert_eq!(world.players[0].attributes[0], Amount::new(10.0));
        assert_eq!(
            world.model.masters[0]
                .get(ids::VILLAGER_MALE)
                .map(|master| master.class),
            Some(class::VILLAGER)
        );
    }

    /// `FUN_140a97970` -> `FUN_140a78150`: each action finds its task's match among the new
    /// master's tasks (action type, target class and master; the last of equals) and takes it; an
    /// action without a match keeps its task.
    #[test]
    fn an_upgrade_actions_take_the_new_master_last_matching_task() {
        use crate::construction::build::BuildTask;
        use crate::gathering::gather::GATHER;
        use crate::gathering::hunt::HuntTask;
        use crate::objects::masters::Task;
        let task = |id, action_kind, object_group, work_range| Task {
            id,
            action_kind,
            object_group,
            work_range,
            work_val1: WorkValue::new((work_range * 2.0).get()),
            attribute_kind: Some(AttributeId::named(17)),
            ..Task::default()
        };
        let mut world = forage_world();
        world
            .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
            .unwrap();
        let mut old = world.model.masters[1].get(master_id(120)).unwrap().clone();
        old.action
            .get_or_insert_with(Default::default)
            .tasks
            .push(task(
                task_id(3),
                ActionKind::BUILD,
                Some(class_id(3)),
                Distance::new(0.25),
            ));
        let make = task(task_id(4), MAKE, None, Distance::ZERO);
        let waiting = task(task_id(9), WAIT_SOLIDIFY, Some(class_id(3)), Distance::ZERO);
        let new = Master {
            id: master_id(121),
            action: Some(ActionPart {
                tasks: vec![
                    task(task_id(5), GATHER, Some(class_id(7)), Distance::new(0.5)),
                    task(task_id(6), GATHER, Some(class_id(7)), Distance::new(0.75)),
                    Task {
                        sprites: TaskSprites::of_data([12, -1, -1, -1]),
                        ..task(
                            task_id(7),
                            ActionKind::BUILD,
                            Some(class_id(3)),
                            Distance::new(1.5),
                        )
                    },
                    task(task_id(8), MAKE, None, Distance::new(2.0)),
                ],
                ..old.action.clone().unwrap_or_default()
            }),
            ..old.clone()
        };
        let players = &mut world.players[1].objects[ListIndex::new(0)];
        let held = old
            .action_part()
            .tasks
            .iter()
            .find(|task| task.id == task_id(3));
        let build = BuildTask::of(held.unwrap(), WorkRate::new(1.0));
        world
            .parts
            .actions
            .of_mut(players.id)
            .unwrap()
            .list
            .set_build(Some(started(
                object_id(10028),
                Position::new(30.0, 88.0, 0.0),
                Distance::new(1.0),
                build,
            )));
        world
            .parts
            .actions
            .of_mut(players.id)
            .unwrap()
            .list
            .set_make(Some(MakeAction {
                state: MakeState::New,
                remaining: Amount::ZERO,
                total: Amount::ZERO,
                task: make,
            }));
        world
            .parts
            .actions
            .of_mut(players.id)
            .unwrap()
            .list
            .set_wait_solidify(Some(WaitSolidify {
                state: WaitSolidifyState::Waiting,
                task: waiting,
                target: FoundationId::new(object_id(10028)),
            }));
        world
            .actions_take_master(Place::Live(PlayerId::new(1), ListIndex::new(0)), &old, &new)
            .unwrap();
        let unit = &world.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            world
                .parts
                .actions_of(unit.id)
                .list
                .gather()
                .copied()
                .unwrap()
                .task,
            HuntTask::of(&new.action_part().tasks[1])
        );
        assert_eq!(
            world
                .parts
                .actions_of(unit.id)
                .list
                .build()
                .copied()
                .unwrap()
                .task,
            BuildTask {
                range: Distance::new(1.5),
                work_value: WorkValue::new(3.0),
                work_rate: WorkRate::new(1.0),
                resource: Some(AttributeId::named(17)),
                id: task_id(7),
                key: TaskKey {
                    action_kind: ActionKind::BUILD,
                    object_group: Some(class_id(3)),
                    object_id: None,
                },
                sprites: TaskSprites {
                    moving: Some(SpriteId::named(12)),
                    ..TaskSprites::NONE
                },
            }
        );
        assert_eq!(
            world
                .parts
                .actions_of(unit.id)
                .list
                .make()
                .copied()
                .unwrap()
                .task,
            new.action_part().tasks[3]
        );
        assert_eq!(
            world
                .parts
                .actions_of(unit.id)
                .list
                .wait_solidify()
                .copied()
                .unwrap()
                .task,
            waiting
        );
        // a Gather holds its task's key, as the game its pointer: a master that lacks the task
        // still gives way to the new master's last task of that key
        let gather = world
            .parts
            .actions_of(unit.id)
            .list
            .gather()
            .copied()
            .unwrap()
            .task;
        let mut bare = old.clone();
        bare.action
            .get_or_insert_with(Default::default)
            .tasks
            .clear();
        world
            .actions_take_master(
                Place::Live(PlayerId::new(1), ListIndex::new(0)),
                &bare,
                &new,
            )
            .unwrap();
        assert_eq!(
            world
                .parts
                .actions_of(world.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .copied()
                .unwrap()
                .task,
            gather
        );
    }

    /// Player 0's world for `FUN_141657d10`'s modes: soldiers 10 and 11 and the sleeping 12 of
    /// master 4 (40 hit points, each at 20), 11 with a master of its own of id 4; the masters 5
    /// and 0x67f (60 hit points); 10 filed in the tracker.
    fn upgrade_world() -> World {
        let soldier = |id, hit_points| Master {
            id,
            kind: MasterKind::COMBAT,
            class: class::CLASS_6,
            hit_points,
            ..Master::default()
        };
        let own = object(11, master_id(4), ObjectState::Alive, HitPoints::new(20.0));
        let player = Player {
            objects: vec![
                object(10, master_id(4), ObjectState::Alive, HitPoints::new(20.0)),
                own,
            ]
            .into(),
            sleeping: vec![object(
                12,
                master_id(4),
                ObjectState::Alive,
                HitPoints::new(20.0),
            )]
            .into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        world.model.masters = vec![Arc::new(Masters::new([
            soldier(master_id(4), MasterHitPoints::of_data(40)),
            soldier(master_id(5), MasterHitPoints::of_data(60)),
            soldier(master_id(0x67f), MasterHitPoints::of_data(60)),
        ]))];
        world
            .set_part(
                object_id(11),
                Part::OwnMaster(Some(Box::new(soldier(
                    master_id(4),
                    MasterHitPoints::of_data(40),
                )))),
            )
            .unwrap();
        world
            .track_add(PlayerId::new(0), object_id(10), master_id(4))
            .unwrap();
        world
    }

    /// `FUN_141657d10` in mode 2 with the effect's object: that object alone takes a master of
    /// its own, its upgrade keeping it, then `to`'s values copied in; the player's master stays.
    /// In mode 3 an object whose own master has `from`'s id takes the upgrade too, and so does a
    /// sleeping object; masters the table lacks change nothing.
    #[test]
    fn mode_2_upgrade_gives_the_effect_object_master_of_its_own() {
        let hp = |world: &World| -> Vec<f32> {
            let player = &world.players[0];
            player
                .objects
                .iter()
                .chain(&player.sleeping)
                .map(|unit| unit.life.hp.get())
                .collect()
        };
        let mut game = upgrade_world();
        game.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(5),
            UpgradeMode::ONE_OBJECT,
            Some(object_id(10)),
        )
        .unwrap();
        let own = game
            .parts
            .own_master
            .get(game.players[0].objects[ListIndex::new(0)].id)
            .unwrap();
        assert_eq!(
            (own.id, own.hit_points),
            (master_id(4), MasterHitPoints::of_data(60))
        );
        assert_eq!(hp(&game), [30.0, 20.0, 20.0]);
        assert_eq!(
            game.model.masters[0]
                .get(master_id(4))
                .map(|master| master.hit_points),
            Some(MasterHitPoints::of_data(40))
        );
        let mut game = upgrade_world();
        game.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(5),
            UpgradeMode::OWN_MASTERS,
            None,
        )
        .unwrap();
        assert_eq!(hp(&game), [30.0, 30.0, 30.0]);
        let own = game
            .parts
            .own_master
            .get(game.players[0].objects[ListIndex::new(1)].id)
            .unwrap();
        assert_eq!(
            (own.id, own.hit_points),
            (master_id(4), MasterHitPoints::of_data(60))
        );
        assert_eq!(
            game.model.masters[0]
                .get(master_id(4))
                .map(|master| master.hit_points),
            Some(MasterHitPoints::of_data(60))
        );
        for (player, to) in [(0, 99), (3, 5)] {
            let mut world = upgrade_world();
            world
                .upgrade_objects(
                    PlayerId::from(player),
                    master_id(4),
                    master_id(to),
                    UpgradeMode::WALK,
                    None,
                )
                .unwrap();
            assert_eq!(world, upgrade_world());
        }
    }

    /// The upgrades to 0x67f and 0x6a1 move the tracker's list of `from` to `to`'s
    /// (`FUN_14104fd10`); another upgrade leaves it.
    #[test]
    fn an_upgrade_to_0x67f_moves_the_tracker_list() {
        let mut world = upgrade_world();
        world
            .upgrade_objects(
                PlayerId::new(0),
                master_id(4),
                master_id(0x67f),
                UpgradeMode::WALK,
                None,
            )
            .unwrap();
        let tracker = &world.players[0].tracker;
        assert_eq!(
            (
                tracker.list(ListKey::master(master_id(0x67f))),
                tracker.list(ListKey::master(master_id(4)))
            ),
            (&[Some(object_id(10))][..], &[][..])
        );
        let mut world = upgrade_world();
        world
            .upgrade_objects(
                PlayerId::new(0),
                master_id(4),
                master_id(5),
                UpgradeMode::WALK,
                None,
            )
            .unwrap();
        assert_eq!(
            world.players[0].tracker.list(ListKey::master(master_id(4))),
            [Some(object_id(10))]
        );
    }
    /// A world of player 0 holding `objects`, its masters `masters`, its graphics `graphics`
    /// (each animated) and 300 attributes.
    fn one_player(objects: Vec<Unit>, masters: Vec<Master>, graphics: &[i16]) -> World {
        let player = Player {
            objects: objects.into(),
            attributes: vec![Amount::ZERO; 300],
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        world.model.masters = vec![Arc::new(Masters::new(masters))];
        world.model.graphics = Arc::new(
            graphics
                .iter()
                .map(|&id| (SpriteId::named(id), graphic(id)))
                .collect(),
        );
        world
    }

    /// `FUN_14106a530`'s stop: without the master kept, a forager whose new master has no task
    /// for its top Gather stops (the Gather goes); kept, or with a Gather task, it goes on.
    #[test]
    fn an_upgrade_stops_top_action_the_new_master_has_no_task_for() {
        let mut world = forage_world();
        world
            .gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
            .unwrap();
        let old = world.model.masters[1].get(master_id(120)).unwrap().clone();
        let bare = Master {
            id: master_id(121),
            action: Some(ActionPart {
                tasks: Vec::new(),
                ..old.action.clone().unwrap_or_default()
            }),
            ..old.clone()
        };
        let place = Place::Live(PlayerId::new(1), ListIndex::new(0));
        let mut kept = world.clone();
        kept.upgrade_actions(place, (&old, &bare), UpgradeMaster::Kept)
            .unwrap();
        assert!(
            kept.parts
                .actions_of(kept.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .is_some()
        );
        let mut same = world.clone();
        same.upgrade_actions(place, (&old, &old), UpgradeMaster::Replaced)
            .unwrap();
        assert!(
            same.parts
                .actions_of(same.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .is_some()
        );
        world
            .upgrade_actions(place, (&old, &bare), UpgradeMaster::Replaced)
            .unwrap();
        assert!(
            world
                .parts
                .actions_of(world.players[1].objects[ListIndex::new(0)].id)
                .list
                .gather()
                .is_none()
        );
    }

    /// `FUN_14114cb80`: an object showing the old master's walking sprite shows the new one's,
    /// and keeps its sprite when the new master has none; one showing the running sprite shows
    /// the new running sprite; any other sprite stays.
    #[test]
    fn an_upgrade_moving_object_shows_the_new_walking_or_running_sprite() {
        let old = Master {
            id: master_id(4),
            moving: Some(MovingPart {
                move_sprite: Some(SpriteId::named(100)),
                run_sprite: Some(SpriteId::named(101)),
                ..MovingPart::default()
            }),
            ..Master::default()
        };
        let new = Master {
            id: master_id(5),
            moving: Some(MovingPart {
                move_sprite: Some(SpriteId::named(200)),
                run_sprite: Some(SpriteId::named(201)),
                ..MovingPart::default()
            }),
            ..Master::default()
        };
        let shown = |at: i16, new: &Master| {
            let mut unit = object(10, master_id(4), ObjectState::Alive, HitPoints::new(10.0));
            unit.sprite = Some(ActiveSprite::start(
                graphic(at),
                &mut Well512::default(),
                tuning().sprite,
            ));
            let mut world = one_player(vec![unit], vec![old.clone()], &[100, 101, 102, 200, 201]);
            world
                .upgrade_moving_sprite(
                    Place::Live(PlayerId::new(0), ListIndex::new(0)),
                    (&old, new),
                )
                .unwrap();
            world.players[0].objects[ListIndex::new(0)]
                .sprite
                .map(|active_sprite| active_sprite.graphic.id)
        };
        assert_eq!(shown(100, &new), Some(SpriteId::named(200)));
        assert_eq!(shown(101, &new), Some(SpriteId::named(201)));
        assert_eq!(shown(102, &new), Some(SpriteId::named(102)));
        let walkless = Master {
            moving: Some(MovingPart {
                move_sprite: None,
                ..new.moving.unwrap_or_default()
            }),
            ..new
        };
        assert_eq!(shown(100, &walkless), Some(SpriteId::named(100)));
    }

    /// `FUN_141186580` without the master kept: a combat object leaves its class's counter; a
    /// villager's load goes to its player (berries as food, wood as wood) and its hands empty;
    /// another combat object, an empty load or one of no kind stays.
    #[test]
    fn an_upgrade_hands_villager_load_to_its_player() {
        let villager = Master {
            id: ids::VILLAGER_MALE,
            kind: MasterKind::COMBAT,
            class: class::VILLAGER,
            hit_points: MasterHitPoints::of_data(25),
            ..Master::default()
        };
        let soldier = Master {
            id: master_id(4),
            class: class::CLASS_6,
            ..villager.clone()
        };
        let load = |master: &Master, kind, amount| {
            let mut object = object(10, master.id, ObjectState::Alive, HitPoints::new(25.0));
            (object.life.attribute_kind, object.life.amount) = (kind, amount);
            let mut world = one_player(vec![object], vec![master.clone()], &[]);
            world.players[0].attributes[37] = Amount::new(1.0);
            world
                .hand_in_load(
                    PlayerId::new(0),
                    Place::Live(PlayerId::new(0), ListIndex::new(0)),
                    master,
                )
                .unwrap();
            let unit = &world.players[0].objects[ListIndex::new(0)];
            (
                unit.life.held(),
                [
                    world.players[0].attributes[0].get(),
                    world.players[0].attributes[1].get(),
                ],
                world.players[0].attributes[37],
            )
        };
        assert_eq!(
            load(&villager, Some(AttributeId::named(17)), Amount::new(10.0)),
            (Held::default(), [10.0, 0.0], Amount::ZERO)
        );
        assert_eq!(
            load(&villager, Some(attribute::WOOD), Amount::new(6.0)),
            (Held::default(), [0.0, 6.0], Amount::ZERO)
        );
        assert_eq!(
            load(&villager, Some(attribute::WOOD), Amount::ZERO),
            (
                carrying(Some(attribute::WOOD), 0.0),
                [0.0, 0.0],
                Amount::ZERO
            )
        );
        assert_eq!(
            load(&villager, None, Amount::new(6.0)),
            (carrying(None, 6.0), [0.0, 0.0], Amount::ZERO)
        );
        assert_eq!(
            load(&soldier, Some(attribute::WOOD), Amount::new(6.0)).0,
            carrying(Some(attribute::WOOD), 6.0)
        );
    }

    /// The upgrade's sight: an object in state 2 on the map takes its sight again with its line
    /// of sight moved by the masters' change (a powerup of 1 kept: 5 to 7), its seer the new
    /// master's id unless the master is kept; one inside another or not in state 2 keeps it.
    #[test]
    fn an_upgrade_moves_the_line_of_sight_by_the_masters_change() {
        let old = Master {
            id: master_id(4),
            line_of_sight: Distance::new(4.0),
            ..Master::default()
        };
        let new = Master {
            id: master_id(5),
            line_of_sight: Distance::new(6.0),
            ..Master::default()
        };
        let sighted = |state: ObjectState, inside: Option<ObjectId>, keep: UpgradeMaster| {
            let mut unit = object(10, master_id(4), state, HitPoints::new(10.0));
            unit.place_for_test(Position::new(5.5, 5.5, 0.0));
            unit.life.inside = inside.map(ContainerId::new);
            let mut world = one_player(vec![unit], vec![old.clone()], &[]);
            world.spatial = Spatial::new(GridSize::new(20, 20));
            world.spatial.insert(
                object_id(10),
                Placed {
                    position: Position::new(5.5, 5.5, 0.0),
                    radius: Footprint::new(0.2, 0.2),
                    class: class::CLASS_6,
                    player: PlayerId::new(0),
                    master: master_id(4),
                    obstruction_kind: ObstructionKind::NONE,
                    buildable_on: BuildableOn::NO,
                    tall: true,
                    moves: false,
                    unknown: None,
                    obstruction: None,
                },
            );
            let map = || VisibleMap {
                counts: vec![0; 400],
                explored: TileCount::ZERO,
            };
            let mut sight = Visibility::new(
                GridSize::new(20, 20),
                vec![map()],
                vec![VisionBits::NONE; 400],
                Vec::new(),
                0,
                vec![PlayerKind::Human],
            );
            sight.add_seer(
                object_id(10),
                Seer {
                    player: PlayerId::new(0),
                    master: master_id(4),
                    los: Distance::new(5.0),
                    los_powerup: Distance::ZERO,
                    state: ObjectState::Alive,
                    explored: true,
                    building: None,
                    umv: 0,
                    capture: CaptureKind::None,
                    kind: MasterKind::COMBAT,
                },
            );
            world.spatial.sight = Some(sight);
            world
                .upgrade_sight(
                    Place::Live(PlayerId::new(0), ListIndex::new(0)),
                    (&old, &new),
                    keep,
                )
                .unwrap();
            let seer = world
                .spatial
                .sight
                .as_ref()
                .and_then(|visibility| visibility.seer(object_id(10)))
                .copied();
            seer.map(|seer| (seer.los, seer.master))
        };
        assert_eq!(
            sighted(ObjectState::Alive, None, UpgradeMaster::Replaced),
            Some((Distance::new(7.0), master_id(5)))
        );
        assert_eq!(
            sighted(ObjectState::Alive, None, UpgradeMaster::Kept),
            Some((Distance::new(7.0), master_id(4)))
        );
        assert_eq!(
            sighted(
                ObjectState::Alive,
                Some(object_id(99)),
                UpgradeMaster::Replaced
            ),
            Some((Distance::new(5.0), master_id(4)))
        );
        assert_eq!(
            sighted(ObjectState::DyingSprite, None, UpgradeMaster::Replaced),
            Some((Distance::new(5.0), master_id(4)))
        );
    }

    /// The upgrade's hit points: the share of the old maximum carries to the new one (none
    /// from a master of no hit points); the standing, second standing, dying and undead
    /// sprites shown become the new master's; a combat object of two charged masters takes
    /// the change of the maximum charge.
    #[test]
    fn an_upgrade_carries_the_hit_points_share_the_sprites_and_the_charge() {
        let old = Master {
            id: master_id(4),
            kind: MasterKind::COMBAT,
            hit_points: MasterHitPoints::of_data(40),
            standing_sprite: Some(SpriteId::named(300)),
            standing_sprite_2: Some(SpriteId::named(301)),
            death_sprite: Some(SpriteId::named(302)),
            undead_sprite: Some(SpriteId::named(303)),
            creatable: Some(CreatablePart {
                charge_kind: ChargeKind::ATTACK,
                max_charge: Charge::new(2.0),
                ..CreatablePart::default()
            }),
            ..Master::default()
        };
        let new = Master {
            id: master_id(5),
            hit_points: MasterHitPoints::of_data(60),
            standing_sprite: Some(SpriteId::named(400)),
            standing_sprite_2: None,
            death_sprite: Some(SpriteId::named(402)),
            undead_sprite: Some(SpriteId::named(403)),
            creatable: Some(CreatablePart {
                max_charge: Charge::new(5.0),
                ..old.clone().creatable.unwrap_or_default()
            }),
            ..old.clone()
        };
        let upgraded = |old: &Master, new: &Master, at: i16| {
            let mut object = object(
                10,
                master_id(4),
                ObjectState::DyingSprite,
                HitPoints::new(20.0),
            );
            object.sprite = Some(ActiveSprite::start(
                graphic(at),
                &mut Well512::default(),
                tuning().sprite,
            ));
            let graphics = [300, 301, 302, 303, 304, 400, 402, 403];
            let mut world = one_player(vec![object], vec![old.clone()], &graphics);
            world
                .upgrade_hit_points(Place::Live(PlayerId::new(0), ListIndex::new(0)), (old, new))
                .unwrap();
            let unit = &world.players[0].objects[ListIndex::new(0)];
            (
                unit.life.hp,
                unit.sprite.map(|active_sprite| active_sprite.graphic.id),
                world.parts.combat_of(unit.id).charge,
            )
        };
        assert_eq!(
            upgraded(&old, &new, 300),
            (
                HitPoints::new(30.0),
                Some(SpriteId::named(400)),
                Charge::new(3.0)
            )
        );
        // the second standing sprite becomes the new standing one when the new has none
        assert_eq!(upgraded(&old, &new, 301).1, Some(SpriteId::named(400)));
        assert_eq!(upgraded(&old, &new, 302).1, Some(SpriteId::named(402)));
        assert_eq!(upgraded(&old, &new, 303).1, Some(SpriteId::named(403)));
        assert_eq!(upgraded(&old, &new, 304).1, Some(SpriteId::named(304)));
        let none = Master {
            hit_points: MasterHitPoints::of_data(0),
            ..old.clone()
        };
        assert_eq!(upgraded(&none, &new, 304).0, HitPoints::ZERO);
        let uncharged = Master {
            creatable: Some(CreatablePart {
                charge_kind: ChargeKind::NONE,
                ..new.creatable.clone().unwrap_or_default()
            }),
            ..new
        };
        assert_eq!(upgraded(&old, &uncharged, 304).2, Charge::ZERO);
    }
}
