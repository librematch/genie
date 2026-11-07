//! A gate's and a farm's update, a building's completion and seconds, the idle swap and the
//! death blast.

use crate::combat::damage::HitLayer;
use crate::combat::damage::Victim;
use crate::combat::damage::blast;
use crate::combat::victims::file_kill;
use crate::combat::victims::{ChargeOf, VictimLayers, VictimView};
use crate::combat::victims::{victim_of, write_back};
use crate::construction::Annexes;
use crate::construction::gates::gate_units;
use crate::map::Snow;
use crate::map::TerrainId;
use crate::map::spatial::Spatial;
use crate::map::terrain::Foundation;
use crate::math::rng::Well512;
use crate::movement::MoveAlgorithm;
use crate::objects::Graphics;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::SpriteId;
use crate::objects::layers::Action;
use crate::objects::layers::MotionMut;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::player_masters;
use crate::objects::sprite::new_sprite;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::players::game_status::GameStatus;
use crate::unit_ai::HUMAN_ORDER_CAP;
use crate::world::ClockSeconds;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::SpriteTuning;
use crate::world::layer::generated::TurnTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::world::{BUILDING_TERRAIN, GATE_CLOSE_DELAY, GATE_EPSILON};
use crate::{GenieError, GenieResult};
use std::mem::take;

/// A farm's foundation without the map's terrain, which the foundation covers.
pub(crate) const FARM_TERRAIN_LACKING: GenieError =
    GenieError::unsupported("a farm's foundation without the map's terrain");

/// `TRIBE_Building_Object::update`'s walls' branch (`0x1411c9000`): a master of class 27 (a
/// wall) or the house runs only its damage frames by its hit points and its combat update; no
/// whole seconds, and an annex of it completes as well ([`Unit::complete_on_update`]).
pub(crate) fn walls_branch(master: &Master, roles: &Roles) -> bool {
    // The house's master id (`0x46`), whose update takes the walls' branch ([`walls_branch`]).
    roles.of_class(master.class).contains(ClassRoles::WALL)
        || roles.of_master(master.id).contains(MasterRoles::HOUSE)
}

impl Unit {
    /// `TRIBE_Building_Object::update` (`0x1411c9000`) outside the walls' branch, before the
    /// gate's update: a building's link to its main building (`+0xbb8`) and each of its annex
    /// slots (`+0xbc0`..`+0xbd8`) whose object is past state 6 (`+0x640`: removed or gone) let
    /// it go. A building updates in every update, so it meets each linked object in state 7 or 8
    /// before that object leaves its list.
    ///
    /// # Errors
    ///
    /// A link to an object the world does not file.
    pub(crate) fn let_removed_links_go(
        &self,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        if master.kind != MasterKind::BUILDING || walls_branch(master, roles) {
            return Ok(());
        }
        let (players, index) = (&step.world.players, &step.world.index);
        let removed = |id: ObjectId| {
            index
                .locate(players, id)
                .map(|(_, unit, _)| ObjectState::Removed <= unit.life.state)
                .ok_or_else(|| {
                    // a link clears as its object dies (`Unit::died`'s annexes) and here once it
                    // is removed, before the world lets the object go
                    GenieError::unreachable(
                        "a building's link to an object the world does not file",
                    )
                    .with("object", id)
                })
        };
        if let Some(main) = step.world.parts.building_of(self.id).annex_of
            && removed(main.id())?
        {
            step.world.parts.building_mut(self.id)?.annex_of = None;
        }
        step.world
            .parts
            .building_mut(self.id)?
            .annexes
            .let_go(removed)
    }

    /// The gate's update (`FUN_1411e7730`, `TRIBE_Building_Object::update` for a master of class 39
    /// outside the walls' branch): a gate in state 2 whose master transforms (`transform_unit`,
    /// `+0x9ba`) clears its first update (its endpoints, not held). Unless it is shut (its master's
    /// height above [`GATE_EPSILON`]) and locked, it counts the units in it ([`Unit::gate_count`]):
    /// a shut, unlocked gate with one of an ally (its player counting the unit's an ally) in it
    /// opens (its player's slot 0x340, `FUN_14162f7e0`: a gate of a height of at least 1,
    /// [`World::toggle_gate`](crate::world::World::toggle_gate), within the update) and its close
    /// timer is 0; an open one (a height below the epsilon) with nothing in it starts its close
    /// timer at 0.5 s, or runs it down by the world's dt, and at 0 shuts (slot 0x338,
    /// `FUN_1416193e0`: a height of at most 0).
    pub(crate) fn gate_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        let transforms = master
            .building
            .as_ref()
            .is_some_and(|building_master| building_master.transform_unit.is_some());
        if !roles.of_class(master.class).contains(ClassRoles::GATE)
            || !transforms
            || self.life.state != ObjectState::Alive
        {
            return Ok(());
        }
        let height = master.height.get();
        let layer = step.world.parts.building_mut(self.id)?;
        let mut gate = layer
            .gate
            .ok_or(GenieError::unsupported("a gate without its gate state"))?;
        gate.first_update = false;
        layer.gate = Some(gate);
        let shut = GATE_EPSILON < height;
        if gate.locked && shut {
            return Ok(());
        }
        let annexes = step.world.parts.building_of(self.id).annexes;
        let by_id = ObjectsRef::new(
            &step.world.players,
            &step.world.index,
            &step.world.model.masters,
            &step.world.parts,
            &step.world.model.roles,
            &step.world.model.tuning,
        );
        // the count only reads the parts: the layer holds `gate` still, written back before the
        // toggle
        let (total, allied) = self.gate_count(annexes, &mut step.world.spatial, by_id)?;
        let toggles = if shut && 0 < allied && !gate.locked {
            gate.close_timer = Seconds::ZERO;
            step.world.model.tuning.turn.gate_shut_height <= height
        } else if height < GATE_EPSILON && total == 0 {
            if gate.close_timer <= Seconds::ZERO {
                gate.close_timer = GATE_CLOSE_DELAY;
                false
            } else {
                gate.close_timer -= step.dt.seconds();
                let ends = gate.close_timer <= Seconds::ZERO;
                if ends {
                    gate.close_timer = Seconds::ZERO;
                }
                ends && height <= 0.0
            }
        } else {
            false
        };
        step.world.parts.building_mut(self.id)?.gate = Some(gate);
        self.toggle_own_gate(actions, step, toggles)
    }

    /// The gate's player's toggle within the gate's update when `toggles` (slots 0x340 and 0x338,
    /// [`crate::world::World::toggle_gate`]), the gate lent to the world.
    ///
    /// # Errors
    ///
    /// The toggle's, or the gate gone from its owner's live list.
    fn toggle_own_gate(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        toggles: bool,
    ) -> GenieResult<()> {
        if !toggles {
            return Ok(());
        }
        let (player_id, id) = (step.player, self.id);
        step.lend((&mut *self, &mut *actions), |world| {
            // the lend puts the gate back in its slot for the call
            let list_index = world.live_index(player_id, id).ok_or_else(|| {
                GenieError::unreachable("a lent gate left its list").with("object", id)
            })?;
            world.toggle_gate(player_id, list_index)
        })
    }

    /// [`gate_units`] of this gate between its `annexes` (its slots, `+0xbc0`..): the units in
    /// it and how many of them are of an ally.
    pub(crate) fn gate_count(
        &self,
        annexes: Annexes,
        spatial: &mut Spatial,
        by_id: ObjectsRef<'_>,
    ) -> GenieResult<(usize, usize)> {
        let annexes: Vec<ObjectId> = annexes.ids().collect();
        let (units, allied) = gate_units(spatial, self.id, &annexes, by_id)?;
        Ok((units.len(), allied))
    }

    /// `TRIBE_Building_Object::update`'s farm part for a foundation (state 0) with a build time
    /// (`0x1411ca2ef`): its stage, its build points times 3 over its build time (truncated, at
    /// most 2), lays terrain under it each update (`FUN_1411ce250` -> `FUN_1411ddd90`,
    /// [`crate::map::terrain::Terrain::cover`]): 29 plus the stage under a farm (master 50); under a
    /// rice farm or a pasture (1187, 1889) 65 plus the stage on mangrove or rice terrain (`+0xcd0`,
    /// [`Building::rice`](crate::objects::layers::Building::rice)), else its foundation terrain (`+0x988`) plus 2 plus the stage (bytes).
    /// `FUN_1411ce250` maps the terrains 8, 27 and 64 by the local player's view and the tile's
    /// own (`+0x2e8`'s `+10`), which the slice does not hold; the masters' foundations (7, 63,
    /// 117) never give them.
    pub(crate) fn farm_stage(&self, step: &mut Step<'_>, roles: &Roles) -> GenieResult<()> {
        let Some(cons) =
            step.world
                .parts
                .building_of(self.id)
                .construction
                .filter(|construction| {
                    self.life.state == ObjectState::Foundation
                        && WholeSeconds::ZERO < construction.build_time
                })
        else {
            return Ok(());
        };
        let master = self.master(step.world.master_tables(), step.player)?;
        let foundation = master
            .building
            .as_ref()
            .and_then(|building_master| building_master.foundation_terrain);
        let Some(terrain) = farm_stage_terrain(
            master.id,
            (step.world.parts.building_of(self.id).rice, foundation),
            cons.progress,
            cons.build_time,
            roles,
            &step.world.model.tuning.turn,
        ) else {
            return Ok(());
        };
        if matches!(terrain.get(), 8 | 27 | 64) {
            return Err(GenieError::unsupported(
                "a farm's stage terrain the local player's view maps (8, 27, 64)",
            ));
        }
        let foundation = Foundation {
            position: self.position(),
            clearance: master.clearance,
            terrain,
            snow: Snow::None,
            master: master.id,
            class: master.class,
            terrain_bonus: master.combat_part().terrain_bonus,
            mode_680: step.world.mode_680,
        };
        step.world
            .spatial
            .terrain
            .as_mut()
            .ok_or(FARM_TERRAIN_LACKING)?
            .cover(&foundation, roles, step.world.model.tuning.terrain)
    }

    /// `TRIBE_Building_Object::update` (`0x1411c9000`) before its combat update: a building in
    /// state 0 whose build points (`+0xc50`, the construction's progress) reached its build time
    /// (its master's first train time, `FUN_140ace5e0`) completes (`set_state(2)`: its annexes
    /// first, then [`crate::world::World::complete_building`], the building lent to the world);
    /// outside the walls' branch (class 27, master 70) only one that is not an annex (`+0xbb8`).
    /// A builder's work completes a foundation itself (`FUN_1411cc430`); this one completes a
    /// building made without a build time (a death spawn's). The walls' branch's facet and the
    /// other branch's work before it are not modelled for such a building.
    ///
    /// # Errors
    ///
    /// What the completion does not model.
    pub(crate) fn complete_on_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.farm_stage(step, roles)?;
        let due = self.life.state == ObjectState::Foundation
            && step
                .world
                .parts
                .building_of(self.id)
                .construction
                .is_some_and(|cons| Seconds::of_master(cons.build_time) <= cons.progress);
        if !due {
            return Ok(());
        }
        let master = self.master(step.world.master_tables(), step.player)?;
        if !walls_branch(master, roles) && step.world.parts.building_of(self.id).annex_of.is_some()
        {
            return Ok(());
        }
        let (player_id, id) = (step.player, self.id);
        step.lend((&mut *self, &mut *actions), |world| {
            world.set_state(player_id, id, ObjectState::Alive)
        })
    }

    /// `TRIBE_Building_Object::update` (`0x1411c9000`) before its combat update, for any object
    /// (each step tests its own): the links to removed objects ([`Unit::let_removed_links_go`]),
    /// a gate's update ([`Unit::gate_update`]), then by the state the update reads once before
    /// its switch (`cVar3`) a built building's whole seconds ([`Unit::building_seconds`]) or a
    /// foundation's completion ([`Unit::complete_on_update`]): a foundation that completes in
    /// this update runs no seconds.
    ///
    /// # Errors
    ///
    /// Each step's.
    pub(crate) fn building_update(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        self.let_removed_links_go(step, roles)?;
        self.gate_update(actions, step, roles)?;
        // the seconds test state 2 before the completion makes it
        self.building_seconds(actions, step, roles)?;
        self.complete_on_update(actions, step, roles)
    }

    /// `TRIBE_Building_Object::update`'s whole seconds of a building in state 2 (`0x1411c9000`,
    /// before its combat update), outside the walls' branch ([`walls_branch`], which counts
    /// none): `FUN_1411c39b0` (attribute 255 naming its master: a terrain
    /// effect around it, not modelled); then for each whole second since `last_garrison_time`
    /// (`+0xbe0`, which moves on by them): a monastery's (master 104) relics give their goods
    /// while its player plays (`+0x89` 0, [`World::relic_goods`], with no relics too); each unit
    /// inside (from the last) whose `raider_build_countdown` (`+0x9a0`) runs would be let out at
    /// its end (not modelled); the units inside (from the first, whoever's) heal by the master's
    /// `garrison_heal_rate` ([`Unit::heal_garrison`]); with villagers inside, a damaged building
    /// with a `garrison_repair_rate` repairs itself at its owner's cost
    /// ([`Unit::repair_by_garrison`]). Its `damaged_lately_timer` (`+0x30d`) loses one when whole
    /// seconds passed.
    ///
    /// # Errors
    ///
    /// What the seconds do not model, and the relic goods', the heal's and the repair's.
    ///
    /// [`World::relic_goods`]: crate::world::World::relic_goods
    pub(crate) fn building_seconds(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        if self.life.state != ObjectState::Alive {
            return Ok(());
        }
        let Ok(master) = object_master(
            step.world.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            step.world.model.masters.of(step.player),
        ) else {
            return Ok(());
        };
        let Some(bm) = master.building else {
            return Ok(());
        };
        // at most a fifth of its hit points (DAT_142eae8d0), it lets its garrison out
        // (FUN_1411e7220 for each from the last, then their slots 0x1a8 and 0xd0); its master's
        // slot 0x110 only sets a sound flag
        if self.life.hp
            <= HitPoints::new(
                HitPoints::of_master(master.hit_points).get()
                    * step.world.model.tuning.turn.eject_share,
            )
            && !self.garrison.is_empty()
        {
            return Err(GenieError::unsupported(
                "a damaged building lets its garrison out (a fifth of its hit points)",
            ));
        }
        // the walls' branch runs only its damage frames and its combat update
        if walls_branch(master, roles) {
            return Ok(());
        }
        if step
            .world
            .players
            .player(step.player)?
            .attributes
            .get(BUILDING_TERRAIN)
            == Some(&Amount::new(f32::from(master.id.get())))
        {
            return Err(GenieError::unsupported(
                "a building whose master attribute 255 names (FUN_1411c39b0)",
            ));
        }
        let seconds = step
            .world
            .world_time
            .since(step.world.parts.building_of(self.id).garrison_time)
            .whole_seconds();
        if seconds == ClockSeconds::ZERO {
            return Ok(());
        }
        // the master's own copy lives beside the layer: what the rest reads of it, first
        let (master_id, master_hp) = (master.id, master.hit_points);
        let layer = step.world.parts.building_mut(self.id)?;
        layer.garrison_time = layer.garrison_time.after(seconds.millis());
        if self.damaged_lately.running() {
            self.damaged_lately = self.damaged_lately.tick();
        }
        if roles.of_master(master_id).contains(MasterRoles::MONASTERY)
            && step.world.players.player(step.player)?.game_status == GameStatus::Playing
        {
            let relics = step.world.parts.building_of(self.id).relics;
            step.world.relic_goods(step.player, relics, seconds)?;
        }
        for &id in self.garrison.iter().rev() {
            let Some(unit) =
                OthersRef::of(&step.world.players, &step.world.index, step.player)?.own(id)
            else {
                continue;
            };
            let kind = unit.master(step.world.master_tables(), step.player)?.kind;
            if kind == MasterKind::COMBAT
                && step.world.parts.combat_of(unit.id).raider_countdown.runs()
            {
                return Err(GenieError::unsupported(
                    "a unit inside whose raider_build_countdown runs (FUN_1411e7220)",
                ));
            }
        }
        let villagers = self.heal_garrison(step, bm.garrison_heal_rate, roles)?;
        self.repair_by_garrison(actions, step, villagers, bm.garrison_repair_rate, master_hp)
    }

    /// The animated object's idle swap (the end of `RGE_Animated_Object::update`,
    /// `0x141071880`) of an object with its motion layer `motion`: an object whose master has a second standing sprite (`+0x478`, `second`;
    /// the first, `+0x470`, is `standing`) and whose
    /// slot 0x5d8 answers 0 (an animated object's always; a moving one's while its master's move
    /// algorithm, `+0x768`, has bit 0 clear), showing either standing sprite, whose sprite's node
    /// looped in this update (the sprite list's slot 0x40, the node's slot 12), draws on the
    /// Sprite stream ("Update"); above 0x3fff it shows the other (`FUN_141189760` with the other
    /// index and 0x7fff: `+0x78c` takes the index, slot 0x68 the master's sprite of that index,
    /// [`crate::objects::sprite::new_sprite`], and the list's slot 0x28 puts its node back to its first
    /// frame, [`crate::objects::sprite::ActiveSprite::reset_frame`]).
    pub(crate) fn idle_swap(
        &mut self,
        mut motion: MotionMut<'_>,
        standing: Option<SpriteId>,
        second: Option<SpriteId>,
        graphics: &Graphics,
        sprite_rng: &mut Well512,
        sprite_tuning: SpriteTuning,
    ) {
        let Some(second) = second else {
            return;
        };
        if motion.mover().is_some_and(|mover| {
            mover
                .stats
                .move_algorithm
                .contains(MoveAlgorithm::ALTERNATES_SPRITES)
        }) {
            return;
        }
        let Some(shown) = self.sprite.as_ref() else {
            return;
        };
        let id = shown.graphic.id;
        if (Some(id) != standing && id != second) || !shown.looped {
            return;
        }
        if sprite_rng.rand15() <= 0x3fff {
            return;
        }
        self.idle_variant = i32::from(self.idle_variant == 0);
        let next = if self.idle_variant == 0 {
            standing
        } else {
            Some(second)
        };
        if let Some(&graphic) = next.and_then(|next| graphics.get(next)) {
            new_sprite(
                &mut self.sprite,
                motion.move_stats_mut(),
                graphic,
                sprite_rng,
                sprite_tuning,
            );
        }
        if let Some(active_sprite) = self.sprite.as_mut() {
            active_sprite.reset_frame();
        }
    }

    /// `TRIBE_Combat_Object::update` (`0x141208440`) before `RGE_Combat_Object::update`: a unit
    /// that blasts at its death ([`blasts_at_death`]) blasts about its place (slot 0x848,
    /// `FUN_14107f690`, [`crate::combat::damage::blast`]: itself left out, no main target, the factor
    /// 1, the hit info `FUN_140aa8720` gives) over the objects its caller gathered
    /// ([`Step::blasted`], written back to the world right after it,
    /// [`write_back`]); its death follows in the static update. Its own part of the blast is
    /// written back ([`Unit::hit_back`]); a kill of it changes nothing, as it dies anyway.
    ///
    /// # Errors
    ///
    /// What the blast does not model.
    pub(crate) fn death_blast(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<()> {
        let master = self.master(step.world.master_tables(), step.player)?;
        if !blasts_at_death(master, &self.life, roles, &step.world.model.tuning.turn) {
            return Ok(());
        }
        let terrain = step.world.spatial.terrain.as_ref();
        let mut own = victim_of(
            VictimView {
                unit: self,
                layers: VictimLayers::of(&step.world.parts, self.id, actions),
                master,
                charge_of: ChargeOf(&|with| step.world.special_charge(self, with)),
                player: step.player,
                masters: player_masters(&step.world.model.masters, step.player)?,
                attributes: &step.world.players.player(step.player)?.attributes,
                terrain,
                mode_680: step.world.mode_680,
            },
            roles,
        )?;
        let mut targets: Vec<&mut Victim> = step.blasted.iter_mut().collect();
        blast(
            &mut own,
            self.position(),
            Some(self.id),
            None,
            &mut targets,
            step.world.model.tuning.turn.death_blast_factor,
            HitLayer {
                roles,
                slots: step.world.model.slots,
                damage_tuning: &step.world.model.tuning.damage,
            },
        )?;
        let mut blasted = take(&mut step.blasted);
        for victim in &mut blasted {
            if let Some(kill_left) = write_back(step, victim)? {
                file_kill(step, (self, actions), kill_left, roles)?;
            }
        }
        step.blasted = blasted;
        self.take_own_master_changes(step, actions, &mut own)?;
        let now = step.world.world_time;
        if let Some(own) = self.hit_back(own, step.world.parts.ai_and_combat_mut(self.id)?, now)? {
            file_kill(step, (self, actions), own, roles)?;
        }
        Ok(())
    }

    /// `RGE_Moving_Object::update`: a last human order time (`+0x650`) at most `FUN_140fe0d10`'s
    /// counts down by the world's dt (`+0x7c`, in ms), not below 0. The walk reads the field
    /// live, so the world's view follows at once: the unit's own walk later in this update ranks
    /// the units it meets by the counted-down time.
    pub(crate) fn count_down_human_order(&mut self, dt: Millis) {
        if self.last_human_order <= HUMAN_ORDER_CAP {
            self.last_human_order = self.last_human_order.saturating_before(dt);
        }
    }
}

/// The terrain a farm foundation of master `master` lays at its stage ([`Unit::farm_stage`]):
/// its build points `progress` times 3 over its `build_time` (truncated, at most 2) onto 29
/// (`0x1d`) for a farm (50), onto 65 (`0x41`) for a rice farm or a pasture (1187, 1889) on
/// mangrove or rice (`rice`), else onto its `foundation` terrain plus 2 (bytes); None for
/// another master.
pub(crate) fn farm_stage_terrain(
    master: MasterId,
    (rice, foundation): (bool, Option<TerrainId>),
    progress: Seconds,
    build_time: WholeSeconds,
    roles: &Roles,
    turn_tuning: &TurnTuning,
) -> Option<TerrainId> {
    /// A farm's first stage terrain (`0x1d`).
    const FIRST_STAGE: u8 = 29;
    /// A rice farm's first stage terrain on mangrove or rice (`0x41`).
    const RICE_STAGE: u8 = 65;
    let stage =
        (((progress.get() * turn_tuning.farm_stages) / Seconds::of_master(build_time).get()) as i32
            as u8)
            .min(2);
    let stage_byte = {
        let held = roles.of_master(master);
        if held.contains(MasterRoles::FARM) {
            FIRST_STAGE + stage
        } else if held.intersects(MasterRoles::RICE_FARM | MasterRoles::PASTURE_1889) && (rice) {
            RICE_STAGE + stage
        } else if held.intersects(MasterRoles::RICE_FARM | MasterRoles::PASTURE_1889) {
            foundation
                .map_or(0xff, TerrainId::get)
                .wrapping_add(2)
                .wrapping_add(stage)
        } else {
            return None;
        }
    };
    Some(TerrainId::named(stage_byte))
}

/// `TRIBE_Combat_Object::update`'s test for the blast at death: a unit of one of
/// [`MasterRoles::DEATH_BLAST`](crate::world::layer::MasterRoles::DEATH_BLAST) or
/// [`ClassRoles::DEATH_BLAST`](crate::world::layer::ClassRoles::DEATH_BLAST) below
/// `TurnTuning::death_blast_hp` hit points in state 2. The update runs for the combat objects and
/// buildings (types 70 and 80).
pub(crate) fn blasts_at_death(
    master: &Master,
    life: &Lifecycle,
    roles: &Roles,
    turn_tuning: &TurnTuning,
) -> bool {
    MasterKind::COMBAT <= master.kind
        && life.hp < HitPoints::new(turn_tuning.death_blast_hp)
        && life.state == ObjectState::Alive
        && (roles
            .of_master(master.id)
            .contains(MasterRoles::DEATH_BLAST)
            || roles
                .of_class(master.class)
                .contains(ClassRoles::DEATH_BLAST))
}
