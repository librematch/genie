//! The player's attribute modifier (slot 0x148, `FUN_1416121b0`): the hooks around it, the
//! masters' change and what their objects and an Attack follow of it.

use crate::GenieError;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::ObstructionType;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::TaskSprites;
use crate::objects::masters::{AttributeWorld, Master, master_type, player_master};
use crate::players::PlayerId;
use crate::technology::effect::{CAPPED, Change, OBJECT_GONE, has_actions, within_table};
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

impl World {
    /// Slot 0x148 (`FUN_1416121b0`) of player `p`: the hooks before (`FUN_141614f40` with 0),
    /// the change of attribute `attribute` by `v` on master `unit` or on the masters and
    /// objects of class `class` ([`World::change_attribute`]), the hooks after (with 1). A master
    /// it reaches whose loader left the changed value unfilled ([`Master::unloaded_for`]) stops it
    /// first.
    pub(crate) fn modify_attribute(
        &mut self,
        p: PlayerId,
        change: Change,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        if let Some(why) = self.reached_masters(p, unit, class).iter().find_map(|&id| {
            self.masters
                .get(p.index())?
                .get(id)?
                .unloaded_for(attribute)
        }) {
            return Err(GenieError::unsupported(why));
        }
        self.effect_hooks(p, false, unit, class, attribute)?;
        self.change_attribute(p, change, unit, class, v, attribute)?;
        self.effect_hooks(p, true, unit, class, attribute)
    }

    /// `FUN_141614f40`: for each of player `p`'s live objects (its list `+0x30`), a combat unit's
    /// or building's volleys lose the shares of the combat units inside before the change
    /// (`FUN_141217ac0`) and take them again after (`FUN_14120b910`), as the change may move
    /// them. The objects' abilities (`+0x928`, `+0x930`, `FUN_14107da40` and `FUN_14107b830`) are
    /// not held: none taken. After a change of a sprite ([`MasterAttribute::shows_sprite`]) each object of the
    /// master or class shows it ([`World::show_changed_sprite`]).
    pub(super) fn effect_hooks(
        &mut self,
        p: PlayerId,
        after: bool,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        let ids: Vec<ObjectId> = self
            .players
            .player(p)?
            .objects
            .iter()
            .map(|u| u.id)
            .collect();
        for id in ids {
            let Some((_, u)) = self.object(id) else {
                continue;
            };
            let master = u.master(&self.masters, p)?;
            let (kind, master_id, master_class) = (master.kind, master.id, master.class);
            if matches!(kind, master_type::COMBAT | master_type::BUILDING) {
                self.garrison_volleys(p, id, after)?;
            }
            if after
                && attribute.shows_sprite()
                && (Some(master_class) == class || Some(master_id) == unit)
            {
                self.show_changed_sprite(p, id, attribute)?;
            }
        }
        Ok(())
    }

    /// `FUN_141217ac0` (`after` false) and `FUN_14120b910` (true) on object `id` of player `p`:
    /// with units inside, a reload above 0 and an attack rate above 0 (`COMISS`, `JBE`: not a
    /// NaN; [`crate::garrison::attack_rate`]), each combat unit inside takes its share
    /// ([`crate::garrison::share`]) off its volleys (`+0xa00`), or adds it.
    pub(super) fn garrison_volleys(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        after: bool,
    ) -> Result<(), GenieError> {
        use crate::math::fmath::at_most_or_unordered;
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        let masters = self.masters.checked(p.index())?;
        let master = u.master(&self.masters, p)?;
        if u.garrison.is_empty() || at_most_or_unordered(master.reload.get(), 0.0) {
            return Ok(());
        }
        let rate = crate::garrison::attack_rate(master, masters, u.reload_powerup);
        if at_most_or_unordered(rate, 0.0) {
            return Ok(());
        }
        let mut shares = Vec::new();
        for &inside in &u.garrison {
            let Some((q, unit)) = self.object(inside) else {
                continue;
            };
            let m = unit.master(&self.masters, q)?;
            if m.kind == master_type::COMBAT {
                shares.push(crate::garrison::share(
                    m,
                    self.masters.checked(q.index())?,
                    unit.reload_powerup,
                    rate,
                )?);
            }
        }
        let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
        for s in shares {
            u.firepower = if after {
                s + u.firepower
            } else {
                u.firepower - s
            };
        }
        Ok(())
    }

    /// The sprite part of `FUN_141614f40` after a sprite's change on object `id` of player `p`:
    /// one below type 40 or without actions shows its master's standing sprite again when the
    /// change was to the one it shows (71 with its idle variant `+0x78c` 0, 72 with 1;
    /// `FUN_141189760`, slot 0x68); one with actions has each take its master's sprites
    /// (`FUN_140a97970` -> `FUN_140a78150`): an Attack's (`FUN_140a7f800`) its moving, attack and
    /// standing sprites from its master unless they are its task's, and from type 70 on its idle
    /// attack and second attack sprites; a `MoveTo` keeps the sprite it holds (it is not one of
    /// the master's new ones) and shows it again (slot 0x60, which the unit shows already); the
    /// other actions' slot 0x50 does nothing.
    pub(super) fn show_changed_sprite(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        let Some((_, u)) = self.object(id) else {
            return Ok(());
        };
        let master = u.master(&self.masters, p)?.clone();
        let g = |id: Option<SpriteId>| id.and_then(|id| self.graphics.get(&id)).copied();
        if master.kind < master_type::ACTION || !has_actions(u) {
            let sprite = match (attribute, u.idle_variant) {
                (a::STANDING_GRAPHIC, 0) => master.standing_sprite,
                (a::STANDING_GRAPHIC_2, 1) => master.standing_sprite_2,
                _ => return Ok(()),
            };
            // slot 0x68 with no sprite (a second standing sprite the table lacks) shows none,
            // which the slice's sprites cannot hold; without the graphics the Sprite stream is not
            // modelled
            let (Some(graphic), false) = (g(sprite), self.graphics.is_empty()) else {
                return Ok(());
            };
            let place = self.place(id).ok_or(OBJECT_GONE)?;
            let rng = self.streams.get(Stream::Sprite);
            let u = match place {
                crate::orders::Place::Live(q, i) => self.players.live_mut(q, i)?,
                crate::orders::Place::Sleeping(q, i) => self.players.asleep_mut(q, i)?,
            };
            if u.sprite.is_some() {
                crate::objects::sprite::new_sprite(
                    &mut u.sprite,
                    u.body.move_stats_mut(),
                    graphic,
                    rng,
                );
            } else {
                // a sprite the slice does not hold is taken to be another one
                let _ = crate::objects::sprite::ActiveSprite::start(graphic, rng);
            }
            return Ok(());
        }
        let sprites = MasterSprites {
            moving: g(master.move_sprite),
            attack: g(master.attack_sprite),
            standing: g(master.standing_sprite),
            idle_attack: g(master.idle_attack_sprite),
            second: g(master.attack_sprite2),
            combat: master_type::COMBAT <= master.kind,
        };
        let (_, u) = self.object_mut(id).ok_or(OBJECT_GONE)?;
        // a unit's own Attack's task (the work command's attack task) has no sprites
        if let Some(attack) = u.actions.attack_mut() {
            follow_master(&mut attack.arms, None, &sprites);
        }
        if let Some(hunt) = u.actions.hunt_mut() {
            let task = hunt.task.sprites;
            if let Some(attack) = hunt.attack.as_mut() {
                follow_master(&mut attack.arms, Some(task), &sprites);
            }
        }
        Ok(())
    }

    /// `FUN_14165a000` of player `p`: a master `unit` within the player's table changes alone
    /// ([`World::change_master_attribute`]; a null slot changes nothing); else with a class, the
    /// objects the tracker files under it ([`World::tracked_class`]: each that runs its slot,
    /// [`World::change_object`], a building also its annexes of the class, but those with
    /// annexes of their own), the pending ones, then each master of the class, by id.
    pub(super) fn change_attribute(
        &mut self,
        p: PlayerId,
        change: Change,
        unit: Option<MasterId>,
        class: Option<ClassId>,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        let Some(masters) = self.masters.of(p) else {
            return Ok(());
        };
        if let Some(unit) = unit.filter(|&u| within_table(masters, u)) {
            return match masters.get(unit) {
                Some(_) => self.change_master_attribute(p, change, unit, v, attribute),
                None => Ok(()),
            };
        }
        let Some(class) = class else {
            return Ok(());
        };
        let (done, pending) = self.tracked_class(p, class)?;
        for id in done {
            if !self.change_object(id, change, v, attribute)? {
                continue;
            }
            let annexes = match self.object(id) {
                Some((_, u)) if u.master(&self.masters, p)?.kind == master_type::BUILDING => {
                    crate::objects::unit::annexes_of(id, &self.players.player(p)?.objects)
                }
                _ => continue,
            };
            for annex in annexes {
                let Some((_, a)) = self.object(annex) else {
                    continue;
                };
                let am = a.master(&self.masters, p)?;
                let own_annexes = am.kind == master_type::BUILDING
                    && !crate::objects::unit::annexes_of(annex, &self.players.player(p)?.objects)
                        .is_empty();
                if !own_annexes && am.class == class {
                    let _ = self.change_object(annex, change, v, attribute)?;
                }
            }
        }
        for id in pending {
            let _ = self.change_object(id, change, v, attribute)?;
        }
        for id in self.reached_masters(p, unit, Some(class)) {
            self.change_one_master(p, id, change, v, attribute)?;
        }
        Ok(())
    }

    /// The masters of player `p` slot 0x148 changes ([`World::change_attribute`]): master `unit`
    /// within the table (none for a null slot), else those of class `class`, by id.
    pub(super) fn reached_masters(
        &self,
        p: PlayerId,
        unit: Option<MasterId>,
        class: Option<ClassId>,
    ) -> Vec<MasterId> {
        let Some(masters) = self.masters.of(p) else {
            return Vec::new();
        };
        if let Some(unit) = unit.filter(|&u| within_table(masters, u)) {
            return masters.get(unit).map(|m| m.id).into_iter().collect();
        }
        let Some(class) = class else {
            return Vec::new();
        };
        masters
            .iter()
            .filter(|m| m.class == class)
            .map(|m| m.id)
            .collect()
    }

    /// `FUN_14165a400` with no object (`param_6` −1) of player `p`'s master `unit`: its objects
    /// the tracker files under it ([`World::tracked_master`]) each run their slot, but an obstruction
    /// type's change to another type and a height's change (32) take the object's obstruction
    /// record off first (`FUN_14165b510` -> `FUN_141189670`), and a hero status (40) on one from
    /// type 70 on runs its slot and is kept too; for a building master its pending objects
    /// follow; then the master's slot; then the kept objects: a building of the new height and not
    /// desolid takes its record again (`FUN_1411c3f70`, `FUN_14118c540`), as every object after
    /// an obstruction type's change; a hero in a group leaves it and joins it again
    /// (`FUN_14102c160`, `FUN_1410083c0`), which the slice's groups do not model.
    pub(super) fn change_master_attribute(
        &mut self,
        p: PlayerId,
        change: Change,
        unit: MasterId,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        let master = self.master(p, unit)?.clone();
        let (done, pending) = self.tracked_master(p, &master)?;
        let mut kept = Vec::new();
        for id in done {
            let Some((_, u)) = self.object(id) else {
                continue;
            };
            let kind = master.kind;
            let unrecorded = match attribute {
                // a char compare of the new type with the object's master's
                a::OBSTRUCTION_TYPE => {
                    ObstructionType::from(crate::math::conversions::cvttss2si(v) as u8)
                        != master.obstruction_type
                }
                a::SIZE_Z => true,
                _ => false,
            };
            if unrecorded {
                self.take_record(id, u.life.inside.is_some(), &master);
                kept.push(id);
            } else if attribute == a::HERO_STATUS && master_type::COMBAT <= kind {
                let _ = self.change_object(id, change, v, attribute)?;
                kept.push(id);
            } else {
                let _ = self.change_object(id, change, v, attribute)?;
            }
        }
        if master_type::BUILDING <= master.kind {
            for id in pending {
                let _ = self.change_object(id, change, v, attribute)?;
            }
        }
        self.change_one_master(p, unit, change, v, attribute)?;
        let master = self.master(p, unit)?.clone();
        for id in kept {
            match attribute {
                a::SIZE_Z => {
                    let desolid = self
                        .object(id)
                        .and_then(|(_, u)| u.construction.as_ref())
                        .is_some_and(|c| c.desolid);
                    if master.kind == master_type::BUILDING && !desolid && 0.0 < master.height {
                        self.give_record(id, &master);
                    }
                }
                a::OBSTRUCTION_TYPE => self.give_record(id, &master),
                _ => {
                    if self.object(id).is_some_and(|(_, u)| u.group.id.is_some()) {
                        return Err(GenieError::unsupported(
                            "a hero status change of a unit in a group (removeUnit, FUN_14102c160)",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// `FUN_141189670` on object `id` (`inside`: it is inside another, `+0x686`'s case; the slice
    /// takes the flag as that): a master with height above 0 (the record's `+0x658`, which the
    /// slice holds with the place's record, otherwise) has the obstruction manager drop the
    /// object's record (`FUN_141055b40`) and the mesh its shape (`+0x660`, `FUN_141053840`).
    pub(super) fn take_record(&mut self, id: ObjectId, inside: bool, master: &Master) {
        if inside {
            return;
        }
        let recorded = self
            .spatial
            .get(id)
            .is_some_and(|o| o.obstruction.is_some());
        if 0.0 < master.height || recorded {
            self.spatial.drop_record(id);
            self.pathing.removed(id);
        }
    }

    /// `FUN_14118c540` on object `id`: below state 7, not inside another (`+0x686`) and of a
    /// master with height above 0, it takes its record again (`FUN_141056460`: by the master's
    /// obstruction kind, not 0 or 12, and class, not 0) and its mesh shape.
    pub(super) fn give_record(&mut self, id: ObjectId, master: &Master) {
        let Some((_, u)) = self.object(id) else {
            return;
        };
        if ObjectState::Removed <= u.life.state
            || u.life.inside.is_some()
            || crate::math::fmath::at_most_or_unordered(master.height, 0.0)
        {
            return;
        }
        let record = crate::map::spatial::Obstruction {
            kind: master.obstruction_type,
            class: master.obstruction_class,
        };
        if record.recorded() {
            self.spatial.add_record(id, record);
            if let Some(placed) = self.spatial.get(id).copied() {
                self.pathing.place_shape(id, &placed);
            }
        }
    }

    /// Player `p`'s master `id` takes the change (its slot 0x18, 0x20 or 0x28), then the values
    /// the slice derives from it follow ([`Master::follow`], [`World::master_followers`]).
    pub(super) fn change_one_master(
        &mut self,
        p: PlayerId,
        id: MasterId,
        change: Change,
        v: f32,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        {
            let sprite = Self::attribute_world(&self.graphics);
            let world = AttributeWorld {
                sprite: &sprite,
                handicap: None,
                capped: CAPPED,
            };
            let terrain = self.spatial.terrain.as_ref();
            let Some(m) = self.masters.of_mut(p).and_then(|m| m.get_mut(id)) else {
                return Ok(());
            };
            match change {
                Change::Set => m.set_attribute(attribute, v, &world),
                Change::Add => m.add_attribute(attribute, v, &world),
                Change::Multiply => m.multiply_attribute(attribute, v, &world),
            }
            m.follow(attribute, &self.graphics, terrain);
        }
        self.master_followers(p, id, attribute)?;
        Ok(())
    }

    /// What player `p`'s objects of master `id` hold of it and the slice keeps beside the master
    /// (the game reads the master): their movement's values ([`Master::follow_moving`]), their
    /// dying sprite and death spawns, their construction's hit points, build time and record,
    /// their sight's line of sight and their tile's height test.
    pub(super) fn master_followers(
        &mut self,
        p: PlayerId,
        id: MasterId,
        attribute: MasterAttribute,
    ) -> Result<(), GenieError> {
        use crate::objects::masters::attribute as a;
        let Some(master) = player_master(&self.masters, p, id).cloned() else {
            return Ok(());
        };
        let terrain = self.spatial.terrain.as_ref();
        let player = self.players.player_mut(p)?;
        let mut ids = Vec::new();
        for u in player
            .objects
            .iter_mut()
            .chain(player.sleeping.iter_mut())
            .filter(|u| u.life.master_id == id)
        {
            ids.push(u.id);
            if let Some(m) = u.body.mover_mut() {
                master.follow_moving(&mut m.stats, attribute, &self.graphics, terrain);
                // the top speed the unit reads with the sprite it shows, from the new base
                if let Some(shown) = u.sprite.as_ref().map(|s| &s.graphic) {
                    m.stats.show(Some(shown));
                }
            }
            match attribute {
                a::DYING_GRAPHIC => {
                    u.life.dying_sprite = master.death_sprite;
                }
                a::DEAD_UNIT_ID => u.life.death_spawn = master.death_spawns[0].is_some(),
                a::BLOOD_UNIT => u.life.remains = master.death_spawns[1],
                _ => {}
            }
            if let Some(c) = u.construction.as_mut() {
                match attribute {
                    a::HIT_POINTS => c.hit_points = master.hit_points,
                    a::TRAIN_TIME
                    | a::TRAIN_LOCATION
                    | a::TRAIN_LOCATIONS_ENTRY_MOD
                    | a::TRAIN_LOCATIONS_TOTAL_NUM => c.build_time = master.build_time,
                    a::OBSTRUCTION_TYPE | a::BLOCKAGE_CLASS => {
                        c.record = crate::map::spatial::Obstruction {
                            kind: master.obstruction_type,
                            class: master.obstruction_class,
                        };
                    }
                    _ => {}
                }
            }
        }
        for id in ids {
            match attribute {
                a::LINE_OF_SIGHT => {
                    if let Some(s) = self.spatial.sight.as_mut().and_then(|v| v.seer_mut(id)) {
                        s.los = master.line_of_sight;
                    }
                }
                a::SIZE_Z => self.spatial.set_tall(id, master.tall),
                _ => {}
            }
        }
        Ok(())
    }
}

/// A master's sprites an Attack's slot 0x50 (`FUN_140a7f800`) takes ([`follow_master`]), as
/// graphics: the walking, attack, standing, idle attack and second attack sprites, and whether
/// the master is of a combat type (70 and up).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MasterSprites {
    pub(crate) moving: Option<crate::objects::sprite::Graphic>,
    pub(crate) attack: Option<crate::objects::sprite::Graphic>,
    pub(crate) standing: Option<crate::objects::sprite::Graphic>,
    pub(crate) idle_attack: Option<crate::objects::sprite::Graphic>,
    pub(crate) second: Option<crate::objects::sprite::Graphic>,
    pub(crate) combat: bool,
}

/// An Attack's slot 0x50 (`FUN_140a7f800`): its walking, attack and standing sprites take the
/// master's (`sprites`) unless they are its task's (`task`, the moving, proceeding, working and
/// carrying sprites; none for a unit's own Attack), and from type 70 on its idle attack and
/// second attack sprites.
pub(crate) fn follow_master(
    arms: &mut crate::combat::attack::Arms,
    task: Option<TaskSprites>,
    sprites: &MasterSprites,
) {
    let of_task = |s: Option<crate::objects::sprite::Graphic>,
                   pick: fn(TaskSprites) -> Option<SpriteId>| {
        task.zip(s).is_some_and(|(t, s)| pick(t) == Some(s.id))
    };
    let proceeding = |t: TaskSprites| t.proceeding;
    let working = |t: TaskSprites| t.working;
    if arms.move_sprite.is_some() && !of_task(arms.move_sprite, |t| t.moving) {
        arms.move_sprite = sprites.moving;
    }
    if arms.sprite.is_some() && !of_task(arms.sprite, proceeding) && !of_task(arms.sprite, working)
    {
        arms.sprite = sprites.attack;
    }
    if arms.idle_sprite.is_some()
        && !of_task(arms.idle_sprite, proceeding)
        && !of_task(arms.idle_sprite, working)
    {
        arms.idle_sprite = sprites.standing;
    }
    if sprites.combat {
        arms.idle_attack_sprite = sprites.idle_attack;
        arms.second_sprite = sprites.second;
    }
}
