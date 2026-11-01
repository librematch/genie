//! An object's upgrade to another master: slot 0x98 (`FUN_141201a60` for a combat object,
//! `FUN_1411c4660` for a building), down the classes' overrides (`FUN_14107cb70`,
//! `FUN_14106a530`, `FUN_14114cb80`) to the static object's `FUN_141186580`, as the upgrade of
//! its master runs it (`FUN_141657d10`, [`World::upgrade_objects`]); and the damage sprites'
//! check (`FUN_1411859a0`), which the static part runs around the hit points' change.

use crate::GenieError;
use crate::combat::Charge;
use crate::math::rng::{Stream, Well512};
use crate::objects::HitPoints;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::ClassId;
use crate::objects::masters::DamageSpriteMode;
use crate::objects::masters::HpPercent;
use crate::objects::masters::MasterKind;
use crate::objects::masters::TaskId;
use crate::objects::masters::class;
use crate::objects::masters::{Master, Task, master_type, player_master};
use crate::objects::unit::Unit;
use crate::orders::Place;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute;
use crate::vision::SightTiles;
use crate::world::World;
use crate::world::slots::PlayerSlots;

/// A master's hit points this near 0 give no share (`ABS(x) <= 1.1920929e-07`).
const NO_HIT_POINTS: f32 = f32::EPSILON;
/// The whole of a damage share (`DAT_14330ebcc`, 100).
const WHOLE: f32 = crate::math::image::upgrade::WHOLE;
/// A sprite the data file does not have.
const NO_SPRITE: GenieError = GenieError::unsupported("a sprite the data file does not have");
/// The resource classes whose damage share is the share of their storage left from state 3 on
/// (`FUN_141193590`), and those whose share is it once dead (state above 2).
const RESOURCE_CLASSES: [ClassId; 11] = [
    class::CLASS_5,
    class::PREY,
    class::PREDATOR,
    class::CLASS_29,
    class::CLASS_31,
    class::CLASS_33,
    class::CLASS_38,
    class::CLASS_40,
    class::CLASS_41,
    class::LIVESTOCK,
    class::CLASS_61,
];
const RESOURCE_CLASSES_DEAD: [ClassId; 6] = [
    class::CLASS_7,
    class::CLASS_8,
    class::TREE,
    class::CLASS_32,
    class::CLASS_48,
    class::CLASS_63,
];
/// The class pairs whose change makes no new unit AI (`FUN_1415c6aa0`: the new class first).
const SAME_AI_CLASSES: [(ClassId, ClassId); 3] = [
    (class::CLASS_12, class::CLASS_47),
    (class::MONK, class::CLASS_43),
    (class::CLASS_43, class::MONK),
];

/// What a damage sprite apply mode does to the simulation (`FUN_1411859a0`'s switch on the
/// entry's `+0x9`).
const SPRITE_PLACED: DamageSpriteMode = DamageSpriteMode::PLACED;
const SPRITE_SCATTERED: DamageSpriteMode = DamageSpriteMode::SCATTERED;
const SPRITE_SHOWN: DamageSpriteMode = DamageSpriteMode::SHOWN;

/// Slot 0xa8 (`FUN_141201670` for a combat object and a building; the base's `FUN_14106a520`
/// answers 1): whether an object of a master of type `kind` in state `state` takes an upgrade
/// (below state 4) rather than a master of its own.
#[must_use]
pub fn upgradable(kind: MasterKind, state: ObjectState) -> bool {
    kind < master_type::COMBAT || state < ObjectState::DyingSprite
}

/// `FUN_141193590`: an object of `master` in `state` whose damage share is its storage's
/// (`+0x681` set, or a resource class: those of [`RESOURCE_CLASSES`] and
/// [`RESOURCE_CLASSES_DEAD`] in a state above 2).
fn storage_share(master: &Master, state: ObjectState) -> bool {
    master.auto_gather
        || ((RESOURCE_CLASSES.contains(&master.class)
            || RESOURCE_CLASSES_DEAD.contains(&master.class))
            && ObjectState::Alive < state)
}

/// The damage sprites' check (`FUN_1411859a0`) of object `u` reading `master`, whose damage share
/// (`+0xd`) is `before`: a master without damage sprites leaves it. Else the share is that of its
/// storage left for a resource object ([`storage_share`]: the first storage entry of the kind it
/// holds, `100 - amount * 100 / storage`, CVTTSS2SI; 0 for none), or of its hit points, which
/// first stay within 0 and the master's (COMISS: a NaN stays; a master below 1 hit point leaves
/// the share). A changed share below 100 that moves the object to another damage sprite (the
/// last entry whose percent lies below the share, the old one's below the old share) removes the
/// object's attachments and shows the new entry by its apply mode: mode 0 places it on the
/// object (`FUN_14118a9d0`, else a decal), mode 1 at a random spot of its footprint (two draws of
/// the Object stream, `CheckDamageSprites`, then a decal), mode 2 as the object's sprite (slot
/// 0x68); then the old entry goes: a decal of mode 0 or 1, or for mode 2 the master's standing
/// sprite (slots 0x798 and 0x68) unless the new entry also shows one. The attachments and decals
/// are drawn, not simulated. Returns the share (`+0xd`), which the slice does not hold.
///
/// # Errors
///
/// A sprite the data file does not have.
pub(crate) fn damage_sprites(
    u: &mut Unit,
    master: &Master,
    before: HpPercent,
    (graphics, object, sprite): (&crate::objects::Graphics, &mut Well512, &mut Well512),
) -> Result<HpPercent, GenieError> {
    let entries = &master.damage_graphics;
    let Some((share, hp)) = share_of(u, master) else {
        return Ok(before);
    };
    u.life.hp = HitPoints::new(hp);
    if share == before || HpPercent::WHOLE <= share {
        return Ok(share);
    }
    let (mut now, mut was) = (None, None);
    for (i, e) in entries.iter().enumerate() {
        if e.percent < share {
            now = Some(i);
        }
        if e.percent < before {
            was = Some(i);
        }
    }
    if now == was {
        return Ok(share);
    }
    let shown = |i: Option<usize>| {
        i.and_then(|i| entries.get(i).copied())
            .filter(|e| e.sprite.is_some())
    };
    let graphic =
        |id: Option<SpriteId>| id.and_then(|id| graphics.get(id)).copied().ok_or(NO_SPRITE);
    if let Some(e) = shown(now) {
        match e.apply_mode {
            SPRITE_SCATTERED => {
                let _ = (object.rand15(), object.rand15());
            }
            SPRITE_SHOWN if !graphics.is_empty() => {
                if u.sprite.is_some() {
                    crate::objects::sprite::new_sprite(
                        &mut u.sprite,
                        u.body.move_stats_mut(),
                        graphic(e.sprite)?,
                        sprite,
                    );
                } else {
                    let _ = crate::objects::sprite::ActiveSprite::start(graphic(e.sprite)?, sprite);
                }
            }
            _ => {}
        }
    }
    if let Some(e) = shown(was)
        && e.apply_mode == SPRITE_SHOWN
        && shown(now).is_none_or(|n| n.apply_mode != SPRITE_SHOWN)
        && !graphics.is_empty()
    {
        let standing = graphic(master.standing_sprite)?;
        if u.sprite.is_some() {
            crate::objects::sprite::new_sprite(
                &mut u.sprite,
                u.body.move_stats_mut(),
                standing,
                sprite,
            );
        } else {
            let _ = crate::objects::sprite::ActiveSprite::start(standing, sprite);
        }
    }
    let _ = SPRITE_PLACED;
    Ok(share)
}

/// The damage share [`damage_sprites`] finds for object `u` under `master`, with its hit points
/// after it (within 0 and the master's for a share of hit points); None where it leaves the
/// share (no damage sprites, or a master below 1 hit point).
fn share_of(u: &Unit, master: &Master) -> Option<(HpPercent, f32)> {
    if master.damage_graphics.is_empty() {
        return None;
    }
    let mut hp = u.life.hp.get();
    if storage_share(master, u.life.state) {
        let held = master
            .storage
            .iter()
            .take(3)
            .find(|&&(kind, _, _)| kind == u.life.attribute_type);
        let share = held.map_or(HpPercent::of_data(0), |&(_, amount, _)| {
            HpPercent::of_data(crate::math::conversions::cvttss2si(
                WHOLE - u.life.amount.get() * WHOLE / amount.get(),
            ) as u8)
        });
        return Some((share, hp));
    }
    if master.hit_points.get() < 1 {
        return None;
    }
    let most = HitPoints::of_master(master.hit_points).get();
    // COMISS hp, most; JBE: a NaN keeps; then COMISS 0, hp; JBE
    if most < hp {
        hp = most;
    } else if hp < 0.0 {
        hp = 0.0;
    }
    Some((
        HpPercent::of_data(crate::math::conversions::cvttss2si(WHOLE - hp * WHOLE / most) as u8),
        hp,
    ))
}

/// The damage share (`+0xd`) object `u` holds under `master`, which every change of its hit
/// points or master set through [`damage_sprites`] (the slice does not hold it); 0 where that
/// check would leave it.
fn damage_share_of(u: &Unit, master: &Master) -> HpPercent {
    share_of(u, master).map_or(HpPercent::of_data(0), |(s, _)| s)
}

/// `FUN_1415c6aa0` on player `p`'s masters `new` and `old` (by id): the unit's new class needs
/// another unit AI (`FUN_141215a90`) when the classes differ, but for the pairs of
/// [`SAME_AI_CLASSES`].
fn class_changes(new: Option<&Master>, old: Option<&Master>) -> bool {
    match (new, old) {
        (Some(n), Some(o)) => n.class != o.class && !SAME_AI_CLASSES.contains(&(n.class, o.class)),
        _ => false,
    }
}

impl World {
    /// Slot 0x98 of player `p`'s object at `place`, whose master `old` upgrades to `new`; with
    /// `keep` (the upgrade's modes −1 and 3, whose master then copies `new`) its master stays.
    /// In the game's order, outermost class first:
    ///
    /// - a building (`FUN_1411c4660`) showing `old`'s construction sprite shows `new`'s (slot
    ///   0x68); its snow decal swaps between a push and a pop of the Object stream (drawn);
    /// - a combat object (`FUN_141201a60`) takes the change of the missiles per volley
    ///   (`+0xa00`); its garrison decal swaps (drawn);
    /// - a combat object (`FUN_14107cb70`) showing `old`'s attack sprite shows `new`'s; slot
    ///   0x8a8 rebuilds what the object keeps of `new`'s tasks, which the slice reads from the
    ///   master;
    /// - an action object (`FUN_14106a530`): each action takes `new`'s task of its type, class
    ///   and target (`FUN_140a97970`, [`World::actions_take_master`]); without `keep` a top
    ///   action of a type `new` has no task for stops the object (slot 0x148);
    /// - a moving object (`FUN_14114cb80`) showing `old`'s walking or running sprite shows
    ///   `new`'s;
    /// - the static object (`FUN_141186580`): without `keep` a combat object leaves its class
    ///   counters ([`crate::objects::lifecycle::death_parts::count_class`] with −1) and a villager's load goes to its
    ///   player (slot 0xf8 with 1 and 1, the foods as food; the statistics are not
    ///   simulation); on the map in state 2 its sight goes and comes with `new`'s line of sight
    ///   (slots 0x1d8 and 0x1d0); its hit points take their share of `new`'s (`old`'s through
    ///   the damage sprites at full hit points first, [`damage_sprites`]); showing `old`'s
    ///   standing, second standing, dying or undead sprite it shows `new`'s; a combat object of
    ///   two masters with charges takes the change of the maximum (`+0x9e0`); in state 2 the
    ///   damage sprites again with `new`; without `keep` the master is `new` and the class
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
        p: PlayerId,
        place: Place,
        (old, new): (&Master, &Master),
        keep: bool,
    ) -> Result<(), GenieError> {
        let kind = old.kind;
        if master_type::BUILDING <= kind {
            let (from, to) = (
                old.building.as_ref().and_then(|b| b.construction_sprite),
                new.building.as_ref().and_then(|b| b.construction_sprite),
            );
            self.show_if(place, from, to)?;
        }
        let class_changed = master_type::COMBAT <= kind
            && class_changes(
                player_master(&self.masters, p, new.id),
                player_master(&self.masters, p, old.id),
            );
        if master_type::COMBAT <= kind {
            let u = self.object_at_mut(place)?;
            u.firepower += new.volley_fire - old.volley_fire;
        }
        if master_type::RGE_COMBAT <= kind && new.attack_sprite.is_some() {
            self.show_if(place, old.attack_sprite, new.attack_sprite)?;
        }
        if master_type::ACTION <= kind {
            self.upgrade_actions(place, (old, new), keep)?;
        }
        if master_type::MOVING <= kind {
            self.upgrade_moving_sprite(place, (old, new))?;
        }
        // the static object's part
        if !keep {
            self.hand_in_load(p, place, old)?;
        }
        self.upgrade_sight(place, (old, new), keep)?;
        self.upgrade_hit_points(place, (old, new))?;
        if !keep {
            self.object_at_mut(place)?.life.master_id = new.id;
            if master_type::COMBAT <= kind {
                crate::objects::lifecycle::death_parts::count_class(
                    &mut self.players.player_mut(p)?.attributes,
                    new,
                    1.0,
                );
            }
        }
        if class_changed {
            self.new_unit_ai(p, place, new)?;
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
        keep: bool,
    ) -> Result<(), GenieError> {
        self.actions_take_master(place, old, new)?;
        if !keep
            && let Some(top) = self.object_at(place)?.top_action_kind()
            && !new.tasks.iter().any(|t| t.action_type == top)
            && let Place::Live(q, i) = place
        {
            self.object_stop(q, i)?;
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
    ) -> Result<(), GenieError> {
        let shows = |s: Option<SpriteId>| {
            self.object_at(place)
                .ok()
                .and_then(|u| u.sprite)
                .is_some_and(|a| Some(a.graphic.id) == s)
        };
        if shows(old.move_sprite) {
            if let Some(s) = new.move_sprite {
                self.show(place, s)?;
            }
        } else if shows(old.run_sprite)
            && let Some(s) = new.run_sprite
        {
            self.show(place, s)?;
        }
        Ok(())
    }

    /// The static object's part of an upgrade from `old` without its master kept
    /// (`FUN_141186580`): a combat object leaves its class counters
    /// ([`crate::objects::lifecycle::death_parts::count_class`] with −1) and a villager's load
    /// goes to its player (slot 0xf8 with 1 and 1, the foods as food).
    fn hand_in_load(&mut self, p: PlayerId, place: Place, old: &Master) -> Result<(), GenieError> {
        let kind = old.kind;
        if master_type::COMBAT <= kind {
            crate::objects::lifecycle::death_parts::count_class(
                &mut self.players.player_mut(p)?.attributes,
                old,
                -1.0,
            );
        }
        let u = self.object_at(place)?;
        if master_type::COMBAT <= kind
            // The villager class (4), whose load an upgrade without its master kept goes to its player.
            && old.class == class::VILLAGER
            && Amount::ZERO < u.life.amount
            && let Some(kind) = u.life.attribute_type
        {
            // the foods a unit carries (15 to 17) as food
            let kind = if kind.is_food_load() {
                attribute::FOOD
            } else {
                kind
            };
            let amount = u.life.amount;
            let mut adds = Vec::new();
            crate::gathering::gather::unit::give_load(
                (&self.spatial.relations, self.players.player(p)?.sharing),
                p,
                &mut adds,
                kind,
                amount,
            )?;
            for (k, v) in adds {
                self.players.player_mut(p)?.add_attribute(k, v)?;
            }
            let u = self.object_at_mut(place)?;
            u.life.attribute_type = None;
            u.life.amount = Amount::ZERO;
        }
        Ok(())
    }

    /// An upgrade's sight (`FUN_141186580`): an object in state 2 on the map, not inside
    /// another, takes its sight off and on again with its line of sight moved by the change
    /// from `old`'s to `new`'s (slots 0x1d8 and 0x1d0); without `keep` its seer takes `new`'s id.
    fn upgrade_sight(
        &mut self,
        place: Place,
        (old, new): (&Master, &Master),
        keep: bool,
    ) -> Result<(), GenieError> {
        let id = self.object_at(place)?.id;
        let u = self.object_at(place)?;
        if u.life.state == ObjectState::Alive
            && u.life.inside.is_none()
            && let Some(placed) = self.spatial.get(id).copied()
            && let Some(sight) = self.spatial.sight.as_mut()
        {
            let now = sight.seer(id).map_or(old.line_of_sight, |s| s.los);
            let los = (now - old.line_of_sight) + new.line_of_sight;
            sight.sight_off(id, placed.position, placed.radius, false, false)?;
            let _ = sight.sight_on_los(
                id,
                placed.position,
                placed.radius,
                false,
                SightTiles::of(los),
                false,
            )?;
            if let Some(s) = sight.seer_mut(id) {
                s.los = los;
                if !keep {
                    s.master = new.id;
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
    ) -> Result<(), GenieError> {
        let kind = old.kind;
        let u = self.object_at(place)?;
        let most = HitPoints::of_master(old.hit_points).get();
        let share = if most.abs() <= NO_HIT_POINTS {
            0.0
        } else {
            u.life.hp.get() / most
        };
        let before = damage_share_of(u, old);
        let state = u.life.state;
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
        let shown = self.object_at(place)?.sprite.map(|s| s.graphic.id);
        let next = sprites
            .iter()
            .find(|&&(from, _)| from.is_some() && shown == from)
            .map(|&(_, to)| to);
        if master_type::COMBAT <= kind
            && master_type::COMBAT <= new.kind
            && old.charge_type.charges()
            && new.charge_type.charges()
        {
            let more = (new.max_charge - old.max_charge).abs();
            if Charge::ZERO < more {
                self.object_at_mut(place)?.charge += more;
            }
        }
        self.object_at_mut(place)?.life.hp =
            HitPoints::new(HitPoints::of_master(new.hit_points).get() * share);
        if let Some(s) = next.flatten() {
            self.show(place, s)?;
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
    ) -> Result<HpPercent, GenieError> {
        let graphics = std::mem::take(&mut self.graphics);
        let mut object = std::mem::take(self.streams.get(Stream::Object));
        let mut sprite = std::mem::take(self.streams.get(Stream::Sprite));
        let u = self.object_at_mut(place)?;
        let r = damage_sprites(u, master, before, (&graphics, &mut object, &mut sprite));
        self.graphics = graphics;
        *self.streams.get(Stream::Object) = object;
        *self.streams.get(Stream::Sprite) = sprite;
        r
    }

    /// Slot 0x68 on the object at `place` with sprite `to` when it shows `from` (both sprites).
    fn show_if(
        &mut self,
        place: Place,
        from: Option<SpriteId>,
        to: Option<SpriteId>,
    ) -> Result<(), GenieError> {
        let shows = self
            .object_at(place)?
            .sprite
            .is_some_and(|s| Some(s.graphic.id) == from);
        if shows && let Some(to) = to {
            self.show(place, to)?;
        }
        Ok(())
    }

    /// Slot 0x68 on the object at `place` with sprite `id`: a new instance unless it shows it
    /// already ([`crate::objects::sprite::new_sprite`] on the Sprite stream). Without the graphics (no
    /// data file) the Sprite stream is not modelled.
    fn show(&mut self, place: Place, id: SpriteId) -> Result<(), GenieError> {
        if self.graphics.is_empty() {
            return Ok(());
        }
        let g = self.graphics.get(id).copied().ok_or(NO_SPRITE)?;
        let rng = self.streams.get(Stream::Sprite);
        let u = match place {
            Place::Live(q, i) => self.players.live_mut(q, i)?,
            Place::Sleeping(q, i) => self.players.asleep_mut(q, i)?,
        };
        crate::objects::sprite::new_sprite(&mut u.sprite, u.body.move_stats_mut(), g, rng);
        Ok(())
    }

    /// `FUN_141215a90` on the object at `place`, whose master became `new`: its unit AI goes and
    /// the factory's type for `new` makes another ([`crate::unit_ai::factory_type`],
    /// [`crate::unit_ai::UnitAi::constructed`] on the AI stream), none for a class without.
    fn new_unit_ai(&mut self, p: PlayerId, place: Place, new: &Master) -> Result<(), GenieError> {
        let position = self.object_at(place)?.position();
        let made = self.new_object(p, new.id, new, position)?;
        let ai = new.unit_ai_type(new.id).and_then(|t| {
            crate::unit_ai::UnitAi::constructed(t, &made, self.streams.get(Stream::Ai))
        });
        self.object_at_mut(place)?.ai = ai;
        Ok(())
    }

    /// `FUN_140a97970` -> `FUN_140a78150` for each action of the object at `place` whose master
    /// `old` gives way to `new`: an action with a task takes the last of `new`'s tasks of the same
    /// action type, target class and target master (none: it keeps its task and changes nothing
    /// else); then its slot 0x50 with the tasks: an Attack's sprites follow `new`'s
    /// ([`crate::technology::effect::follow_master`]), its sub-actions alike (`FUN_140a979b0`).
    pub(crate) fn actions_take_master(
        &mut self,
        place: Place,
        old: &Master,
        new: &Master,
    ) -> Result<(), GenieError> {
        let find = |task: &Task| {
            new.tasks
                .iter()
                .rev()
                .find(|t| {
                    t.action_type == task.action_type
                        && t.object_group == task.object_group
                        && t.object_id == task.object_id
                })
                .copied()
        };
        let old_task = |id: TaskId| old.tasks.iter().find(|t| t.id == id).copied();
        let graphics = &self.graphics;
        let g = |id: Option<SpriteId>| id.and_then(|s| graphics.get(s).copied());
        let sprites = crate::technology::effect::MasterSprites {
            moving: g(new.move_sprite),
            attack: g(new.attack_sprite),
            standing: g(new.standing_sprite),
            idle_attack: g(new.idle_attack_sprite),
            second: g(new.attack_sprite2),
            combat: master_type::COMBAT <= new.kind,
        };
        let u = match place {
            Place::Live(q, i) => self.players.live_mut(q, i)?,
            Place::Sleeping(q, i) => self.players.asleep_mut(q, i)?,
        };
        if let Some(attack) = u.actions.attack_mut() {
            crate::technology::effect::follow_master(&mut attack.arms, None, &sprites);
        }
        if let Some(hunt) = u.actions.hunt_mut()
            && let Some(t) = old_task(hunt.task.id).as_ref().and_then(find)
        {
            hunt.task = crate::gathering::hunt::HuntTask::of(&t);
            let task = hunt.task.sprites;
            if let Some(attack) = hunt.attack.as_mut() {
                crate::technology::effect::follow_master(&mut attack.arms, Some(task), &sprites);
            }
        }
        if let Some(gather) = u.actions.gather_mut()
            && let Some(t) = old_task(gather.task.id).as_ref().and_then(find)
        {
            gather.task = crate::gathering::hunt::HuntTask::of(&t);
        }
        if let Some(make) = u.actions.make_mut()
            && let Some(t) = find(&make.task)
        {
            make.task = t;
        }
        if let Some(fly) = u.actions.fly_mut()
            && let Some(t) = find(&fly.task)
        {
            fly.task = t;
        }
        if let Some(w) = u.actions.wait_solidify_mut()
            && let Some(t) = find(&w.task)
        {
            w.task = t;
        }
        if let Some(r) = u.actions.repair_mut()
            && let Some(t) = old_task(r.task.id).as_ref().and_then(find)
        {
            r.task = crate::construction::repair::RepairTask::of(&t);
        }
        if let Some(b) = u.actions.build_mut()
            && let Some(t) = old_task(b.task.id).as_ref().and_then(find)
        {
            b.task = crate::construction::build::BuildTask::of(&t, b.task.work_rate);
        }
        Ok(())
    }

    /// The object at `place`.
    ///
    /// # Errors
    ///
    /// A place the world does not hold.
    pub(crate) fn object_at(&self, place: Place) -> Result<&Unit, GenieError> {
        match place {
            Place::Live(q, i) => self.players.live(q, i),
            Place::Sleeping(q, i) => self.players.asleep(q, i),
        }
    }

    /// The object at `place`, to change.
    ///
    /// # Errors
    ///
    /// A place the world does not hold.
    pub(crate) fn object_at_mut(&mut self, place: Place) -> Result<&mut Unit, GenieError> {
        match place {
            Place::Live(q, i) => self.players.live_mut(q, i),
            Place::Sleeping(q, i) => self.players.asleep_mut(q, i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::wait_solidify::WaitSolidifyState;
    use crate::garrison::ContainerId;
    use crate::gathering::make::MakeState;
    use crate::map::Distance;
    use crate::map::Footprint;
    use crate::map::GridSize;
    use crate::map::Position;
    use crate::movement::Speed;
    use crate::objects::ListIndex;
    use crate::objects::MasterHitPoints;
    use crate::objects::ObjectId;
    use crate::objects::SpeedMultiplier;
    use crate::objects::SpriteFlags;
    use crate::objects::WorkRate;
    use crate::objects::lifecycle::Held;
    use crate::objects::masters::BuildableOn;
    use crate::objects::masters::ChargeType;
    use crate::objects::masters::ObstructionType;
    use crate::objects::masters::TaskSprites;
    use crate::objects::masters::action_type;
    use crate::objects::masters::{BuildingMaster, DamageGraphic, Masters};
    use crate::objects::masters::{MasterId, ids};
    use crate::objects::sprite::Graphic;
    use crate::ownership::CaptureKind;
    use crate::players::attribute;
    use crate::players::attribute::AttributeId;
    use crate::players::tracker::ListKey;
    use crate::technology::UpgradeMode;
    use crate::test_ids::carrying;
    use crate::test_ids::class_id;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::test_ids::opt_master;
    use crate::vision::TileCount;
    use crate::vision::VisionBits;
    use crate::world::Millis;
    use crate::world::Seconds;
    use crate::world::WorldTime;

    /// An animated graphic of `frames` frames with a random first frame (flags 5).
    const fn graphic(id: i16) -> Graphic {
        Graphic {
            id: SpriteId::named(id),
            frames: crate::objects::Frame::of_data(10),
            facets: crate::objects::FacetCount::of_data(1),
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
            kind: master_type::BUILDING,
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
            damage_sprites: true,
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
            life: crate::objects::lifecycle::Lifecycle {
                master_id: master,
                state,
                hp,
                ..crate::objects::lifecycle::Lifecycle::default()
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
        let graphics: crate::objects::Graphics = [70, 1070, 2095, 2120, 2145]
            .map(|id| (SpriteId::named(id), graphic(id)))
            .into_iter()
            .collect();
        let (mut object, mut sprite) = (Well512::default(), Well512::default());
        let bare = Master {
            hit_points: MasterHitPoints::of_data(100),
            ..Master::default()
        };
        let mut u = object_of(HitPoints::new(150.0));
        let r = damage_sprites(
            &mut u,
            &bare,
            HpPercent::of_data(7),
            (&graphics, &mut object, &mut sprite),
        );
        assert_eq!(
            (r, u.life.hp),
            (Ok(HpPercent::of_data(7)), HitPoints::new(150.0))
        );
        let placed = building(70, 100, SPRITE_PLACED, -1);
        let r = damage_sprites(
            &mut u,
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
        );
        assert_eq!(
            (r, u.life.hp),
            (Ok(HpPercent::of_data(0)), HitPoints::new(100.0))
        );
        let mut low = object_of(HitPoints::new(-5.0));
        let r = damage_sprites(
            &mut low,
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
        );
        assert_eq!(
            (r, low.life.hp),
            (Ok(HpPercent::of_data(100)), HitPoints::ZERO)
        );
        let mut nan = object_of(HitPoints::new(f32::NAN));
        let _ = damage_sprites(
            &mut nan,
            &placed,
            HpPercent::of_data(0),
            (&graphics, &mut object, &mut sprite),
        );
        assert!(nan.life.hp.get().is_nan());
        // mode 0: nothing drawn
        let before = object.clone();
        let mut hurt = object_of(HitPoints::new(40.0));
        assert_eq!(
            damage_sprites(
                &mut hurt,
                &placed,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite)
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(object, before);
        // mode 1: two draws of the Object stream
        let scattered = building(70, 100, SPRITE_SCATTERED, -1);
        let mut expected = object.clone();
        let _ = (expected.rand15(), expected.rand15());
        let mut hurt = object_of(HitPoints::new(40.0));
        assert_eq!(
            damage_sprites(
                &mut hurt,
                &scattered,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite)
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(object, expected);
        // the same entry (25 < 30 and 40, below 50): nothing more
        let mut hurt = object_of(HitPoints::new(60.0));
        assert_eq!(
            damage_sprites(
                &mut hurt,
                &scattered,
                HpPercent::of_data(30),
                (&graphics, &mut object, &mut sprite)
            ),
            Ok(HpPercent::of_data(40))
        );
        assert_eq!(object, expected);
        // mode 2: the entry's sprite, then the standing one when it goes
        let shown = building(70, 100, SPRITE_SHOWN, -1);
        let mut u = Unit {
            sprite: Some(crate::objects::sprite::ActiveSprite::start(
                graphic(1070),
                &mut sprite,
            )),
            ..object_of(HitPoints::new(40.0))
        };
        assert_eq!(
            damage_sprites(
                &mut u,
                &shown,
                HpPercent::of_data(0),
                (&graphics, &mut object, &mut sprite)
            ),
            Ok(HpPercent::of_data(60))
        );
        assert_eq!(u.sprite.map(|s| s.graphic.id), Some(SpriteId::named(2120)));
        u.life.hp = HitPoints::new(100.0);
        assert_eq!(
            damage_sprites(
                &mut u,
                &shown,
                HpPercent::of_data(60),
                (&graphics, &mut object, &mut sprite)
            ),
            Ok(HpPercent::of_data(0))
        );
        assert_eq!(u.sprite.map(|s| s.graphic.id), Some(SpriteId::named(1070)));
    }

    fn object_of(hp: HitPoints) -> Unit {
        object(1, ids::HOUSE, ObjectState::Alive, hp)
    }

    /// `FUN_141193590`: a dead tree's share is that of its storage left (the entry of the kind
    /// it holds; none, 0), a living one's that of its hit points.
    #[test]
    fn a_resource_object_s_damage_share_is_its_storage_s() {
        let tree = Master {
            class: class::TREE,
            hit_points: MasterHitPoints::of_data(20),
            storage: vec![(Some(attribute::WOOD), Amount::new(100.0), 0)],
            damage_graphics: vec![DamageGraphic {
                sprite: None,
                percent: HpPercent::of_data(50),
                apply_mode: DamageSpriteMode::of_data(0),
            }],
            ..Master::default()
        };
        let mut u = object(1, master_id(1), ObjectState::Dying, HitPoints::new(20.0));
        (u.life.attribute_type, u.life.amount) = (Some(attribute::WOOD), Amount::new(25.0));
        assert_eq!(share_of(&u, &tree), Some((HpPercent::of_data(75), 20.0)));
        u.life.attribute_type = Some(attribute::STONE);
        assert_eq!(share_of(&u, &tree), Some((HpPercent::of_data(0), 20.0)));
        u.life.state = ObjectState::Alive;
        u.life.hp = HitPoints::new(5.0);
        assert_eq!(share_of(&u, &tree), Some((HpPercent::of_data(75), 5.0)));
    }

    /// Slot 0xa8 and `FUN_1415c6aa0`: a combat object or building takes an upgrade below state
    /// 4, any other object always; a class change makes a new unit AI but for the pairs 12/47,
    /// 18/43 and 43/18 (the new class first).
    #[test]
    fn who_takes_an_upgrade_and_who_a_new_unit_ai() {
        assert!(upgradable(master_type::BUILDING, ObjectState::Dying));
        assert!(!upgradable(master_type::BUILDING, ObjectState::DyingSprite));
        assert!(!upgradable(master_type::COMBAT, ObjectState::Removed));
        assert!(upgradable(master_type::ACTION, ObjectState::Removed));
        let m = |class| Master {
            class,
            ..Master::default()
        };
        assert!(!class_changes(
            Some(&m(class::BUILDING)),
            Some(&m(class::BUILDING))
        ));
        assert!(class_changes(
            Some(&m(class::CLASS_6)),
            Some(&m(class::CLASS_12))
        ));
        assert!(!class_changes(
            Some(&m(class::CLASS_12)),
            Some(&m(class::CLASS_47))
        ));
        assert!(class_changes(
            Some(&m(class::CLASS_47)),
            Some(&m(class::CLASS_12))
        ));
        assert!(!class_changes(
            Some(&m(class::MONK)),
            Some(&m(class::CLASS_43))
        ));
        assert!(!class_changes(
            Some(&m(class::CLASS_43)),
            Some(&m(class::MONK))
        ));
        assert!(!class_changes(None, Some(&m(class::BUILDING))));
    }

    /// The Feudal Age's upgrade of the house (70 to 464, mode −1, `FUN_141657d10`): a house in
    /// state 2 keeps its master and its share of the hit points (275 of 550: 450 of 900), and
    /// shows the new standing sprite; a dead one (state 4) and the rubble of its old death spawn
    /// (1403, whose copy differs from 464's 1435) take masters of their own, the dead one also
    /// its death spawn's; the player's master 70 then takes 464's values but its id and class.
    #[test]
    fn an_age_s_upgrade_keeps_a_house_s_share_of_its_hit_points() {
        let old = Master {
            death_spawns: [Some(master_id(1403)), None],
            ..building(70, 550, SPRITE_PLACED, 1403)
        };
        let new = Master {
            class: class::BUILDING,
            ..building(464, 900, SPRITE_PLACED, 1435)
        };
        let rubble = |id: i16, copy: i16| Master {
            id: master_id(id),
            copy_id: opt_master(copy),
            kind: master_type::STATIC,
            ..Master::default()
        };
        let mut house = object(10, ids::HOUSE, ObjectState::Alive, HitPoints::new(275.0));
        let mut sprites = Well512::default();
        house.sprite = Some(crate::objects::sprite::ActiveSprite::start(
            graphic(1070),
            &mut sprites,
        ));
        let player = crate::world::Player {
            objects: vec![
                house,
                object(11, ids::HOUSE, ObjectState::DyingSprite, HitPoints::ZERO),
                object(12, master_id(1403), ObjectState::Alive, HitPoints::new(1.0)),
            ]
            .into(),
            ..crate::world::Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        t.masters = vec![Masters::new([
            old,
            new,
            rubble(1403, 1403),
            rubble(1435, 1435),
        ])];
        t.graphics = [1070, 1464, 3070, 3464]
            .map(|id| (SpriteId::named(id), graphic(id)))
            .into_iter()
            .collect();
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(70),
            master_id(464),
            UpgradeMode::KEEP,
            None,
        )
        .unwrap();
        let [house, dead, rubble] = t.players[0].objects.as_slice() else {
            panic!("three objects");
        };
        assert_eq!(
            (house.life.master_id, house.life.hp),
            (master_id(70), HitPoints::new(450.0))
        );
        assert_eq!(
            house.sprite.map(|s| s.graphic.id),
            Some(SpriteId::named(1464))
        );
        assert!(house.own_master.is_none());
        assert_eq!(
            dead.own_master.as_deref().map(|m| m.hit_points),
            Some(MasterHitPoints::of_data(550))
        );
        assert_eq!(
            dead.dead_master.as_deref().map(|m| m.id),
            Some(master_id(1403))
        );
        assert_eq!(
            rubble.own_master.as_deref().map(|m| m.id),
            Some(master_id(1403))
        );
        let master = t.masters[0].get(ids::HOUSE).unwrap();
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
            kind: master_type::COMBAT,
            class: class::VILLAGER,
            hit_points: MasterHitPoints::of_data(25),
            volley_fire: crate::combat::Firepower::new(1.0),
            ..Master::default()
        };
        let new = Master {
            id: ids::ID_6,
            kind: master_type::COMBAT,
            class: class::CLASS_6,
            hit_points: MasterHitPoints::of_data(50),
            volley_fire: crate::combat::Firepower::new(3.0),
            ..Master::default()
        };
        let mut v = object(
            10,
            ids::VILLAGER_MALE,
            ObjectState::Alive,
            HitPoints::new(25.0),
        );
        (v.life.attribute_type, v.life.amount) =
            (Some(AttributeId::numbered(17)), Amount::new(10.0));
        v.firepower = crate::combat::Firepower::new(1.0);
        let player = crate::world::Player {
            objects: vec![v].into(),
            attributes: vec![Amount::ZERO; 300],
            ..crate::world::Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        t.masters = vec![Masters::new([old, new])];
        t.players[0].attributes[37] = Amount::new(1.0);
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(83),
            master_id(6),
            UpgradeMode::WALK,
            None,
        )
        .unwrap();
        let v = &t.players[0].objects[ListIndex::new(0)];
        assert_eq!(
            (v.life.master_id, v.life.hp, v.firepower),
            (
                master_id(6),
                HitPoints::new(50.0),
                crate::combat::Firepower::new(3.0)
            )
        );
        assert_eq!(v.life.held(), carrying(None, 0.0));
        let a = &t.players[0].attributes;
        assert_eq!((a[37], a[40]), (Amount::ZERO, Amount::new(1.0)));
        assert_eq!(t.players[0].attributes[0], Amount::new(10.0));
        assert_eq!(
            t.masters[0].get(ids::VILLAGER_MALE).map(|m| m.class),
            Some(class::VILLAGER)
        );
    }

    /// `FUN_140a97970` -> `FUN_140a78150`: each action finds its old task's match among the new
    /// master's tasks (action type, target class and master; the last of equals) and takes it; an
    /// action without a match keeps its task.
    #[test]
    fn an_upgrade_s_actions_take_the_new_master_s_last_matching_task() {
        use crate::construction::build::BuildTask;
        use crate::gathering::gather::GATHER;
        use crate::gathering::hunt::HuntTask;
        use crate::objects::masters::Task;
        let task = |id, action_type, object_group, work_range| Task {
            id,
            action_type,
            object_group,
            work_range,
            work_val1: crate::objects::masters::WorkValue::new((work_range * 2.0).get()),
            attribute_type: Some(AttributeId::numbered(17)),
            ..Task::default()
        };
        let mut t = crate::world::tests::forage_world();
        t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
            .unwrap();
        let mut old = t.masters[1].get(master_id(120)).unwrap().clone();
        old.tasks.push(task(
            TaskId::test(3),
            action_type::BUILD,
            Some(class_id(3)),
            Distance::new(0.25),
        ));
        let make = task(
            TaskId::test(4),
            crate::gathering::make::MAKE,
            None,
            Distance::ZERO,
        );
        let waiting = task(
            TaskId::test(9),
            crate::construction::wait_solidify::WAIT_SOLIDIFY,
            Some(class_id(3)),
            Distance::ZERO,
        );
        let new = Master {
            id: master_id(121),
            tasks: vec![
                task(
                    TaskId::test(5),
                    GATHER,
                    Some(class_id(7)),
                    Distance::new(0.5),
                ),
                task(
                    TaskId::test(6),
                    GATHER,
                    Some(class_id(7)),
                    Distance::new(0.75),
                ),
                Task {
                    sprites: TaskSprites::of_data([12, -1, -1, -1]),
                    ..task(
                        TaskId::test(7),
                        action_type::BUILD,
                        Some(class_id(3)),
                        Distance::new(1.5),
                    )
                },
                task(
                    TaskId::test(8),
                    crate::gathering::make::MAKE,
                    None,
                    Distance::new(2.0),
                ),
            ],
            ..old.clone()
        };
        let u = &mut t.players[1].objects[ListIndex::new(0)];
        let build = BuildTask {
            range: Distance::new(0.25),
            work_value: crate::objects::masters::WorkValue::new(0.5),
            work_rate: WorkRate::new(1.0),
            resource: Some(AttributeId::numbered(17)),
            id: TaskId::test(3),
            moving: None,
        };
        u.actions
            .set_build(Some(crate::construction::build::tests::started(
                object_id(10028),
                Position::new(30.0, 88.0, 0.0),
                Distance::new(1.0),
                build,
            )));
        u.actions.set_make(Some(crate::gathering::make::MakeAction {
            state: MakeState::New,
            remaining: Amount::ZERO,
            total: Amount::ZERO,
            task: make,
        }));
        u.actions
            .set_wait_solidify(Some(crate::construction::wait_solidify::WaitSolidify {
                state: WaitSolidifyState::Waiting,
                task: waiting,
                target: object_id(10028),
            }));
        t.actions_take_master(Place::Live(PlayerId::new(1), ListIndex::new(0)), &old, &new)
            .unwrap();
        let u = &t.players[1].objects[ListIndex::new(0)];
        assert_eq!(
            u.actions.gather().copied().unwrap().task,
            HuntTask::of(&new.tasks[1])
        );
        assert_eq!(
            u.actions.build().copied().unwrap().task,
            BuildTask {
                range: Distance::new(1.5),
                work_value: crate::objects::masters::WorkValue::new(3.0),
                work_rate: WorkRate::new(1.0),
                resource: Some(AttributeId::numbered(17)),
                id: TaskId::test(7),
                moving: Some(SpriteId::named(12)),
            }
        );
        assert_eq!(u.actions.make().copied().unwrap().task, new.tasks[3]);
        assert_eq!(u.actions.wait_solidify().copied().unwrap().task, waiting);
        // a Gather whose task the old master lacks keeps it
        let gather = u.actions.gather().copied().unwrap().task;
        let mut bare = old.clone();
        bare.tasks.clear();
        t.actions_take_master(
            Place::Live(PlayerId::new(1), ListIndex::new(0)),
            &bare,
            &new,
        )
        .unwrap();
        assert_eq!(
            t.players[1].objects[ListIndex::new(0)]
                .actions
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
            kind: master_type::COMBAT,
            class: class::CLASS_6,
            hit_points,
            ..Master::default()
        };
        let mut own = object(11, master_id(4), ObjectState::Alive, HitPoints::new(20.0));
        own.own_master = Some(Box::new(soldier(
            master_id(4),
            MasterHitPoints::of_data(40),
        )));
        let player = crate::world::Player {
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
            ..crate::world::Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        t.masters = vec![Masters::new([
            soldier(master_id(4), MasterHitPoints::of_data(40)),
            soldier(master_id(5), MasterHitPoints::of_data(60)),
            soldier(master_id(0x67f), MasterHitPoints::of_data(60)),
        ])];
        t.track_add(PlayerId::new(0), object_id(10), master_id(4))
            .unwrap();
        t
    }

    /// `FUN_141657d10` in mode 2 with the effect's object: that object alone takes a master of
    /// its own, its upgrade keeping it, then `to`'s values copied in; the player's master stays.
    /// In mode 3 an object whose own master has `from`'s id takes the upgrade too, and so does a
    /// sleeping object; masters the table lacks change nothing.
    #[test]
    fn a_mode_2_upgrade_gives_the_effect_s_object_a_master_of_its_own() {
        let hp = |t: &World| -> Vec<f32> {
            let p = &t.players[0];
            p.objects
                .iter()
                .chain(&p.sleeping)
                .map(|u| u.life.hp.get())
                .collect()
        };
        let mut t = upgrade_world();
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(5),
            UpgradeMode::ONE_OBJECT,
            Some(object_id(10)),
        )
        .unwrap();
        let own = t.players[0].objects[ListIndex::new(0)]
            .own_master
            .as_deref()
            .unwrap();
        assert_eq!(
            (own.id, own.hit_points),
            (master_id(4), MasterHitPoints::of_data(60))
        );
        assert_eq!(hp(&t), [30.0, 20.0, 20.0]);
        assert_eq!(
            t.masters[0].get(master_id(4)).map(|m| m.hit_points),
            Some(MasterHitPoints::of_data(40))
        );
        let mut t = upgrade_world();
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(5),
            UpgradeMode::OWN_MASTERS,
            None,
        )
        .unwrap();
        assert_eq!(hp(&t), [30.0, 30.0, 30.0]);
        let own = t.players[0].objects[ListIndex::new(1)]
            .own_master
            .as_deref()
            .unwrap();
        assert_eq!(
            (own.id, own.hit_points),
            (master_id(4), MasterHitPoints::of_data(60))
        );
        assert_eq!(
            t.masters[0].get(master_id(4)).map(|m| m.hit_points),
            Some(MasterHitPoints::of_data(60))
        );
        for (p, to) in [(0, 99), (3, 5)] {
            let mut t = upgrade_world();
            t.upgrade_objects(
                PlayerId::from(p),
                master_id(4),
                master_id(to),
                UpgradeMode::WALK,
                None,
            )
            .unwrap();
            assert_eq!(t, upgrade_world());
        }
    }

    /// The upgrades to 0x67f and 0x6a1 move the tracker's list of `from` to `to`'s
    /// (`FUN_14104fd10`); another upgrade leaves it.
    #[test]
    fn an_upgrade_to_0x67f_moves_the_tracker_s_list() {
        let mut t = upgrade_world();
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(0x67f),
            UpgradeMode::WALK,
            None,
        )
        .unwrap();
        let tracker = &t.players[0].tracker;
        assert_eq!(
            (
                tracker.list(ListKey::master(master_id(0x67f))),
                tracker.list(ListKey::master(master_id(4)))
            ),
            (&[Some(object_id(10))][..], &[][..])
        );
        let mut t = upgrade_world();
        t.upgrade_objects(
            PlayerId::new(0),
            master_id(4),
            master_id(5),
            UpgradeMode::WALK,
            None,
        )
        .unwrap();
        assert_eq!(
            t.players[0].tracker.list(ListKey::master(master_id(4))),
            [Some(object_id(10))]
        );
    }
    /// A world of player 0 holding `objects`, its masters `masters`, its graphics `graphics`
    /// (each animated) and 300 attributes.
    fn one_player(objects: Vec<Unit>, masters: Vec<Master>, graphics: &[i16]) -> World {
        let player = crate::world::Player {
            objects: objects.into(),
            attributes: vec![Amount::ZERO; 300],
            ..crate::world::Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(0),
            vec![Millis::ZERO],
            vec![player],
        );
        t.masters = vec![Masters::new(masters)];
        t.graphics = graphics
            .iter()
            .map(|&id| (SpriteId::named(id), graphic(id)))
            .collect();
        t
    }

    /// `FUN_14106a530`'s stop: without the master kept, a forager whose new master has no task
    /// for its top Gather stops (the Gather goes); kept, or with a Gather task, it goes on.
    #[test]
    fn an_upgrade_stops_a_top_action_the_new_master_has_no_task_for() {
        let mut t = crate::world::tests::forage_world();
        t.gather_command(PlayerId::new(1), ListIndex::new(0), object_id(600))
            .unwrap();
        let old = t.masters[1].get(master_id(120)).unwrap().clone();
        let bare = Master {
            id: master_id(121),
            tasks: Vec::new(),
            ..old.clone()
        };
        let place = Place::Live(PlayerId::new(1), ListIndex::new(0));
        let mut kept = t.clone();
        kept.upgrade_actions(place, (&old, &bare), true).unwrap();
        assert!(
            kept.players[1].objects[ListIndex::new(0)]
                .actions
                .gather()
                .is_some()
        );
        let mut same = t.clone();
        same.upgrade_actions(place, (&old, &old), false).unwrap();
        assert!(
            same.players[1].objects[ListIndex::new(0)]
                .actions
                .gather()
                .is_some()
        );
        t.upgrade_actions(place, (&old, &bare), false).unwrap();
        assert!(
            t.players[1].objects[ListIndex::new(0)]
                .actions
                .gather()
                .is_none()
        );
    }

    /// `FUN_14114cb80`: an object showing the old master's walking sprite shows the new one's,
    /// and keeps its sprite when the new master has none; one showing the running sprite shows
    /// the new running sprite; any other sprite stays.
    #[test]
    fn an_upgrade_s_moving_object_shows_the_new_walking_or_running_sprite() {
        let old = Master {
            id: master_id(4),
            move_sprite: Some(SpriteId::named(100)),
            run_sprite: Some(SpriteId::named(101)),
            ..Master::default()
        };
        let new = Master {
            id: master_id(5),
            move_sprite: Some(SpriteId::named(200)),
            run_sprite: Some(SpriteId::named(201)),
            ..Master::default()
        };
        let shown = |at: i16, new: &Master| {
            let mut u = object(10, master_id(4), ObjectState::Alive, HitPoints::new(10.0));
            u.sprite = Some(crate::objects::sprite::ActiveSprite::start(
                graphic(at),
                &mut Well512::default(),
            ));
            let mut t = one_player(vec![u], vec![old.clone()], &[100, 101, 102, 200, 201]);
            t.upgrade_moving_sprite(
                Place::Live(PlayerId::new(0), ListIndex::new(0)),
                (&old, new),
            )
            .unwrap();
            t.players[0].objects[ListIndex::new(0)]
                .sprite
                .map(|s| s.graphic.id)
        };
        assert_eq!(shown(100, &new), Some(SpriteId::named(200)));
        assert_eq!(shown(101, &new), Some(SpriteId::named(201)));
        assert_eq!(shown(102, &new), Some(SpriteId::named(102)));
        let walkless = Master {
            move_sprite: None,
            ..new
        };
        assert_eq!(shown(100, &walkless), Some(SpriteId::named(100)));
    }

    /// `FUN_141186580` without the master kept: a combat object leaves its class's counter; a
    /// villager's load goes to its player (berries as food, wood as wood) and its hands empty;
    /// another combat object, an empty load or one of no kind stays.
    #[test]
    fn an_upgrade_hands_a_villager_s_load_to_its_player() {
        let villager = Master {
            id: ids::VILLAGER_MALE,
            kind: master_type::COMBAT,
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
            let mut v = object(10, master.id, ObjectState::Alive, HitPoints::new(25.0));
            (v.life.attribute_type, v.life.amount) = (kind, amount);
            let mut t = one_player(vec![v], vec![master.clone()], &[]);
            t.players[0].attributes[37] = Amount::new(1.0);
            t.hand_in_load(
                PlayerId::new(0),
                Place::Live(PlayerId::new(0), ListIndex::new(0)),
                master,
            )
            .unwrap();
            let v = &t.players[0].objects[ListIndex::new(0)];
            (
                v.life.held(),
                [
                    t.players[0].attributes[0].get(),
                    t.players[0].attributes[1].get(),
                ],
                t.players[0].attributes[37],
            )
        };
        assert_eq!(
            load(
                &villager,
                Some(AttributeId::numbered(17)),
                Amount::new(10.0)
            ),
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
        let sighted = |state: ObjectState, inside: Option<ObjectId>, keep: bool| {
            let mut u = object(10, master_id(4), state, HitPoints::new(10.0));
            u.place_for_test(Position::new(5.5, 5.5, 0.0));
            u.life.inside = inside.map(ContainerId::new);
            let mut t = one_player(vec![u], vec![old.clone()], &[]);
            t.spatial = crate::map::spatial::Spatial::new(GridSize::new(20, 20));
            t.spatial.insert(
                object_id(10),
                crate::map::spatial::Placed {
                    position: Position::new(5.5, 5.5, 0.0),
                    radius: Footprint::new(0.2, 0.2),
                    class: class::CLASS_6,
                    player: PlayerId::new(0),
                    master: master_id(4),
                    obstruction_type: ObstructionType::NONE,
                    buildable_on: BuildableOn::NO,
                    tall: true,
                    speed: Speed::ZERO,
                    unknown: None,
                    obstruction: None,
                },
            );
            let map = || crate::vision::visibility::VisibleMap {
                counts: vec![0; 400],
                explored: TileCount::ZERO,
            };
            let mut v = crate::vision::visibility::Visibility::new(
                GridSize::new(20, 20),
                vec![map()],
                vec![VisionBits::NONE; 400],
                Vec::new(),
                0,
                vec![false],
            );
            v.add_seer(
                object_id(10),
                crate::vision::visibility::Seer {
                    player: PlayerId::new(0),
                    master: master_id(4),
                    los: Distance::new(5.0),
                    state: ObjectState::Alive,
                    explored: true,
                    building: None,
                    umv: 0,
                    capture: CaptureKind::NONE,
                    kind: master_type::COMBAT,
                },
            );
            t.spatial.sight = Some(v);
            t.upgrade_sight(
                Place::Live(PlayerId::new(0), ListIndex::new(0)),
                (&old, &new),
                keep,
            )
            .unwrap();
            let seer = t
                .spatial
                .sight
                .as_ref()
                .and_then(|v| v.seer(object_id(10)))
                .copied();
            seer.map(|s| (s.los, s.master))
        };
        assert_eq!(
            sighted(ObjectState::Alive, None, false),
            Some((Distance::new(7.0), master_id(5)))
        );
        assert_eq!(
            sighted(ObjectState::Alive, None, true),
            Some((Distance::new(7.0), master_id(4)))
        );
        assert_eq!(
            sighted(ObjectState::Alive, Some(object_id(99)), false),
            Some((Distance::new(5.0), master_id(4)))
        );
        assert_eq!(
            sighted(ObjectState::DyingSprite, None, false),
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
            kind: master_type::COMBAT,
            hit_points: MasterHitPoints::of_data(40),
            standing_sprite: Some(SpriteId::named(300)),
            standing_sprite_2: Some(SpriteId::named(301)),
            death_sprite: Some(SpriteId::named(302)),
            undead_sprite: Some(SpriteId::named(303)),
            charge_type: ChargeType::ATTACK,
            max_charge: Charge::new(2.0),
            ..Master::default()
        };
        let new = Master {
            id: master_id(5),
            hit_points: MasterHitPoints::of_data(60),
            standing_sprite: Some(SpriteId::named(400)),
            standing_sprite_2: None,
            death_sprite: Some(SpriteId::named(402)),
            undead_sprite: Some(SpriteId::named(403)),
            max_charge: Charge::new(5.0),
            ..old.clone()
        };
        let upgraded = |old: &Master, new: &Master, at: i16| {
            let mut u = object(
                10,
                master_id(4),
                ObjectState::DyingSprite,
                HitPoints::new(20.0),
            );
            u.sprite = Some(crate::objects::sprite::ActiveSprite::start(
                graphic(at),
                &mut Well512::default(),
            ));
            let graphics = [300, 301, 302, 303, 304, 400, 402, 403];
            let mut t = one_player(vec![u], vec![old.clone()], &graphics);
            t.upgrade_hit_points(Place::Live(PlayerId::new(0), ListIndex::new(0)), (old, new))
                .unwrap();
            let u = &t.players[0].objects[ListIndex::new(0)];
            (u.life.hp, u.sprite.map(|s| s.graphic.id), u.charge)
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
            charge_type: ChargeType::NONE,
            ..new
        };
        assert_eq!(upgraded(&old, &uncharged, 304).2, Charge::ZERO);
    }
}
