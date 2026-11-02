//! Taking and releasing targets, the order record, actions cleared, transformations, gates
//! and master switches.

use crate::GenieError;
use crate::construction::gates::gate_units;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::math::rng::Stream;
use crate::math::rng::Well512;
use crate::movement::Mover;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::layers::Combat;
use crate::objects::layers::MotionMut;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::release::Releaser;
use crate::objects::sprite::{ActiveSprite, new_sprite};
use crate::objects::unit::Unit;
use crate::orders::object_gone;
use crate::orders::{TROPHIES, TROPHY, clear_powerups, damage_share};
use crate::pathing::Pathing;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderRecord;
use crate::unit_ai::best_target::category;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::slots::PlayerSlots;

impl World {
    /// Slot 0x178 on the object an action of `by` takes as its target, outside an update
    /// ([`crate::world::lets::Lets::take`] on the whole world): a building's (`FUN_1411dba70`)
    /// first lists an unallied taker of a foundation with a stack unit ([`Unit::taken`]); then
    /// the base take (`FUN_14119d3e0`): its worker count (`+0x644`) rises, to at most 550; a
    /// sleeping object (static, never a building) wakes: it leaves its player's sleeping list
    /// (slot 0x2d8, the last object taking its place) for the end of the live list (slot
    /// 0x2d0). A live object flagged to sleep only drops the flags.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::Lets::take`]'s.
    pub(crate) fn take(&mut self, id: ObjectId, by: Releaser) -> Result<(), GenieError> {
        let roles = self.model.roles.clone();
        self.with_lets(by.player, |lets, others| {
            lets.take(others, None, id, by, &roles)
        })?
    }

    /// Slot 0x180 on the object an action of `by` lets go, outside an update
    /// ([`crate::world::lets::Lets::release`] on the whole world,
    /// [`crate::objects::release::released`]): a building's (`FUN_1411d6020`) drops an unallied
    /// releaser from the attacking units of a foundation with a stack unit
    /// ([`Unit::lists_taker`]); its worker count falls; a static object (type below 30,
    /// `FUN_141197d00`) that no one works on any more and is alive (state 2 with hit points at
    /// least 0) or in state 6 is flagged to sleep (`+0x641`, `+0x643`); a combat object
    /// (`FUN_141086e60`) tells its unit AI (notify 699). A sleeping object only counts a worker
    /// less.
    ///
    /// # Errors
    ///
    /// [`crate::world::lets::Lets::release`]'s.
    pub(crate) fn release(&mut self, id: ObjectId, by: Releaser) -> Result<(), GenieError> {
        self.with_lets(by.player, |lets, others| lets.release(others, None, id, by))?
    }

    /// Object `idx` of player `player` as the releaser of its actions' targets.
    pub(crate) fn releaser(
        &self,
        player: PlayerId,
        idx: ListIndex,
    ) -> Result<Releaser, GenieError> {
        let u = self.players.live(player, idx)?;
        Ok(Releaser {
            id: u.id,
            class: Some(u.master(self.master_tables(), player)?.class),
            player,
        })
    }

    /// `FUN_140c42080` after the notify of a new action (`FUN_14106e4e0`, the combat object's
    /// `FUN_14121c890`, the attack order of `FUN_14161b970`) of object `idx` of player `player`:
    /// with a target the world holds (`FUN_1415417a0`), its unit AI remembers `order` and
    /// `action` on it ([`crate::unit_ai::UnitAi::remember_order`]) at the world's time, with the
    /// unit's and the target's slot 0x520 ([`Unit::moved_vector`]: x, height, y) and the target's
    /// attack category by its master
    /// ([`crate::unit_ai::best_target::category`]: its own, else its owner's). An order on a point (no
    /// target) records nothing.
    pub(crate) fn remember_order(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        (order, action): (Option<OrderKind>, Option<AiAction>),
    ) -> Result<(), GenieError> {
        let u = &self.players.live(player, idx)?;
        let Some(target) = self
            .parts
            .unit_ai
            .get(u.id)
            .and_then(|a| a.current_target)
            .filter(|t| !t.is_recyclable())
        else {
            return Ok(());
        };
        let position = u.moved_vector(self.parts.mover_of(u.id));
        let Some((q, t)) = self.object(target) else {
            return Ok(());
        };
        let class = t.master(self.master_tables(), q)?.class;
        let record = OrderRecord {
            order,
            action,
            time: self.world_time,
            position,
            target: Some(target),
            category: category(t.life.master_id, class, &self.model.roles),
            target_position: t.moved_vector(self.parts.mover_of(t.id)),
        };
        if let Some(ai) = self
            .parts
            .unit_ai
            .get_mut(self.players.live(player, idx)?.id)
        {
            ai.remember_order(record);
        }
        Ok(())
    }

    /// The action list's clearing (`FUN_140a98620`) of object `idx` of player `player`: each
    /// action's teardown ([`Unit::clear_own_actions`]), then what they leave to the world: a
    /// counted builder leaves its building (`FUN_140aadbf0` before its base teardown,
    /// [`World::leave_builders`]), then the targets go (slot 0x180, [`World::release`]). A
    /// building's production is a list of its own, which stays.
    pub(crate) fn clear_actions(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        let by = self.releaser(player, idx)?;
        let (mut releases, mut builder_leaves) = (Vec::new(), Vec::new());
        let unit = self.players.live_mut(player, idx)?;
        unit.clear_own_actions(
            self.parts.actions.of_mut(unit.id)?,
            self.parts.motion.layer_mut(unit.id),
            &mut releases,
            &mut builder_leaves,
            by,
        )?;
        for id in builder_leaves {
            self.leave_builders(Some(id))?;
        }
        for (id, by) in releases {
            self.release(id, by)?;
        }
        Ok(())
    }

    /// `TRANSFORM_OBJECT` (`RGE_Command::do_command` case 0x29): the object `id`, the player's
    /// (another player's is refused, the cheat flag `+0xb12` aside), of the trophy master
    /// (1649) takes one of the trophies 1650 to 1653 the player has (`FUN_141659d20`): slot 0x90
    /// with 0, the building's ([`World::transform_building`]).
    pub(crate) fn transform_object(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        let Some(Place::Live(p, idx)) = self.place(id) else {
            return Ok(());
        };
        if p != player || self.master(player, master).is_err() {
            return Ok(());
        }
        let unit = &self.players.live(p, idx)?;
        if unit.life.master_id != TROPHY || !TROPHIES.contains(&master.get()) {
            return Ok(());
        }
        self.transform_building(p, idx, master)
    }

    /// A building's slot 0x90 (`FUN_1411e6d90`, with 0 from each caller): [`World::change_master`],
    /// then in state 2 its build points (`+0xc50`) take the new master's build time (its first
    /// train location's time, 0 without one). The slice holds them in the building's
    /// construction record.
    pub(super) fn transform_building(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        new: MasterId,
    ) -> Result<(), GenieError> {
        self.change_master(player, idx, new)?;
        let build_time = self.master(player, new)?.build_time;
        let unit = self.players.live_mut(player, idx)?;
        if unit.life.state == ObjectState::Alive
            && let Some(c) = self.parts.building_mut(unit.id)?.construction.as_mut()
        {
            c.progress = Seconds::of_master(build_time);
        }
        Ok(())
    }

    /// The player's gate toggle (`FUN_14162f8c0`, from its slots 0x340 and 0x338): a gate of its
    /// player's masters, without masters of its own ([`crate::world::Gate::other_masters`]),
    /// changes to its master's `transform_unit` (slot 0x90, [`World::transform_building`]). A
    /// gate that shut (the new master's height above 0) kills the units in it (slot 0xd8 on
    /// those its count finds, [`World::shut_in_gate`]) and takes the world time plus 1 as its
    /// last move time (`+0x870`).
    pub(crate) fn toggle_gate(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(player, idx)?;
        if self
            .parts
            .building_of(unit.id)
            .gate
            .is_some_and(|g| g.other_masters)
        {
            return Err(GenieError::unsupported(
                "a gate's toggle to a master of its own or its original owner's",
            ));
        }
        let Some(new) = unit
            .master(self.master_tables(), player)?
            .building
            .as_ref()
            .and_then(|b| b.transform_unit)
        else {
            return Ok(());
        };
        self.transform_building(player, idx, new)?;
        let height = self.master(player, new)?.height;
        let now = self.world_time;
        if Distance::ZERO < height {
            let id = self.players.live(player, idx)?.id;
            let annexes: Vec<ObjectId> = self.objects().annexes(id).ids().collect();
            let (units, _) = gate_units(
                &mut self.spatial,
                id,
                &annexes,
                ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.model.masters,
                    &self.parts,
                    &self.model.roles,
                ),
            )?;
            for u in units {
                self.shut_in_gate(u)?;
            }
            self.parts
                .mover_mut(self.players.live(player, idx)?.id)
                .ok_or(GenieError::unsupported(
                    "a shut gate without its moving part",
                ))?
                .last_move = now.after(Millis::new(1));
        }
        Ok(())
    }

    /// Slot 0xd8 of object `id` in a gate that shut (`FUN_14162f8c0`): from type 70 on
    /// (`FUN_141211340`) class 42 is spared, and livestock (class 58) while it holds some
    /// (`COMISS`, `JA`: a NaN load holds none); else, and below type 70, `FUN_14118f580`: an
    /// object asleep wakes to the end of its player's live list (slots 0x2d8, 0x2d0), one flagged
    /// to sleep drops the flag, and its hit points go to 0. Its place in its player's
    /// special-object list (`+0x30c` bit 0, `FUN_141677380`) is not held.
    pub(super) fn shut_in_gate(&mut self, id: ObjectId) -> Result<(), GenieError> {
        /// The first master type whose slot 0xd8 is `FUN_141211340` (`TRIBE_Combat_Object`).
        const TRIBE_COMBAT: MasterKind = MasterKind::COMBAT;

        let Some((p, u)) = self.object(id) else {
            return Ok(());
        };
        let master = u.master(self.master_tables(), p)?;
        let spared = TRIBE_COMBAT <= master.kind
            && (self.model.roles.of_class(master.class).contains(ClassRoles::ALWAYS_SPARED)
                // The class `FUN_140c89760` lets scout without the master's flag when a game setting allows it
                // (raw `0x3a`).
                // The town centre (master 109, `0x6109` raw), to which livestock's `MoveTo` goes as to a point
                // (`FUN_140aa0df0`).
                || (self.model.roles.of_class(master.class).contains(ClassRoles::LIVESTOCK) && Amount::ZERO < u.life.amount));
        if spared {
            return Ok(());
        }
        let unit = match self.place(id).ok_or_else(|| object_gone(id))? {
            Place::Live(p, i) => self.players.live_mut(p, i)?,
            Place::Sleeping(p, i) => {
                let woken = self.swap_remove_asleep(p, i)?;
                let (id, master) = (woken.unit.id, woken.unit.life.master_id);
                let i = self.push_live(p, woken)?;
                // the wake's remove and add (slots 0x2d8, 0x2d0) file it last in the tracker
                self.track_relist(p, id, master)?;
                self.players.live_mut(p, i)?
            }
        };
        unit.life.goto_sleep = false;
        unit.life.hp = HitPoints::ZERO;
        Ok(())
    }

    /// Slot 0x90 (`FUN_141225b70` with 0): the unit changes its master. Its action list is
    /// cleared; in a state below 3 (`FUN_1411a6a70`) the old master's storage leaves its player's
    /// attributes and the new one's comes in (`FUN_1411a56c0`, flag 2), its hit points keep their
    /// share of the new master's (whole, at least 1), its sight goes and the new master's comes
    /// (slots 0x1d8, 0x1d0), its place and obstruction record take the new master's
    /// (`FUN_141055b40`, `FUN_14118c540`) and it moves with the new master's values; its powerups
    /// go ([`clear_powerups`]) and in state 2 its volleys (`+0xa00`) are the new master's volley
    /// fire; it takes the new master's standing sprite (slot 0x68). Its
    /// per-master counts, facet, class registry (`DAT_143f69170`) and particles are not held.
    pub(crate) fn change_master(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        new: MasterId,
    ) -> Result<(), GenieError> {
        self.clear_actions(player, idx)?;
        let unit = &self.players.live(player, idx)?;
        let old = unit.master(self.master_tables(), player)?.clone();
        let next = self.master(player, new)?.clone();
        let world = SwitchWorld {
            spatial: &mut self.spatial,
            pathing: &mut self.pathing,
            graphics: &self.model.graphics,
            sprite: self.streams.get(Stream::Sprite),
        };
        let unit = self.players.live_mut(player, idx)?;
        let (combat, motion) = self.parts.combat_and_motion_mut(unit.id)?;
        let switched = switch_master(
            (unit, combat, motion),
            &old,
            &next,
            world,
            &self.model.roles,
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
}

/// What a master change ([`switch_master`]) changes of the world besides its unit: the place and
/// its records, the pathing, and the sprites with the Sprite stream.
pub(crate) struct SwitchWorld<'a> {
    pub(crate) spatial: &'a mut Spatial,
    pub(crate) pathing: &'a mut Pathing,
    pub(crate) graphics: &'a Graphics,
    pub(crate) sprite: &'a mut Well512,
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

/// Slot 0x90's unit part (`FUN_141225b70` with 0) of a master change from `old` to `next` of
/// `unit` with its combat layer `combat` and its motion layer `motion`, its action list cleared by the caller: in a state below 3 (`FUN_1411a6a70`) the old master's
/// storage leaves its player's attributes and the new one's comes in (`FUN_1411a56c0`, flag 2:
/// the owner's, [`Switched`]), its hit points keep their share of the new master's (whole, at
/// least 1), its sight goes and the new master's comes (slots 0x1d8, 0x1d0), its place and
/// obstruction record take the new master's (`FUN_141055b40`, `FUN_14118c540`) and it moves with
/// the new master's values; it takes the new master's standing sprite (slot 0x68). Then its
/// powerups go ([`clear_powerups`]) and in state 2 its volleys (`+0xa00`) are the new master's
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
    roles: &Roles,
) -> Result<Switched, GenieError> {
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
        if (old.damage_sprites || next.damage_sprites)
            && (damage_share(old_hp, old.hit_points, old.class, roles)?
                != damage_share(unit.life.hp, next.hit_points, next.class, roles)?)
        {
            return Err(GenieError::unsupported(
                "a master change that changes the damage sprites (FUN_1411859a0)",
            ));
        }
        unit.life.master_id = new;
        let moving = motion.mover().is_some_and(Mover::is_moving);
        if let Some(m) = motion.mover_mut() {
            m.stats = next.moving.ok_or(GenieError::unsupported(
                "a moving unit's new master does not move",
            ))?;
            // the top speed the unit reads with the sprite it shows, from the new master's base
            if let Some(shown) = unit.sprite.as_ref().map(|s| &s.graphic) {
                m.stats.show(Some(shown));
            }
        }
        let id = unit.id;
        if let Some(mut placed) = world.spatial.get(id).copied() {
            placed.radius = next.radius;
            placed.class = next.class;
            placed.master = new;
            placed.obstruction_kind = next.obstruction_kind;
            placed.buildable_on = next.buildable_on;
            placed.tall = next.tall;
            placed.moves = next.moves();
            if unit.life.inside.is_some() {
                // inside another (`+0x2f0`) the unit has no tile (`+0x2e8` 0: no sight slots) and
                // is in no list nor the obstruction manager: its record takes the new master's
                // values for its way out
                world.spatial.set_inside_record(id, placed);
                if let Some(v) = world.spatial.sight.as_mut() {
                    v.seer_master(id, new, next.line_of_sight);
                }
            } else {
                if let Some(o) = placed.obstruction.as_mut() {
                    o.kind = next.obstruction_kind;
                    o.class = next.obstruction_class;
                }
                world.pathing.removed(id);
                world
                    .spatial
                    .sight_master(id, next.radius, next.class, new, next.line_of_sight)?;
                world.spatial.change_master(id, placed);
                world.pathing.moved(id, &placed, moving);
            }
        }
        // slot 0x68 with the new master's standing sprite (`+0x470`, FUN_141217000): a new
        // instance unless the unit shows it already. The slice holds sprites from an attack
        // on; a unit whose sprite it does not hold shows its old master's. Without the
        // graphics (no data file) the Sprite stream is not modelled.
        if !world.graphics.is_empty() && old.standing_sprite != next.standing_sprite {
            let g = next
                .standing_sprite
                .and_then(|s| world.graphics.get(s).copied())
                .ok_or(GenieError::unsupported(
                    "a new master's standing sprite the data file does not have",
                ))?;
            if unit.sprite.is_some() {
                new_sprite(&mut unit.sprite, motion.move_stats_mut(), g, world.sprite);
            } else {
                let _ = ActiveSprite::start(g, world.sprite);
            }
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
    clear_powerups(combat);
    // in state 2 the volleys (`+0xa00`) are the new master's volley fire, the shares of the
    // units inside dropped
    if unit.life.state == ObjectState::Alive {
        combat.firepower = next.volley_fire;
    }
    Ok(switched)
}
