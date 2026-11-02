//! The world as `bestUnitToAttack` and the attack tests read it ([`WorldPicker`]).

use crate::GenieError;
use crate::combat::victims::victim_of;
use crate::map::Distance;
use crate::map::spatial::Spatial;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::RGE_COMBAT;
use crate::objects::masters::player_master;
use crate::objects::unit::Unit;
use crate::ownership::CaptureKind;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::NearestAttacker;
use crate::unit_ai::world_view::Foreign;
use crate::world::WorldTime;
use crate::world::others::OthersRef;

/// Slot 0x228 (`FUN_141084070`): the sum of master `m`'s attacks above 0 (`COMISS`), from +0.
pub(super) fn attacks(m: &crate::objects::masters::Master) -> f32 {
    m.weapons
        .iter()
        .filter(|w| crate::objects::masters::DamageAmount::ZERO < w.amount)
        .fold(0.0, |sum, w| sum + w.amount.as_f32())
}

/// The hit points (`DAT_14330ebc8`) and the range (`DAT_142e7f384`) below which a villager
/// fights its attacker back ([`WorldPicker::harmless`]).
pub(super) const HARMLESS_HP: f32 = crate::math::image::unit_ai::HARMLESS_HP;
pub(super) const HARMLESS_RANGE: Distance =
    Distance::new(crate::math::image::unit_ai::HARMLESS_RANGE);

/// An object the unit AI found or names that the slice does not hold (another player's it does
/// not keep).
pub(crate) const NOT_HELD: GenieError =
    GenieError::unsupported("a found object the slice does not hold");

/// What `bestUnitToAttack` reads of the world for the unit whose AI runs
/// ([`crate::unit_ai::AttackPicker`]): the objects it found, by what the world holds of them, and
/// the unit's quick path to them.
pub(crate) struct WorldPicker<'a> {
    /// What the tests read of the unit.
    pub(crate) attacker: &'a crate::unit_ai::Attacker,
    /// What they read of its unit AI; None outside `bestUnitToAttack`.
    pub(crate) seeker: Option<crate::unit_ai::Seeker>,
    /// The unit.
    pub(crate) me: ObjectId,
    /// Its owner.
    pub(crate) player: PlayerId,
    /// The owner's other objects.
    pub(crate) others: OthersRef<'a>,
    /// The world's objects, whose gaits the quick path reads.
    pub(crate) by_id: crate::world::index::ObjectsRef<'a>,
    /// The other players' lists.
    pub(crate) foreign: Foreign<'a>,
    /// Every player's masters.
    pub(crate) every: crate::objects::masters::MasterTables<'a>,
    /// The world's parts by id ([`crate::world::parts::Parts`]): the units' AIs (the unit's own
    /// is out of the table) and the buildings' layers.
    pub(crate) parts: &'a crate::world::parts::Parts,
    /// The unit's master's terrain restriction (`+0x606`).
    pub(crate) restriction: Option<crate::map::RestrictionId>,
    /// The unit itself.
    pub(crate) unit: &'a Unit,
    /// Its owner's attributes, diplomacy and AI settings.
    pub(crate) attributes: &'a [Amount],
    /// Its owner's diplomacy (`player+0x260`).
    pub(crate) relations: &'a crate::players::Relations,
    /// Its owner's AI settings.
    pub(crate) ai_settings: crate::world::player::AiSettings,
    /// The world's time.
    pub(crate) now: WorldTime,
    /// The game's setting `+0x680`.
    pub(crate) mode_680: crate::world::Mode680,
}

/// An object the picker reads: its owner, the object, its owner's attributes and its master.
type Held<'a> = (
    PlayerId,
    &'a Unit,
    &'a [Amount],
    &'a crate::objects::masters::Master,
);

impl WorldPicker<'_> {
    /// The object `id` (another player's, the owner's others or the unit itself) with its owner,
    /// its owner's attributes and its master (its own, else its owner's).
    pub(super) fn held(&self, id: ObjectId) -> Result<Option<Held<'_>>, GenieError> {
        let (q, u, attributes) = if id == self.me {
            (self.player, self.unit, self.attributes)
        } else if let Some(u) = self.others.own(id) {
            (self.player, u, self.attributes)
        } else {
            let Some((q, u)) = self.others.foreign(id) else {
                return Ok(None);
            };
            let Some(foreign) = self.foreign.player(q) else {
                return Ok(None);
            };
            (q, u, &foreign.attributes[..])
        };
        let master = u.master(self.every, q)?;
        Ok(Some((q, u, attributes, master)))
    }

    /// The priest's slot 0x1b8 of object `who` (a unit AI of another type answers false): may it
    /// convert object `whom`, `any` not asking its faith ([`crate::unit_ai::can_convert`]).
    pub(super) fn converts(
        &self,
        who: ObjectId,
        whom: ObjectId,
        any: bool,
    ) -> Result<bool, GenieError> {
        let (_, p, attributes, pm) = self.held(who)?.ok_or(GenieError::unsupported(
            "a priest's conversion test of an object the slice does not hold",
        ))?;
        if self
            .parts
            .unit_ai
            .get(p.id)
            .is_none_or(|a| a.ai_type != crate::unit_ai::PRIEST_AI)
        {
            return Ok(false);
        }
        let converter = crate::unit_ai::Converter {
            class: pm.class,
            master: p.life.master_id,
            faith: p.life.amount,
            attributes,
            tasks: &pm.tasks,
        };
        let target = self
            .held(whom)?
            .map(|(q, t, _, tm)| crate::unit_ai::Convertee {
                owner: q,
                class: tm.class,
                master: t.life.master_id,
                hero: tm.hero_flag,
            });
        Ok(crate::unit_ai::can_convert(
            &converter,
            target.as_ref(),
            any,
        ))
    }

    /// What object `from` deals object `to` per hit (`to`'s slot 0x220 with `from`,
    /// `FUN_141083f20`): `from`'s elevation factor against it (slot 0x1f0: a combat object's
    /// [`crate::combat::damage::elevation_factor`], a building's [`crate::combat::damage::building_elevation_factor`]
    /// over it), then `to`'s armor (slot 0xf0, [`crate::combat::damage::armor_damage`]) against `from`'s
    /// attacks.
    pub(super) fn damage(
        &self,
        spatial: &Spatial,
        from: ObjectId,
        to: ObjectId,
    ) -> Result<HitPoints, GenieError> {
        let (fq, f, fa, fm) = self.held(from)?.ok_or(NOT_HELD)?;
        let (tq, t, ta, tm) = self.held(to)?.ok_or(NOT_HELD)?;
        let terrain = spatial.terrain.as_ref();
        let target = victim_of(
            (
                t,
                self.parts.unit_ai.contains(t.id),
                self.parts.building_of(t.id),
                self.parts.combat_of(t.id),
                self.parts.actions.of(t.id),
            ),
            tm,
            ta,
            tq,
            terrain,
            self.mode_680,
        );
        let unit = victim_of(
            (
                f,
                self.parts.unit_ai.contains(f.id),
                self.parts.building_of(f.id),
                self.parts.combat_of(f.id),
                self.parts.actions.of(f.id),
            ),
            fm,
            fa,
            fq,
            terrain,
            self.mode_680,
        );
        let mut factor = crate::combat::damage::elevation_factor(
            f.position().z(),
            unit.attack_elevation,
            &target,
        );
        if fm.kind == BUILDING {
            factor = crate::combat::damage::building_elevation_factor(
                factor,
                self.parts.building_of(f.id).relics,
                tm.kind,
                self.parts.building_of(t.id).relics,
            );
        }
        let bonus = crate::combat::damage::bonus_damage(&unit, &target);
        crate::combat::damage::armor_damage(&target, &unit.weapons, factor, &unit.source, bonus)
    }

    /// What a computer player's unit weighs of object `other` on a damage notify
    /// ([`crate::unit_ai::Duel`]).
    pub(crate) fn duel(
        &self,
        spatial: &Spatial,
        other: ObjectId,
    ) -> Result<crate::unit_ai::Duel, GenieError> {
        let (_, o, _, om) = self.held(other)?.ok_or(NOT_HELD)?;
        Ok(crate::unit_ai::Duel {
            converts: self.converts(other, self.me, true)?,
            attacks: attacks(om),
            harms: self.damage(spatial, other, self.me)?,
            harmed: self.damage(spatial, self.me, other)?,
            hp: o.life.hp,
        })
    }

    /// The villager's test of its attacker `other` (`FUN_140c22bf0` case 500): one of a master
    /// with object flag 0x10 (`+0x654`) that is its own copy (`+0x158` equal to `+0x58`), which
    /// shows an attack below 5 (slot 0x878, [`crate::combat::damage::shown_attack`]; below 4, or with
    /// its master's hit points below [`HARMLESS_HP`]), a melee armor below 1 (slot 0x868) and a
    /// range of at most [`HARMLESS_RANGE`] (slot 0x880, `COMISS` with `JNC`: not a NaN), is
    /// fought back as Gaia's attackers are. Its attack's target (an Attack's, `FUN_14107cc30`)
    /// counts when the world holds it.
    pub(crate) fn harmless(&self, spatial: &Spatial, other: ObjectId) -> Result<bool, GenieError> {
        let (q, o, attributes, om) = self.held(other)?.ok_or(NOT_HELD)?;
        if !om.object_flags.contains(ObjectFlags::SCOUT) || om.copy_id != Some(om.id) {
            return Ok(false);
        }
        let terrain = spatial.terrain.as_ref();
        let v = victim_of(
            (
                o,
                self.parts.unit_ai.contains(o.id),
                self.parts.building_of(o.id),
                self.parts.combat_of(o.id),
                self.parts.actions.of(o.id),
            ),
            om,
            attributes,
            q,
            terrain,
            self.mode_680,
        );
        let attacked = (self.parts.actions_of(o.id).top_action_kind()
            == Some(crate::combat::attack::ATTACK))
        .then(|| self.parts.actions_of(o.id).top_target())
        .flatten();
        let target = match attacked {
            Some(t) => self.held(t)?.map(|(tq, t, ta, tm)| {
                victim_of(
                    (
                        t,
                        self.parts.unit_ai.contains(t.id),
                        self.parts.building_of(t.id),
                        self.parts.combat_of(t.id),
                        self.parts.actions.of(t.id),
                    ),
                    tm,
                    ta,
                    tq,
                    terrain,
                    self.mode_680,
                )
            }),
            None => None,
        };
        let shown = crate::combat::damage::Shown {
            target: target.as_ref(),
            projectile: om
                .charge_projectile
                .and_then(|id| player_master(self.every.players, q, id))
                .map(|m| &m.weapons[..]),
            charge_event: om.charge_event,
            range_powerup: self.parts.combat_of(o.id).range_powerup,
        };
        let attack = crate::combat::damage::shown_attack(&v, &shown);
        let amount = crate::objects::masters::DamageAmount::new;
        if amount(4) < attack
            || (amount(3) < attack && HARMLESS_HP <= HitPoints::of_master(om.hit_points).get())
        {
            return Ok(false);
        }
        if amount(0) < crate::combat::damage::shown_melee_armor(&v)? {
            return Ok(false);
        }
        Ok(crate::combat::damage::shown_range(&v, &shown) <= HARMLESS_RANGE)
    }

    /// Slot 0x78 of object `target`'s unit AI (`FUN_140c48610`): of its attackers in their order,
    /// those the world holds, the one with the least footprint gap to it (`FUN_14118f680`,
    /// [`crate::movement::footprint_gap`]; the first of a tie, `COMISS`: a NaN gap takes none
    /// over; [`NearestAttacker`]).
    pub(crate) fn nearest_attacker_of(
        &self,
        target: ObjectId,
    ) -> Result<NearestAttacker, GenieError> {
        let Some((_, t, _, tm)) = self.held(target)? else {
            return Ok(NearestAttacker::NoUnitAi);
        };
        let Some(ai) = self.parts.unit_ai.get(t.id) else {
            return Ok(NearestAttacker::NoUnitAi);
        };
        let mut best: Option<(f32, ObjectId)> = None;
        // a missile's sender gone (−1) is held by no one
        for id in ai.attackers.iter().flatten() {
            let Some((_, a, _, am)) = self.held(id)? else {
                continue;
            };
            let gap =
                crate::movement::footprint_gap(t.position(), tm.radius, a.position(), am.radius);
            if best.is_none_or(|(least, _)| gap < Distance::new(least)) {
                best = Some((gap.get(), id));
            }
        }
        Ok(best.map_or(NearestAttacker::NoAttacker, |(_, id)| {
            NearestAttacker::Attacker(id)
        }))
    }

    /// The object `id` as the attack tests read it, and whether it moves (slot 0x208 above 0).
    pub(crate) fn target(
        &self,
        spatial: &Spatial,
        id: ObjectId,
    ) -> Result<(crate::unit_ai::Target, bool), GenieError> {
        let (owner, t) = self.others.find(id).ok_or(NOT_HELD)?;
        let m = t.master(self.every, owner)?;
        let placed = spatial
            .get_known(id)?
            .ok_or(GenieError::unsupported("a found object without a place"))?;
        let capture = spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(id))
            .map_or(CaptureKind::NONE, |s| s.capture);
        let target = crate::unit_ai::Target {
            owner,
            class: m.class,
            kind: m.kind,
            master: t.life.master_id,
            combat_level: m.combat_level,
            capture,
            has_ai: self.parts.unit_ai.contains(t.id),
            inside: t.life.inside.is_some(),
            targeting: !t.untargetable,
            attackable: t.attackable,
            desolid: self
                .parts
                .building_of(t.id)
                .construction
                .as_ref()
                .is_some_and(|c| c.desolid),
            position: placed.position,
            radius: placed.radius,
        };
        let moving = Speed::ZERO
            < self
                .parts
                .mover_of(t.id)
                .map_or(Speed::ZERO, |m| m.stats.speed);
        Ok((target, moving))
    }
}

impl crate::unit_ai::AttackPicker for WorldPicker<'_> {
    /// The unit, its master and its AI as `bestUnitToAttack` reads them: the AI's targets, order
    /// and action ([`crate::unit_ai::Seeker`]), its may-move test, the sight
    /// ([`crate::unit_ai::attack_sight`]), its ranges and speed, its owner's walls flag and
    /// targeting, the order history's weights and the retarget entries' targets.
    fn searcher(
        &mut self,
        memory: crate::unit_ai::Memory<'_>,
    ) -> Result<crate::unit_ai::best_target::Searcher, GenieError> {
        let seeker = self.seeker.ok_or(GenieError::unsupported(
            "an attack test outside bestUnitToAttack",
        ))?;
        let a = self.attacker;
        Ok(crate::unit_ai::best_target::Searcher {
            id: self.me,
            target: seeker.target,
            target_classes: [seeker.target_class, seeker.last_target_class],
            order: seeker.order,
            action: seeker.action,
            master: a.master,
            class: a.class,
            kind: a.kind,
            attack_priority: a.attack_priority,
            computer: a.computer,
            walls_allowed: self.ai_settings.may_attack_walls,
            can_move: seeker.can_move(),
            sight: crate::unit_ai::attack_sight(a)?,
            range: a.range_against()?,
            weapon_range: a.range,
            min_range: a.min_range,
            speed: a.speed,
            scenario: a.scenario,
            targeting: (
                self.ai_settings.local_targeting_mode,
                self.ai_settings.targeting_factor_scalar,
            ),
            weights: crate::unit_ai::best_target::order_weights(memory.history, self.now),
            retries: memory.retries.iter().map(|&(id, _)| id).collect(),
            now: self.now,
        })
    }

    fn thing(
        &mut self,
        spatial: &Spatial,
        id: ObjectId,
    ) -> Result<Option<crate::unit_ai::best_target::Thing>, GenieError> {
        let Some((owner, t, _, m)) = self.held(id)? else {
            return Ok(None);
        };
        let placed = spatial
            .get_known(id)?
            .ok_or(GenieError::unsupported("a found object without a place"))?;
        let capture = spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(id))
            .map_or(CaptureKind::NONE, |s| s.capture);
        let a = self.attacker;
        Ok(Some(crate::unit_ai::best_target::Thing {
            owner,
            allied: crate::unit_ai::task_finder::allied(self.relations, self.player, owner, true),
            state: t.life.state,
            has_ai: self.parts.unit_ai.contains(t.id),
            inside: t.life.inside.is_some(),
            master: t.life.master_id,
            class: m.class,
            kind: m.kind,
            capture: capture.any(),
            targetable: !t.untargetable,
            attackable: t.attackable,
            held: t.life.amount,
            distance: crate::movement::footprint_gap(
                a.position,
                a.radius,
                placed.position,
                placed.radius,
            ),
            speed: self
                .parts
                .mover_of(t.id)
                .map_or(Speed::ZERO, |mv| mv.stats.speed),
        }))
    }

    fn can_attack(&mut self, spatial: &Spatial, id: ObjectId) -> Result<bool, GenieError> {
        let seeker = self.seeker.ok_or(GenieError::unsupported(
            "an attack test outside bestUnitToAttack",
        ))?;
        let (target, _) = self.target(spatial, id)?;
        crate::unit_ai::can_attack(self.attacker, &seeker, &target)
    }

    fn attacks_neutral(&mut self, class: ClassId) -> Result<bool, GenieError> {
        Ok(crate::unit_ai::attacks_neutral(self.attacker, class))
    }

    fn spares_friends(&mut self, spatial: &Spatial, id: ObjectId) -> Result<bool, GenieError> {
        let (target, _) = self.target(spatial, id)?;
        crate::unit_ai::spares_friends(self.attacker, &target)
    }

    /// `FUN_140c41690`: a human player's unit passes nothing over (the computer player's test
    /// never runs: its search fails at the sight).
    fn shuns(&mut self, _: ObjectId) -> Result<bool, GenieError> {
        Ok(false)
    }

    /// What the statistics read of object `id`: its class and speed, whether it is armed (its
    /// attacks' sum, slot 0x228, above 0, or its priest's conversion test on the unit), the
    /// unit's range against it, the unit's damage per hit to it (slot 0x220: the unit's
    /// elevation factor, slot 0x1f0, and its armor, slot 0xf0, [`crate::combat::damage::armor_damage`]),
    /// its hit points, the unit's reload time (slot 0x218) and the unit's own conversion test on
    /// it.
    fn candidate(
        &mut self,
        spatial: &Spatial,
        id: ObjectId,
        distance: Distance,
        category: Option<crate::unit_ai::best_target::AttackCategory>,
    ) -> Result<crate::unit_ai::best_target::Candidate, GenieError> {
        let (_, t, _, m) = self.held(id)?.ok_or(NOT_HELD)?;
        let (_, me, _, mm) = self.held(self.me)?.ok_or(GenieError::unsupported(
            "a searching unit of a master the slice does not have",
        ))?;
        let damage = self.damage(spatial, self.me, id)?;
        let armed = 0.0 < attacks(m) || self.converts(id, self.me, true)?;
        Ok(crate::unit_ai::best_target::Candidate {
            id,
            distance,
            category,
            class: m.class,
            speed: self
                .parts
                .mover_of(t.id)
                .map_or(Speed::ZERO, |mv| mv.stats.speed),
            armed,
            reach: self.attacker.range_against()?,
            damage,
            hp: t.life.hp,
            reload: self.parts.combat_of(me.id).reload_stinger
                + (mm.reload - self.parts.combat_of(me.id).reload_powerup),
            workable: self.converts(self.me, id, false)?,
            current: self.seeker.is_some_and(|s| s.target == Some(id)),
        })
    }

    /// Slot 0x870's armor (`FUN_141213920` -> `FUN_1410857f0`): the highest melee armor (class
    /// 3; else the base armor or the highest armor of any class) plus the pierce armor powerup
    /// (`+0xa4c`, slot 0x710; slot 0x778's stinger `+0xb50` is 0), at least 100.
    fn armored(&mut self, id: ObjectId) -> Result<bool, GenieError> {
        let (_, t, _, m) = self.held(id)?.ok_or(NOT_HELD)?;
        let class3 = m
            .armors
            .iter()
            .filter(|a| a.class == crate::objects::masters::DamageClass::PIERCE)
            .map(|a| a.amount)
            .max();
        let armor = class3.unwrap_or_else(|| {
            m.armors.iter().fold(
                m.base_armor,
                |best, a| if best < a.amount { a.amount } else { best },
            )
        });
        Ok(crate::objects::masters::DamageAmount::new(100)
            <= armor.plus(self.parts.combat_of(t.id).armor_powerups.pierce))
    }

    /// Object `id`'s weapon range (slot 0x230: a combat object's master's, else 0).
    fn weapon_range(&mut self, id: ObjectId) -> Result<Distance, GenieError> {
        let (_, _, _, m) = self.held(id)?.ok_or(NOT_HELD)?;
        Ok(if RGE_COMBAT <= m.kind {
            m.weapon_range
        } else {
            Distance::ZERO
        })
    }

    fn engages(&mut self, id: ObjectId) -> Result<bool, GenieError> {
        let (_, _, _, m) = self.held(id)?.ok_or(NOT_HELD)?;
        Ok(0.0 < attacks(m) || self.converts(id, self.me, true)?)
    }

    /// The quick path (slot 0x3e0) within `range`, with its master's terrain mask and radius on
    /// x.
    fn quick_path(
        &mut self,
        spatial: &mut Spatial,
        id: ObjectId,
        range: Distance,
    ) -> Result<bool, GenieError> {
        let terrain = spatial
            .terrain
            .as_ref()
            .ok_or(crate::pathing::hull_path::NO_TERRAIN)?;
        let restriction = self
            .restriction
            .ok_or(crate::pathing::hull_path::NO_RESTRICTION)?;
        let mask = terrain.restriction_mask(restriction);
        crate::pathing::hull_path::quick_reach(
            (spatial, self.by_id),
            self.me,
            Some(id),
            mask,
            Distance::new(self.attacker.radius.x()),
            range,
        )
    }
}

/// A unit's slot 0x468 (`FUN_141157c50`, `getPatrolInformation`): its top level's path
/// (`FUN_140f960f0`) copied into a new path ([`crate::pathing::tile_path::Path::append`]: its points, its
/// length or one measured) with its flags, and that level's waypoint; None without path data or
/// a path at the top.
pub(crate) fn patrol_information(
    mover: Option<&Mover>,
) -> Option<(crate::unit_ai::PatrolPath, crate::movement::WaypointIndex)> {
    let top = mover?.paths.last().filter(|l| l.has_path)?;
    let mut path = crate::pathing::tile_path::Path::default();
    path.append(&crate::pathing::tile_path::Path {
        points: top.points.clone(),
        length: top.length,
    });
    let saved = crate::unit_ai::PatrolPath {
        path,
        flags: top.path_flags,
    };
    Some((saved, top.waypoint))
}
