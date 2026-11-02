//! A gate's and a farm's update, a building's completion and seconds, the idle swap and the
//! death blast.

use crate::GenieError;
use crate::combat::damage::Victim;
use crate::combat::victims::{victim_of, write_back};
use crate::construction::gates::gate_units;
use crate::map::TerrainId;
use crate::map::spatial::Spatial;
use crate::math::rng::Well512;
use crate::movement::MoveAlgorithm;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Relics;
use crate::objects::SpriteId;
use crate::objects::layers::MotionMut;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ClassId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::HOUSE;
use crate::objects::masters::ids::MONASTERY;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::world::Millis;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use crate::world::{
    BUILDING_TERRAIN, EJECT_SHARE, GATE_CLOSE_DELAY, GATE_EPSILON, GATE_SHUT_HEIGHT,
};

/// `TRIBE_Building_Object::update`'s walls' branch (`0x1411c9000`): a master of class 27 (a
/// wall) or the house runs only its damage frames by its hit points and its combat update; no
/// whole seconds, and an annex of it completes as well ([`Unit::complete_on_update`]).
pub(crate) fn walls_branch(master: &Master) -> bool {
    // The house's master id (`0x46`), whose update takes the walls' branch ([`walls_branch`]).
    master.class == class::WALL || master.id == HOUSE
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
    pub(crate) fn let_removed_links_go(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        if master.kind != BUILDING || walls_branch(master) {
            return Ok(());
        }
        let (players, index) = (&c.w.players, &c.w.index);
        let removed = |id: ObjectId| {
            index
                .locate(players, id)
                .map(|(_, u, _)| ObjectState::Removed <= u.life.state)
                .ok_or_else(|| {
                    GenieError::unreachable(
                        "a building's link to an object the world does not file",
                    )
                    .with("object", id)
                })
        };
        if let Some(main) = c.w.parts.building_of(self.id).annex_of
            && removed(main.id())?
        {
            c.w.parts.building_mut(self.id)?.annex_of = None;
        }
        c.w.parts.building_mut(self.id)?.annexes.let_go(removed)
    }

    /// The gate's update (`FUN_1411e7730`, `TRIBE_Building_Object::update` for a master of class
    /// 39 outside the walls' branch): a gate in state 2 whose master transforms
    /// (`transform_unit`, `+0x9ba`) clears its first update (its endpoints, not held). Unless it
    /// is shut (its master's height above [`GATE_EPSILON`]) and locked, it counts the units in it
    /// ([`Unit::gate_count`]): a shut, unlocked gate with one of an ally (its player counting the
    /// unit's an ally) in it opens (its player's slot 0x340, `FUN_14162f7e0`: a gate of a height
    /// of at least 1, [`World::toggle_gate`], within the update) and its close timer is 0; an
    /// open one (a height below the epsilon) with nothing in it starts its close timer at 0.5 s,
    /// or runs it down by the world's dt, and at 0 shuts (slot 0x338, `FUN_1416193e0`: a height
    /// of at most 0).
    pub(crate) fn gate_update(
        &mut self,
        actions: &mut crate::objects::layers::Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        let transforms = master
            .building
            .as_ref()
            .is_some_and(|b| b.transform_unit.is_some());
        if master.class != class::GATE || !transforms || self.life.state != ObjectState::Alive {
            return Ok(());
        }
        let height = master.height.get();
        let gate =
            c.w.parts
                .building_mut(self.id)?
                .gate
                .as_mut()
                .ok_or(GenieError::unsupported("a gate without its gate state"))?;
        gate.first_update = false;
        let shut = GATE_EPSILON < height;
        if gate.locked && shut {
            return Ok(());
        }
        let locked = gate.locked;
        let annexes = c.w.parts.building_of(self.id).annexes;
        let by_id = crate::world::index::ObjectsRef::new(
            &c.w.players,
            &c.w.index,
            &c.w.masters,
            &c.w.parts,
        );
        let (total, allied) = self.gate_count(annexes, &mut c.w.spatial, by_id)?;
        let gate =
            c.w.parts
                .building_mut(self.id)?
                .gate
                .as_mut()
                .ok_or(GenieError::unreachable(
                    "a gate whose state went during its update",
                ))?;
        if shut && 0 < allied && !locked {
            gate.close_timer = Seconds::ZERO;
            return self.toggle_own_gate(actions, c, GATE_SHUT_HEIGHT <= height);
        }
        if height < GATE_EPSILON && total == 0 {
            if gate.close_timer <= Seconds::ZERO {
                gate.close_timer = GATE_CLOSE_DELAY;
            } else {
                gate.close_timer -= c.dt.seconds();
                if gate.close_timer <= Seconds::ZERO {
                    gate.close_timer = Seconds::ZERO;
                    return self.toggle_own_gate(actions, c, height <= 0.0);
                }
            }
        }
        Ok(())
    }

    /// The gate's player's toggle within the gate's update when `toggles` (slots 0x340 and 0x338,
    /// [`crate::world::World::toggle_gate`]), the gate lent to the world.
    ///
    /// # Errors
    ///
    /// The toggle's, or the gate gone from its owner's live list.
    fn toggle_own_gate(
        &mut self,
        actions: &mut crate::objects::layers::Action,
        c: &mut Step<'_>,
        toggles: bool,
    ) -> Result<(), GenieError> {
        if !toggles {
            return Ok(());
        }
        let (p, id) = (c.player, self.id);
        c.lend((&mut *self, &mut *actions), |w| {
            let i = w.live_index(p, id).ok_or_else(|| {
                GenieError::unreachable("a lent gate left its list").with("object", id)
            })?;
            w.toggle_gate(p, i)
        })
    }

    /// [`gate_units`] of this gate between its `annexes` (its slots, `+0xbc0`..): the units in
    /// it and how many of them are of an ally.
    pub(crate) fn gate_count(
        &self,
        annexes: crate::construction::Annexes,
        spatial: &mut Spatial,
        by_id: crate::world::index::ObjectsRef<'_>,
    ) -> Result<(usize, usize), GenieError> {
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
    pub(crate) fn farm_stage(&self, c: &mut Step<'_>) -> Result<(), GenieError> {
        let Some(cons) = c.w.parts.building_of(self.id).construction.filter(|k| {
            self.life.state == ObjectState::Foundation && WholeSeconds::ZERO < k.build_time
        }) else {
            return Ok(());
        };
        let m = self.master(c.w.master_tables(), c.player)?;
        let foundation = m.building.as_ref().and_then(|b| b.foundation_terrain);
        let Some(terrain) = farm_stage_terrain(
            m.id,
            (c.w.parts.building_of(self.id).rice, foundation),
            cons.progress,
            cons.build_time,
        ) else {
            return Ok(());
        };
        if matches!(terrain.get(), 8 | 27 | 64) {
            return Err(GenieError::unsupported(
                "a farm's stage terrain the local player's view maps (8, 27, 64)",
            ));
        }
        let f = crate::map::terrain::Foundation {
            position: self.position(),
            clearance: m.clearance,
            terrain,
            snow: false,
            master: m.id,
            class: m.class,
            terrain_bonus: m.terrain_bonus,
            mode_680: c.w.mode_680,
        };
        c.w.spatial
            .terrain
            .as_mut()
            .ok_or(GenieError::unsupported(
                "a farm's foundation without the map's terrain",
            ))?
            .cover(&f)
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
        actions: &mut crate::objects::layers::Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        self.farm_stage(c)?;
        let due = self.life.state == ObjectState::Foundation
            && c.w
                .parts
                .building_of(self.id)
                .construction
                .is_some_and(|cons| Seconds::of_master(cons.build_time) <= cons.progress);
        if !due {
            return Ok(());
        }
        let master = self.master(c.w.master_tables(), c.player)?;
        if !walls_branch(master) && c.w.parts.building_of(self.id).annex_of.is_some() {
            return Ok(());
        }
        let (p, id) = (c.player, self.id);
        c.lend((&mut *self, &mut *actions), |w| {
            w.set_state(p, id, ObjectState::Alive)
        })
    }

    /// `TRIBE_Building_Object::update`'s whole seconds of a building in state 2 (`0x1411c9000`,
    /// before its combat update), outside the walls' branch ([`walls_branch`], which counts
    /// none): `FUN_1411c39b0` (attribute 255 naming its master: a terrain
    /// effect around it, not modelled); then for each whole second since `last_garrison_time`
    /// (`+0xbe0`, which moves on by them): a monastery's (master 104) relics give their goods
    /// (not modelled with relics inside); each unit inside (from the last) whose
    /// `raider_build_countdown` (`+0x9a0`) runs would be let out at its end (not modelled); the
    /// units inside (from the first) heal by the master's `garrison_heal_rate` below their
    /// master's hit points (slot 0x110, [`Unit::heal`]); with villagers inside, a damaged
    /// building with a `garrison_repair_rate` repairs itself at a cost (`FUN_1411d33e0`, not
    /// modelled). Its `damaged_lately_timer` (`+0x30d`) loses one when whole seconds passed.
    pub(crate) fn building_seconds(&mut self, c: &mut Step<'_>) -> Result<(), GenieError> {
        if self.life.state != ObjectState::Alive {
            return Ok(());
        }
        let Ok(master) = object_master(
            c.w.parts.own_master.get(self.id),
            (self.id, self.life.master_id),
            c.w.masters.of(c.player),
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
            <= HitPoints::new(HitPoints::of_master(master.hit_points).get() * EJECT_SHARE)
            && !self.garrison.is_empty()
        {
            return Err(GenieError::unsupported(
                "a damaged building lets its garrison out (a fifth of its hit points)",
            ));
        }
        // the walls' branch runs only its damage frames and its combat update
        if walls_branch(master) {
            return Ok(());
        }
        if c.w
            .players
            .player(c.player)?
            .attributes
            .get(BUILDING_TERRAIN)
            == Some(&Amount::new(f32::from(master.id.get())))
        {
            return Err(GenieError::unsupported(
                "a building whose master attribute 255 names (FUN_1411c39b0)",
            ));
        }
        let seconds =
            c.w.world_time
                .since(c.w.parts.building_of(self.id).garrison_time)
                .get()
                / 1000;
        if seconds == 0 {
            return Ok(());
        }
        // the master's own copy lives beside the layer: what the rest reads of it, first
        let (master_id, master_hp) = (master.id, master.hit_points);
        let layer = c.w.parts.building_mut(self.id)?;
        layer.garrison_time = layer.garrison_time.after(Millis::new(seconds * 1000));
        if self.damaged_lately.running() {
            self.damaged_lately = self.damaged_lately.tick();
        }
        if master_id == MONASTERY && c.w.parts.building_of(self.id).relics != Relics::NONE {
            return Err(GenieError::unsupported(
                "a monastery's relics' goods (its seconds)",
            ));
        }
        for &id in self.garrison.iter().rev() {
            let Some(u) = OthersRef::of(&c.w.players, &c.w.index, c.player)?.own(id) else {
                continue;
            };
            let kind = u.master(c.w.master_tables(), c.player)?.kind;
            if kind == COMBAT && c.w.parts.combat_of(u.id).raider_countdown.runs() {
                return Err(GenieError::unsupported(
                    "a unit inside whose raider_build_countdown runs (FUN_1411e7220)",
                ));
            }
        }
        let rate = bm.garrison_heal_rate;
        let mut villagers = 0;
        for &id in &self.garrison {
            let _ = c.w.players.player(c.player)?;
            let Some(i) = c.w.live_index(c.player, id) else {
                continue;
            };
            let u = c.w.players.live_mut(c.player, i)?;
            let m = object_master(
                c.w.parts.own_master.get(u.id),
                (u.id, u.life.master_id),
                c.w.masters.of(c.player),
            )?;
            // The villager's class (4), which a building's repair by its garrison counts.
            if m.class == class::VILLAGER {
                villagers += 1;
            }
            let (class, hit_points) = (m.class, m.hit_points);
            if HitPoints::ZERO < rate && u.life.hp < HitPoints::of_master(hit_points) {
                u.heal(c.w.parts.combat_mut(u.id)?, class, hit_points, rate)?;
            }
        }
        if 0 < villagers
            && HitPoints::ZERO < bm.garrison_repair_rate
            && self.life.hp < HitPoints::new(HitPoints::of_master(master_hp).get())
        {
            return Err(GenieError::unsupported(
                "a building its villagers inside repair (FUN_1411d33e0)",
            ));
        }
        Ok(())
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
        graphics: &crate::objects::Graphics,
        sprite_rng: &mut Well512,
    ) {
        let Some(second) = second else {
            return;
        };
        if motion.mover().is_some_and(|m| {
            m.stats
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
        if let Some(&g) = next.and_then(|next| graphics.get(next)) {
            crate::objects::sprite::new_sprite(
                &mut self.sprite,
                motion.move_stats_mut(),
                g,
                sprite_rng,
            );
        }
        if let Some(s) = self.sprite.as_mut() {
            s.reset_frame();
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
        actions: &mut crate::objects::layers::Action,
        c: &mut Step<'_>,
    ) -> Result<(), GenieError> {
        let master = self.master(c.w.master_tables(), c.player)?;
        if !blasts_at_death(master, &self.life) {
            return Ok(());
        }
        let terrain = c.w.spatial.terrain.as_ref();
        let mut own = victim_of(
            (
                self,
                c.w.parts.unit_ai.contains(self.id),
                c.w.parts.building_of(self.id),
                c.w.parts.combat_of(self.id),
                actions,
            ),
            master,
            &c.w.players.player(c.player)?.attributes,
            c.player,
            terrain,
            c.w.mode_680,
        );
        let mut targets: Vec<&mut Victim> = c.blasted.iter_mut().collect();
        let _ = crate::combat::damage::blast(
            &mut own,
            self.position(),
            Some(self.id),
            None,
            &mut targets,
            DEATH_BLAST_FACTOR,
        )?;
        let mut blasted = std::mem::take(&mut c.blasted);
        for v in &mut blasted {
            if let Some(k) = write_back(c, v)? {
                crate::combat::victims::file_kill(c, (self, actions), k)?;
            }
        }
        c.blasted = blasted;
        let _ = self.hit_back(own, c.w.parts.ai_and_combat_mut(self.id)?)?;
        Ok(())
    }

    /// `RGE_Moving_Object::update`: a last human order time (`+0x650`) at most `FUN_140fe0d10`'s
    /// counts down by the world's dt (`+0x7c`, in ms), not below 0. The walk reads the field
    /// live, so the world's view follows at once: the unit's own walk later in this update ranks
    /// the units it meets by the counted-down time.
    pub(crate) fn count_down_human_order(&mut self, dt: Millis) {
        if self.last_human_order <= crate::unit_ai::HUMAN_ORDER_CAP {
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
) -> Option<TerrainId> {
    /// The stages over the build time (`DAT_142eb1c98`).
    const STAGES: f32 = crate::math::image::turn::FARM_STAGES;
    /// A farm's first stage terrain (`0x1d`).
    const FIRST_STAGE: u8 = 29;
    /// A rice farm's first stage terrain on mangrove or rice (`0x41`).
    const RICE_STAGE: u8 = 65;
    let stage =
        (((progress.get() * STAGES) / Seconds::of_master(build_time).get()) as i32 as u8).min(2);
    let stage_byte = match master {
        ids::FARM => FIRST_STAGE + stage,
        ids::ID_1187 | ids::PASTURE_1889 if rice => RICE_STAGE + stage,
        // none is the short −1, whose low byte is 255
        ids::ID_1187 | ids::PASTURE_1889 => foundation
            .map_or(0xff, TerrainId::get)
            .wrapping_add(2)
            .wrapping_add(stage),
        _ => return None,
    };
    Some(TerrainId::named(stage_byte))
}

/// The masters whose units blast about themselves below 1 hit point
/// (`TRIBE_Combat_Object::update`, `0x141208440`): the demolition ships 527 and 528, the
/// demolition raft 1104 and the incendiary raft and ships 2133 to 2135.
pub(crate) const DEATH_BLAST_MASTERS: [MasterId; 6] = [
    MasterId::named(527),
    MasterId::named(528),
    MasterId::named(1104),
    MasterId::named(2133),
    MasterId::named(2134),
    MasterId::named(2135),
];
/// The classes whose units blast so: 35 (petards) and 64.
pub(crate) const DEATH_BLAST_CLASSES: [ClassId; 2] = [class::CLASS_35, class::CLASS_64];
/// The hit points below which they blast (`DAT_14330eb8c`; `COMISS`, a NaN does not).
pub(crate) const DEATH_BLAST_HP: f32 = crate::math::image::turn::DEATH_BLAST_HP;
/// The blast's damage factor (`XMM11`, the same constant).
pub(crate) const DEATH_BLAST_FACTOR: f32 = crate::math::image::turn::DEATH_BLAST_FACTOR;

/// `TRIBE_Combat_Object::update`'s test for the blast at death: a unit of one of
/// [`DEATH_BLAST_MASTERS`] or [`DEATH_BLAST_CLASSES`] below [`DEATH_BLAST_HP`] hit points in state
/// 2. The update runs for the combat objects and buildings (types 70 and 80).
pub(crate) fn blasts_at_death(master: &Master, life: &Lifecycle) -> bool {
    COMBAT <= master.kind
        && life.hp < HitPoints::new(DEATH_BLAST_HP)
        && life.state == ObjectState::Alive
        && (DEATH_BLAST_MASTERS.contains(&master.id) || DEATH_BLAST_CLASSES.contains(&master.class))
}
