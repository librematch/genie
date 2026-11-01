//! A new building: BUILD's foundation and the building constructor, with its combat setup,
//! its storage and its annexes.

use crate::GenieError;
use crate::combat::Charge;
use crate::combat::damage::ArmorPair;
use crate::construction::build::Construction;
use crate::map::Distance;
use crate::map::Position;
use crate::map::Tile;
use crate::math::Angle;
use crate::math::rng::Stream;
use crate::movement::Mover;
use crate::objects::HitPoints;
use crate::objects::HpPowerup;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Relics;
use crate::objects::WorkRate;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::Held;
use crate::objects::lifecycle::death_parts::count_class;
use crate::objects::lifecycle::static_storage;
use crate::objects::lifecycle::{Class, Lifecycle};
use crate::objects::masters::MasterId;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::objects::masters::ids::FARM;
use crate::objects::masters::ids::PASTURE_1889;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::{Master, player_master};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::BUILDING_COST;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiType;
use crate::world::Seconds;
use crate::world::WholeSeconds;
use crate::world::WorldTime;
use crate::world::slots::PlayerSlots;
use crate::world::{Gate, World};

impl World {
    /// The new foundation of BUILD and of `BUILD_WALL`'s tiles (the master's slot 0x108,
    /// [`World::make_building`], with the unit AI type the factory gives its master,
    /// [`Master::unit_ai_type`]), then the player pays the master's cost (each entry flagged
    /// paid). Its id and position.
    pub(crate) fn found(
        &mut self,
        player: PlayerId,
        master: MasterId,
        position: Position,
    ) -> Result<(ObjectId, Position), GenieError> {
        let unit_ai = self.master(player, master)?.unit_ai_type(master);
        let made = self.make_building(player, master, Ok(unit_ai), position)?;
        let cost = self.master(player, master)?.cost;
        let owner = self.players.player_mut(player)?;
        for (resource, amount, paid) in crate::objects::masters::cost_entries(&cost) {
            let slot = owner
                .attributes
                .get_mut(resource.index())
                .ok_or(crate::world::NO_ATTRIBUTE)?;
            if paid {
                *slot -= Amount::new(f32::from(amount));
            }
        }
        Ok(made)
    }

    /// [`World::make_building`]'s combat setup (`FUN_14121df60`): its class counters
    /// (`FUN_141226060`), its powerup offset (`+0xa54`, the Object stream's "setup" draw) and,
    /// for a building with a train location (an entry of `+0x898` other than −1), its paid cost
    /// in attribute 247 (slot 0xf8).
    pub(super) fn building_combat_setup(
        &mut self,
        player: PlayerId,
        master: MasterId,
    ) -> Result<(), GenieError> {
        if let Some(m) = player_master(&self.masters, player, master) {
            count_class(&mut self.players.player_mut(player)?.attributes, m, 1.0);
        }
        let _ = self.streams.get(Stream::Object).rand15();
        if let Some(m) = player_master(&self.masters, player, master)
            .filter(|m| m.kind == BUILDING && !m.trained_at.is_empty())
        {
            let paid: i32 = crate::objects::masters::cost_entries(&m.cost)
                .filter(|&(_, _, paid)| paid)
                .map(|(_, amount, _)| i32::from(amount))
                .sum();
            self.players
                .player_mut(player)?
                .add_attribute(BUILDING_COST, Amount::new(paid as f32))?;
        }
        Ok(())
    }

    /// [`World::make_building`]'s static init (`FUN_1411a45a0`): the storage entries of flags 1, 2
    /// and 32 go to the owner (`FUN_141193860`); returns the last entry of flag 0, which the
    /// building holds (`FUN_141193710`; none: −1 and 0).
    pub(super) fn building_storage(
        &mut self,
        player: PlayerId,
        master: MasterId,
    ) -> Result<Held, GenieError> {
        let storage = player_master(&self.masters, player, master)
            .map(|m| m.storage.clone())
            .unwrap_or_default();
        static_storage(self.players.player_mut(player)?, &storage)
    }

    /// A new building (the master's slot 0x40 or 0x108, `FUN_1411ac5c0` / `FUN_1411aca00` with
    /// their init: the static, animated, moving and combat setups, then the constructor
    /// `FUN_1411de5f0`) of `master` for `player` at `position`, the ground's height: its id
    /// (`mNextObjectId`), its unit AI of the given type if its class gets one, the setups' draws
    /// with its master's standing sprite current ([`World::setup_draws`]: the angle it faces), its
    /// movement state ([`Mover::new_object`]), the combat setup's class counters
    /// ([`count_class`]), its powerup offset (the Object stream's "setup" draw)
    /// and a building with a train location its paid cost (attribute 247); the static init's
    /// storage; its place and sight (its place links its tiles to its anchor, `FUN_1411e6780`).
    /// The constructor: masters 50 and 1889 on snow keep their snow (`+0xc61` 2); a master
    /// neither of classes 51, 54 nor 357, 1188, nor an annex (`+0x9b8` −1) takes its build time
    /// as a foundation (a build time above 0: hit points 1, a farm's terrain,
    /// [`World::construction_terrain`]; build points 0), the others are built already (build
    /// points their train time, their master's hit points); one with build points 0 and a
    /// build time above 0, not of classes 51 or 54, turns desolid (`FUN_1411cf770`: no record,
    /// its tiles unlinked); its footprint is cleared ([`World::clear_footprint`]); each annex
    /// slot (`+0x990`) of a master the player has makes its annex the same way at the slot's
    /// offset (`+0x998`, `+0x9a8`), depth first, linked to the building (`+0xbb8`), out of its
    /// owner's watched list unless the building is a gate (39), desolid when its master is tall,
    /// its own footprint cleared; `+0xcd0` notes mangrove or rice terrain under it
    /// (`FUN_141188db0`); last its owner watches it (`FUN_1411cc980`, [`World::watch_started`]).
    /// Not held: the construction sprite and its facets, the world's `+0x154`, slot 0x840's
    /// `+0xba0` and `+0xbf8`, and the wonder's `FUN_1411665c0`.
    ///
    /// # Errors
    ///
    /// A master the player has not got or without its building part, an annex of another type,
    /// and what the clearing and the attributes do not model.
    #[track_caller]
    pub(crate) fn make_building(
        &mut self,
        player: PlayerId,
        master: MasterId,
        unit_ai: Result<Option<UnitAiType>, GenieError>,
        position: Position,
    ) -> Result<(ObjectId, Position), GenieError> {
        let made = self.master(player, master)?.clone();
        let building = made.building.ok_or(GenieError::unsupported(
            "a building constructor of a master without its building part",
        ))?;
        let given_z = position.z();
        // the new object's new_position: the ground's height (slot 0x60)
        let position = self.on_ground(player, master, position);
        let id = self.take_object_id(player, master);
        // what its first update as a built building starts (slot 0x7f0)
        let (default_make, default_task) = player_master(&self.masters, player, master)
            .map_or((false, None), Master::default_start);
        let ai = unit_ai
            .and_then(|t| Ok((t, self.new_object(player, master, &made, position)?)))
            .map(|(t, new)| {
                t.and_then(|t| UnitAi::constructed(t, &new, self.streams.get(Stream::Ai)))
            });
        let ai = match ai {
            Ok(ai) => ai,
            Err(e) => {
                // the constructor's draws are unknown, so the turn player's are too
                crate::trace::player_unsupported(self.ticks, self.player_turn, &e);
                let ticks = self.ticks;
                let turn = self.players.player_mut(self.player_turn)?;
                let at = turn.objects.next_place();
                let _ = turn.unsupported.get_or_insert((ticks, at, e));
                turn.stopped = true;
                None
            }
        };
        // the setups the combat setup runs first, with its master's standing sprite current
        // (slot 0x798, FUN_1412029f0): their draws, the sprite and the angle it faces
        let standing = made
            .standing_sprite
            .and_then(|s| self.graphics.get(&s).copied());
        let (mut sprite, angle) = standing.map_or((None, Angle::ZERO), |g| {
            let (s, _, angle) = self.setup_draws(g);
            (Some(s), angle)
        });
        self.building_combat_setup(player, master)?;
        let held = self.building_storage(player, master)?;
        // what its death reads of the master: its dying sprite, death spawns and decay
        let death =
            player_master(&self.masters, player, master).map_or((None, false, None, None), |m| {
                (
                    m.death_sprite,
                    m.death_spawns[0].is_some(),
                    m.death_spawns[1],
                    m.life,
                )
            });
        // the moving setup (FUN_1411580a0) of every building, a moving object though it never
        // moves (a mule cart does): the walk reads its last move time
        let mut mover = player_master(&self.masters, player, master)
            .and_then(|m| m.moving)
            .map(|stats| Mover::new_object(stats, position, self.world_time, angle, None));
        // the constructor (FUN_1411de5f0)
        let (x, y) = (position.x(), position.y());
        let (tx, ty) = (
            crate::math::conversions::cvttss2si(x),
            crate::math::conversions::cvttss2si(y),
        );
        let terrain = self.spatial.terrain.as_ref();
        // masters 50 and 1889 on snow keep their snow (+0xc61 2)
        let snow = matches!(master, FARM | PASTURE_1889)
            && -1 < tx
            && -1 < ty
            && terrain
                .map(|t| t.snow_under(Tile::new(tx, ty)))
                .transpose()?
                .unwrap_or(false);
        // +0xcd0 (FUN_141188db0): mangrove or rice terrain under its place
        let rice = terrain.is_some_and(|t| {
            t.kind(Tile::new(tx, ty))
                .is_some_and(crate::map::terrain::is_rice_terrain)
        });
        let foundation = !matches!(made.class, class::CLASS_51 | class::CLASS_54)
            && !matches!(master, ids::ID_357 | ids::ID_1188)
            && building.head_unit.is_none();
        let hp = if foundation && WholeSeconds::ZERO < made.build_time {
            1.0
        } else {
            HitPoints::of_master(made.hit_points).get()
        };
        // a foundation shows its master's construction sprite (+0x950; its standing one for
        // none, FUN_141158620) through slot 0x68 (FUN_141217000)
        if foundation && WholeSeconds::ZERO < made.build_time {
            let shown = building
                .construction_sprite
                .and_then(|s| self.graphics.get(&s).copied())
                .or(standing);
            if let Some(g) = shown {
                crate::objects::sprite::new_sprite(
                    &mut sprite,
                    mover.as_mut().map(|m| &mut m.stats),
                    g,
                    self.streams.get(Stream::Sprite),
                );
            }
        }
        let progress = if foundation {
            Seconds::ZERO
        } else {
            Seconds::of_master(made.build_time)
        };
        // FUN_1411cf770 with no master copy of its own and no annex link yet
        let desolid = WholeSeconds::ZERO < made.build_time
            && !matches!(made.class, class::CLASS_51 | class::CLASS_54)
            && progress == Seconds::ZERO;
        let owner = self.players.player_mut(player)?;
        owner.objects.push(Unit {
            id,
            footprint_waits: 0,
            life: Lifecycle {
                class: Class::Animated,
                state: ObjectState::Foundation,
                hp: HitPoints::new(hp),
                master_id: master,
                attribute_type: held.kind,
                amount: held.amount,
                dying_sprite: death.0,
                death_spawn: death.1,
                remains: death.2,
                master: death.3,
                ..Lifecycle::default()
            },
            ai,
            actions: ActionList::default(),
            unmodelled_action: None,
            production: None,
            gather_point: None,
            builders: 0,
            // the animated object's constructor (FUN_141070cf0): animation speed (+0x7b8) 0
            body: crate::objects::layers::Body::new(
                position,
                Some(crate::objects::layers::Animated::default()),
                mover,
            ),
            construction: Some(Construction {
                progress,
                desolid,
                builders: 0,
                hit_points: made.hit_points,
                build_time: made.build_time,
                record: crate::map::spatial::Obstruction {
                    kind: made.obstruction_type,
                    class: made.obstruction_class,
                },
                snow,
                sprite_override: false,
            }),
            work_rate_powerup: WorkRate::ZERO,
            attackable: true,
            untargetable: false,
            under_attack: false,
            last_human_order: WorldTime::ZERO,
            town_bell: crate::orders::town_bell::TownBell::default(),
            pasture: None,
            my_pasture: None,
            undeletable: false,
            capture_flag: 0,
            annex_of: None,
            group: crate::groups::GroupLink::default(),
            hp_powerup: HpPowerup::default(),
            regen_timer: Seconds::ZERO,
            regen_powerup: crate::objects::Regeneration::ZERO,
            regen_stinger: crate::objects::Regeneration::ZERO,
            missile: None,
            attack_count: 0,
            // the TRIBE combat setup (FUN_14121df60): the master's volley fire
            firepower: player_master(&self.masters, player, master).map_or(0.0, |m| m.volley_fire),
            next_volley: 0,
            reload_worldtime: WorldTime::ZERO,
            garrison: Vec::new(),
            garrison_time: WorldTime::ZERO,
            relics: Relics::NONE,
            damaged_lately: 0,
            livestock_summation: 0.0,
            livestock: Vec::new(),
            pasture_workers: Vec::new(),
            build_frame: crate::objects::Frame::ZERO,
            raider_countdown: 0,
            // every building holds a gate's lock, open
            gate: Some(Gate {
                first_update: true,
                ..Gate::default()
            }),
            workers: 0,
            combat: true,
            default_task,
            default_fly: None,
            default_make,
            default_artifact: false,
            stopped: false,
            sprite,
            attack_timer: Seconds::ZERO,
            abilities: crate::combat::abilities::Abilities::default(),
            reload_stinger: Seconds::ZERO,
            reload_powerup: Seconds::ZERO,
            attack_powerup: 0.0,
            range_powerup: Distance::ZERO,
            armor_powerups: ArmorPair::ZERO,
            armor_removed: ArmorPair::ZERO,
            charge: Charge::ZERO,
            charge_hit: false,
            idle_variant: 0,
            roo_called: false,
            rice,
            store: Vec::new(),
            own_master: None,
            dead_master: None,
            stack_made: None,
            attacking_units: Vec::new(),
        });
        self.settle_building(player, id, &made, (position, desolid), foundation)?;
        for (&annex, &off) in building.annexes.iter().zip(&building.annex_offsets) {
            let Some(annex) = annex else {
                continue;
            };
            // a master the player has not got makes nothing (its slot 0x40 is not called)
            let Some(am) = player_master(&self.masters, player, annex).cloned() else {
                continue;
            };
            // the .dat's annexes are buildings (type 80): another type's master has no building
            // part, which the constructor below refuses
            let ai = Ok(am.unit_ai_type(annex));
            let at = Position::new(position.x() + off.x(), position.y() + off.y(), given_z);
            let (aid, at) = self.make_building(player, annex, ai, at)?;
            self.link_annex(player, id, aid, &made, &am)?;
            self.clear_footprint(player, annex, at.ground(), false)?;
        }
        self.watch_started(player, id)?;
        Ok((id, position))
    }

    /// The static init's add of the new building `id` to its player (slot 0x2d0: it waits in the
    /// tracker's pending lists), its place, a foundation's terrain and its footprint.
    fn settle_building(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        made: &Master,
        (position, desolid): (Position, bool),
        foundation: bool,
    ) -> Result<(), GenieError> {
        let master = made.id;
        self.track_add(player, id, master)?;
        self.building_place(player, id, made, position, desolid)?;
        if foundation && WholeSeconds::ZERO < made.build_time {
            self.construction_terrain(player, master, position)?;
        }
        self.clear_footprint(player, master, position.ground(), false)
    }

    /// The building constructor's link of a new annex `aid` (master `am`) to its building `id`
    /// (master `made`): the building's slot and the annex's `+0xbb8`; its owner's watched list
    /// lets the annex go (`FUN_141091310`) unless the building is a gate (39); a tall annex
    /// master makes it desolid (`FUN_1411cf770`, which an annex's link lets through: its record
    /// and mesh shape go, its own master copy has no height, the watched list lets it go; its
    /// tiles keep their links, as an annex's do).
    pub(crate) fn link_annex(
        &mut self,
        player: PlayerId,
        id: ObjectId,
        aid: ObjectId,
        made: &Master,
        am: &Master,
    ) -> Result<(), GenieError> {
        let not_held = crate::construction::build::NOT_HELD;
        let (_, a) = self.object_mut(aid).ok_or_else(|| not_held.clone())?;
        a.annex_of = Some(id);
        self.spatial.set_annex(aid, Some(id));
        if made.class != class::GATE {
            self.unwatch(player, aid)?;
        }
        if 0.0 < am.height {
            let (_, a) = self.object_mut(aid).ok_or_else(|| not_held.clone())?;
            let c = a
                .construction
                .as_mut()
                .ok_or(crate::construction::build::NO_CONSTRUCTION)?;
            if !c.desolid {
                c.desolid = true;
                self.pathing.removed(aid);
                self.spatial.drop_record(aid);
                self.spatial.set_tall(aid, false);
                self.pathing.tiles_changed(self.spatial.take_changes());
                self.unwatch(player, aid)?;
            }
        }
        Ok(())
    }
}
