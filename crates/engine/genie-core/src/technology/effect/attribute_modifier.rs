//! The player's attribute modifier (slot 0x148, `FUN_1416121b0`): the hooks around it, the
//! masters' change and what their objects and an Attack follow of it.

use crate::garrison::ContainerId;
use crate::garrison::{attack_rate, share};
use crate::map::Distance;
use crate::map::spatial::Obstruction;
use crate::math::conversions::cvttss2si;
use crate::math::fmath::at_most_or_unordered;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::AttributeWorld;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObstructionKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::follow;
use crate::objects::masters::follow_moving;
use crate::objects::masters::player_master;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::players::PlayerId;
use crate::technology::effect::object_slots::ObjectChange;
use crate::technology::effect::{CAPPED, Change, OBJECT_GONE, has_actions, within_table};
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};

/// Which side of a player's change a hook runs on (`FUN_141614f40`'s flag): before it (0: the
/// volleys lose the shares of the units inside, `FUN_141217ac0`) or after it (1: they take
/// them again, `FUN_14120b910`, and a changed sprite shows).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum EffectHook {
    /// Before the change.
    Before,
    /// After the change.
    After,
}

impl World {
    /// Slot 0x148 (`FUN_1416121b0`) of player `player_id`: the hooks before (`FUN_141614f40` with
    /// 0), the change of attribute `attribute` by `value` on master `unit` or on the masters and
    /// objects of class `class` ([`World::change_attribute`]), the hooks after (with 1). A master
    /// it reaches whose loader left the changed value unfilled ([`Master::unloaded_for`]) stops it
    /// first.
    pub(crate) fn modify_attribute(
        &mut self,
        player_id: PlayerId,
        change: Change,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        if let Some(why) = self
            .reached_masters(player_id, unit, class)
            .iter()
            .find_map(|&id| {
                self.model
                    .masters
                    .get(player_id.index())?
                    .get(id)?
                    .unloaded_for(attribute)
            })
        {
            return Err(GenieError::unsupported(why));
        }
        self.effect_hooks(player_id, EffectHook::Before, unit, class, attribute)?;
        self.change_attribute(player_id, change, unit, class, value, attribute)?;
        self.effect_hooks(player_id, EffectHook::After, unit, class, attribute)
    }

    /// `FUN_141614f40`: for each of player `player_id`'s live objects (its list `+0x30`), a combat
    /// unit's or building's volleys lose the shares of the combat units inside before the change
    /// (`FUN_141217ac0`) and take them again after (`FUN_14120b910`), as the change may move them.
    /// The objects' abilities (`+0x928`, `+0x930`, `FUN_14107da40` and `FUN_14107b830`) are not
    /// held: none taken. After a change of a sprite ([`MasterAttribute::shows_sprite`]) each object
    /// of the master or class shows it ([`World::show_changed_sprite`]).
    pub(super) fn effect_hooks(
        &mut self,
        player_id: PlayerId,
        hook: EffectHook,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        let ids: Vec<ObjectId> = self
            .players
            .player(player_id)?
            .objects
            .iter()
            .map(|unit| unit.id)
            .collect();
        for id in ids {
            let Some((_, held)) = self.object(id) else {
                continue;
            };
            let master = held.master(self.master_tables(), player_id)?;
            let (kind, master_id, master_class) = (master.kind, master.id, master.class);
            if matches!(kind, MasterKind::COMBAT | MasterKind::BUILDING) {
                self.garrison_volleys(player_id, id, hook)?;
            }
            if hook == EffectHook::After
                && attribute.shows_sprite()
                && (Some(master_class) == class || Some(master_id) == unit)
            {
                self.show_changed_sprite(player_id, id, attribute)?;
            }
        }
        Ok(())
    }

    /// `FUN_141217ac0` ([`EffectHook::Before`]) and `FUN_14120b910` ([`EffectHook::After`]) on
    /// object `id` of player `owner`:
    /// with units inside, a reload above 0 and an attack rate above 0 (`COMISS`, `JBE`: not a
    /// NaN; [`crate::garrison::attack_rate`]), each combat unit inside takes its share
    /// ([`crate::garrison::share`]) off its volleys (`+0xa00`), or adds it.
    pub(super) fn garrison_volleys(
        &mut self,
        owner: PlayerId,
        id: ObjectId,
        hook: EffectHook,
    ) -> GenieResult<()> {
        use crate::math::fmath::at_most_or_unordered;
        let Some((_, held)) = self.object(id) else {
            return Ok(());
        };
        let masters = self.model.masters.checked(owner.index())?;
        let master = held.master(self.master_tables(), owner)?;
        if held.garrison.is_empty() || at_most_or_unordered(master.combat_part().reload.get(), 0.0)
        {
            return Ok(());
        }
        let rate = attack_rate(
            master,
            masters,
            self.parts.combat_of(held.id).reload_powerup,
            &self.model.roles,
            &self.model.tuning.garrison,
        );
        if at_most_or_unordered(rate.get(), 0.0) {
            return Ok(());
        }
        let mut shares = Vec::new();
        for &inside in &held.garrison {
            let Some((player_id, unit)) = self.object(inside) else {
                continue;
            };
            let master = unit.master(self.master_tables(), player_id)?;
            if master.kind == MasterKind::COMBAT {
                shares.push(share(
                    master,
                    self.model.masters.checked(player_id.index())?,
                    self.parts.combat_of(unit.id).reload_powerup,
                    rate,
                    &self.model.roles,
                )?);
            }
        }
        let _ = self.object(id).ok_or(OBJECT_GONE)?;
        let combat = self.parts.combat_mut(id)?;
        for firepower in shares {
            combat.firepower = if hook == EffectHook::After {
                firepower + combat.firepower
            } else {
                combat.firepower - firepower
            };
        }
        Ok(())
    }

    /// The sprite part of `FUN_141614f40` after a sprite's change on object `id` of player
    /// `player`: one below type 40 or without actions shows its master's standing sprite again when
    /// the change was to the one it shows (71 with its idle variant `+0x78c` 0, 72 with 1;
    /// `FUN_141189760`, slot 0x68); one with actions runs the task change with its master as
    /// both the old and the new one (`FUN_140a97970` -> `FUN_140a78150`,
    /// [`World::actions_take_master`]): an action with a task takes the master's last task of
    /// its key, and an Attack's sprites ([`Arms::take_master`]) and a `MoveTo`'s follow the
    /// changed master's.
    ///
    /// [`Arms::take_master`]: crate::combat::attack::Arms::take_master
    pub(super) fn show_changed_sprite(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        let Some((_, held)) = self.object(id) else {
            return Ok(());
        };
        let master = held.master(self.master_tables(), player)?.clone();
        let graphic_of =
            |id: Option<SpriteId>| id.and_then(|id| self.model.graphics.get(id)).copied();
        if master.kind < MasterKind::ACTION
            || !has_actions(
                self.parts.actions_of(held.id),
                self.parts.production.get(held.id),
                self.parts.missile.get(held.id),
            )
        {
            let sprite = match (attribute, held.idle_variant) {
                (MasterAttribute::STANDING_GRAPHIC, 0) => master.standing_sprite,
                (MasterAttribute::STANDING_GRAPHIC_2, 1) => master.standing_sprite_2,
                _ => return Ok(()),
            };
            // slot 0x68 with no sprite (a second standing sprite the table lacks) shows none,
            // which the slice's sprites cannot hold; without the graphics the Sprite stream is not
            // modelled
            let (Some(graphic), false) = (graphic_of(sprite), self.model.graphics.is_empty())
            else {
                return Ok(());
            };
            let place = self.place(id).ok_or(OBJECT_GONE)?;
            let rng = self.streams.get_mut(Stream::Sprite);
            let unit = match place {
                Place::Live(owner, slot) => self.players.live_mut(owner, slot)?,
                Place::Sleeping(player_id, list_index) => {
                    self.players.asleep_mut(player_id, list_index)?
                }
            };
            if unit.sprite.is_some() {
                new_sprite(
                    &mut unit.sprite,
                    self.parts.move_stats_mut(unit.id),
                    graphic,
                    rng,
                    self.model.tuning.sprite,
                );
            } else {
                // a sprite the slice does not hold is taken to be another one
                let _ = ActiveSprite::start(graphic, rng, self.model.tuning.sprite);
            }
            return Ok(());
        }
        let place = self.place(id).ok_or(OBJECT_GONE)?;
        self.actions_take_master(place, &master, &master)
    }

    /// `FUN_14165a000` of player `player_id`: a master `unit` within the player's table changes
    /// alone ([`World::change_master_attribute`]; a null slot changes nothing); else with a class,
    /// the objects the tracker files under it ([`World::tracked_class`]: each that runs its slot,
    /// [`World::change_object`], a building also its annexes of the class, but those with annexes
    /// of their own), the pending ones, then each master of the class, by id.
    pub(super) fn change_attribute(
        &mut self,
        player_id: PlayerId,
        change: Change,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        let Some(masters) = self.model.masters.of(player_id) else {
            return Ok(());
        };
        if let Some(unit) = unit.filter(|&master_id| within_table(masters, master_id)) {
            return match masters.get(unit) {
                Some(_) => self.change_master_attribute(player_id, change, unit, value, attribute),
                None => Ok(()),
            };
        }
        let Some(class) = class else {
            return Ok(());
        };
        let (done, pending) = self.tracked_class(player_id, class)?;
        for id in done {
            if self.change_object(id, change, value, attribute)? == ObjectChange::NotHeld {
                continue;
            }
            let annexes = match self.object(id) {
                Some((_, unit))
                    if unit.master(self.master_tables(), player_id)?.kind
                        == MasterKind::BUILDING =>
                {
                    self.parts.building_of(unit.id).annexes
                }
                _ => continue,
            };
            for annex in annexes.ids() {
                let Some((_, annex_unit)) = self.object(annex) else {
                    continue;
                };
                let am = annex_unit.master(self.master_tables(), player_id)?;
                let own_annexes = am.kind == MasterKind::BUILDING
                    && self
                        .parts
                        .building_of(annex_unit.id)
                        .annexes
                        .ids()
                        .next()
                        .is_some();
                if !own_annexes && am.class == class {
                    let _ = self.change_object(annex, change, value, attribute)?;
                }
            }
        }
        for id in pending {
            let _ = self.change_object(id, change, value, attribute)?;
        }
        for id in self.reached_masters(player_id, unit, Some(class)) {
            self.change_one_master(player_id, id, change, value, attribute)?;
        }
        Ok(())
    }

    /// The masters of player `player_id` slot 0x148 changes ([`World::change_attribute`]): master
    /// `unit` within the table (none for a null slot), else those of class `class`, by id.
    pub(super) fn reached_masters(
        &self,
        player_id: PlayerId,
        unit: Option<MasterId>,
        class: Option<ClassId>,
    ) -> Vec<MasterId> {
        let Some(masters) = self.model.masters.of(player_id) else {
            return Vec::new();
        };
        if let Some(unit) = unit.filter(|&master_id| within_table(masters, master_id)) {
            return masters
                .get(unit)
                .map(|unit_master| unit_master.id)
                .into_iter()
                .collect();
        }
        let Some(class) = class else {
            return Vec::new();
        };
        masters
            .iter()
            .filter(|master| master.class == class)
            .map(|master| master.id)
            .collect()
    }

    /// `FUN_14165a400` with no object (`param_6` −1) of player `player_id`'s master `unit`: its
    /// objects the tracker files under it ([`World::tracked_master`]) each run their slot, but an
    /// obstruction type's change to another type and a height's change (32) take the object's
    /// obstruction record off first (`FUN_14165b510` -> `FUN_141189670`), and a hero status (40) on
    /// one from type 70 on runs its slot and is kept too; for a building master its pending objects
    /// follow; then the master's slot; then the kept objects: a building of the new height and not
    /// desolid takes its record again (`FUN_1411c3f70`, `FUN_14118c540`), as every object after an
    /// obstruction type's change; a hero in a group leaves it and joins it again (`FUN_14102c160`,
    /// `FUN_1410083c0`), which the slice's groups do not model.
    pub(super) fn change_master_attribute(
        &mut self,
        player_id: PlayerId,
        change: Change,
        unit: MasterId,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        let master = self.master(player_id, unit)?.clone();
        let (done, pending) = self.tracked_master(player_id, &master)?;
        let mut kept = Vec::new();
        for id in done {
            let Some((_, unit)) = self.object(id) else {
                continue;
            };
            let kind = master.kind;
            let unrecorded = match attribute {
                // a char compare of the new type with the object's master's
                MasterAttribute::OBSTRUCTION_KIND => {
                    ObstructionKind::from(cvttss2si(value) as u8) != master.obstruction_kind
                }
                MasterAttribute::SIZE_Z => true,
                _ => false,
            };
            if unrecorded {
                self.take_record(id, unit.life.inside, &master);
                kept.push(id);
            } else if attribute == MasterAttribute::HERO_STATUS && MasterKind::COMBAT <= kind {
                let _ = self.change_object(id, change, value, attribute)?;
                kept.push(id);
            } else {
                let _ = self.change_object(id, change, value, attribute)?;
            }
        }
        if MasterKind::BUILDING <= master.kind {
            for id in pending {
                let _ = self.change_object(id, change, value, attribute)?;
            }
        }
        self.change_one_master(player_id, unit, change, value, attribute)?;
        let master = self.master(player_id, unit)?.clone();
        for id in kept {
            match attribute {
                MasterAttribute::SIZE_Z => {
                    let desolid = self
                        .object(id)
                        .and_then(|(_, unit)| self.parts.building_of(unit.id).construction.as_ref())
                        .is_some_and(|construction| construction.desolid);
                    if master.kind == MasterKind::BUILDING
                        && !desolid
                        && Distance::ZERO < master.height
                    {
                        self.give_record(id, &master);
                    }
                }
                MasterAttribute::OBSTRUCTION_KIND => self.give_record(id, &master),
                _ => {
                    if self
                        .object(id)
                        .is_some_and(|(_, unit)| unit.group.id.is_some())
                    {
                        return Err(GenieError::unsupported(
                            "a hero status change of a unit in a group (removeUnit, FUN_14102c160)",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// `FUN_141189670` on object `id`, inside `container` or on the map (`+0x686`'s case; the
    /// slice takes the flag as that): on the map, a master with height above 0 (the record's
    /// `+0x658`, which the slice holds with the place's record, otherwise) has the obstruction
    /// manager drop the object's record (`FUN_141055b40`) and the mesh its shape (`+0x660`,
    /// `FUN_141053840`).
    pub(super) fn take_record(
        &mut self,
        id: ObjectId,
        container: Option<ContainerId>,
        master: &Master,
    ) {
        if container.is_some() {
            return;
        }
        let recorded = self
            .spatial
            .get(id)
            .is_some_and(|placed| placed.obstruction.is_some());
        if Distance::ZERO < master.height || recorded {
            self.spatial.drop_record(id);
            self.pathing.removed(id);
        }
    }

    /// `FUN_14118c540` on object `id`: below state 7, not inside another (`+0x686`) and of a
    /// master with height above 0, it takes its record again (`FUN_141056460`: by the master's
    /// obstruction kind, not 0 or 12, and class, not 0) and its mesh shape.
    pub(super) fn give_record(&mut self, id: ObjectId, master: &Master) {
        let Some((_, unit)) = self.object(id) else {
            return;
        };
        if ObjectState::Removed <= unit.life.state
            || unit.life.inside.is_some()
            || at_most_or_unordered(master.height.get(), 0.0)
        {
            return;
        }
        let record = Obstruction {
            kind: master.obstruction_kind,
            class: master.obstruction_class,
        };
        if record.recorded() {
            self.spatial.add_record(id, record);
            if let Some(placed) = self.spatial.get(id).copied() {
                self.pathing.place_shape(id, &placed);
            }
        }
    }

    /// Player `player_id`'s master `id` takes the change (its slot 0x18, 0x20 or 0x28), then the
    /// values the slice derives from it follow ([`follow`], [`World::master_followers`]).
    pub(super) fn change_one_master(
        &mut self,
        player_id: PlayerId,
        id: MasterId,
        change: Change,
        value: f32,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        {
            let world = AttributeWorld {
                sprites: &self.model.graphics,
                handicap: None,
                capped: CAPPED,
                roles: &self.model.roles,
                masters_tuning: &self.model.tuning.masters,
            };
            let terrain = self.spatial.terrain.as_ref();
            let Some(master) = self
                .model
                .masters
                .of_mut(player_id)
                .and_then(|masters| masters.get_mut(id))
            else {
                return Ok(());
            };
            match change {
                Change::Set => master.set_attribute(attribute, value, &world),
                Change::Add => master.add_attribute(attribute, value, &world),
                Change::Multiply => master.multiply_attribute(attribute, value, &world),
            }
            follow(master, attribute, &self.model.graphics, terrain);
        }
        self.master_followers(player_id, id, attribute)?;
        Ok(())
    }

    /// What player `player_id`'s objects of master `id` hold of it and the slice keeps beside the
    /// master (the game reads the master): their movement's values ([`follow_moving`]), their
    /// construction's hit points, build time and record, their sight's line of sight and their
    /// tile's height test.
    pub(super) fn master_followers(
        &mut self,
        player_id: PlayerId,
        id: MasterId,
        attribute: MasterAttribute,
    ) -> GenieResult<()> {
        use crate::objects::masters::MasterAttribute;
        let Some(master) = player_master(&self.model.masters, player_id, id).cloned() else {
            return Ok(());
        };
        let terrain = self.spatial.terrain.as_ref();
        let player = self.players.player_mut(player_id)?;
        let mut ids = Vec::new();
        for unit in player
            .objects
            .iter_mut()
            .chain(player.sleeping.iter_mut())
            .filter(|unit| unit.life.master_id == id)
        {
            ids.push(unit.id);
            if let Some(mover) = self.parts.mover_mut(unit.id) {
                follow_moving(
                    &master,
                    &mut mover.stats,
                    attribute,
                    &self.model.graphics,
                    terrain,
                );
                // the top speed the unit reads with the sprite it shows, from the new base
                if let Some(shown) = unit
                    .sprite
                    .as_ref()
                    .map(|active_sprite| &active_sprite.graphic)
                {
                    mover.stats.show(Some(shown));
                }
            }
            if let Some(construction) = self.parts.building_mut(unit.id)?.construction.as_mut() {
                match attribute {
                    MasterAttribute::HIT_POINTS => construction.hit_points = master.hit_points,
                    MasterAttribute::TRAIN_TIME
                    | MasterAttribute::TRAIN_LOCATION
                    | MasterAttribute::TRAIN_LOCATIONS_ENTRY_MOD
                    | MasterAttribute::TRAIN_LOCATIONS_TOTAL_NUM => {
                        construction.build_time = master.creatable_part().build_time;
                    }
                    MasterAttribute::OBSTRUCTION_KIND | MasterAttribute::BLOCKAGE_CLASS => {
                        construction.record = Obstruction {
                            kind: master.obstruction_kind,
                            class: master.obstruction_class,
                        };
                    }
                    _ => {}
                }
            }
        }
        for id in ids {
            match attribute {
                MasterAttribute::LINE_OF_SIGHT => {
                    if let Some(seer) = self
                        .spatial
                        .sight
                        .as_mut()
                        .and_then(|visibility| visibility.seer_mut(id))
                    {
                        seer.los = master.line_of_sight;
                    }
                }
                MasterAttribute::SIZE_Z => self.spatial.set_tall(id, master.is_tall()),
                _ => {}
            }
        }
        Ok(())
    }
}
