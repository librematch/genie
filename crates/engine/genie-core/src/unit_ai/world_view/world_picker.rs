//! The world as `bestUnitToAttack` and the attack tests read it ([`WorldPicker`]).

use crate::combat::attack::ATTACK;
use crate::combat::damage::{
    Shown, armor_damage, bonus_damage, building_elevation_factor, elevation_factor, shown_attack,
    shown_melee_armor, shown_range,
};
use crate::combat::victims::victim_of;
use crate::map::Distance;
use crate::map::RestrictionId;
use crate::map::spatial::Spatial;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::{WaypointIndex, footprint_gap};
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::masters::MasterKind;
use crate::objects::masters::ObjectFlags;
use crate::objects::masters::player_master;
use crate::objects::masters::{DamageAmount, Master, MasterTables};
use crate::objects::unit::Unit;
use crate::objects::unit::UnitFlags;
use crate::ownership::CaptureKind;
use crate::pathing::tile_path::Path;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::Relations;
use crate::unit_ai::AttackTargetFlags;
use crate::unit_ai::NearestAttacker;
use crate::unit_ai::TargetRules;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::world_view::Foreign;
use crate::unit_ai::{
    Attacker, Convertee, Converter, Duel, PatrolPath, Seeker, Target, can_convert,
};
use crate::world::Mode680;
use crate::world::WorldTime;
use crate::world::index::ObjectsRef;
use crate::world::layer::Roles;
use crate::world::layer::generated::DamageTuning;
use crate::world::layer::generated::UnitAiTuning;
use crate::world::others::OthersRef;
use crate::world::parts::Parts;
use crate::world::player::AiSettings;
use crate::{GenieError, GenieResult};

/// Slot 0x228 (`FUN_141084070`): the sum of master `m`'s attacks above 0 (`COMISS`), from +0.
pub(super) fn attacks(m: &Master) -> f32 {
    m.combat_part()
        .weapons
        .iter()
        .filter(|w| DamageAmount::ZERO < w.amount)
        .fold(0.0, |sum, w| sum + w.amount.as_f32())
}

/// An object the unit AI found or names that the slice does not hold (another player's it does
/// not keep).
pub(crate) const NOT_HELD: GenieError =
    GenieError::unsupported("a found object the slice does not hold");

/// What `bestUnitToAttack` reads of the world for the unit whose AI runs: the objects it found,
/// by what the world holds of them, and the unit's quick path to them.
pub(crate) struct WorldPicker<'a> {
    /// What the tests read of the unit.
    pub(crate) attacker: &'a Attacker,
    /// What they read of its unit AI; None outside `bestUnitToAttack`.
    pub(crate) seeker: Option<Seeker>,
    /// The unit.
    pub(crate) me: ObjectId,
    /// Its owner.
    pub(crate) player: PlayerId,
    /// The owner's other objects.
    pub(crate) others: OthersRef<'a>,
    /// The world's objects, whose gaits the quick path reads.
    pub(crate) by_id: ObjectsRef<'a>,
    /// The other players' lists.
    pub(crate) foreign: Foreign<'a>,
    /// Every player's masters.
    pub(crate) every: MasterTables<'a>,
    /// The world's parts by id ([`crate::world::parts::Parts`]): the units' AIs (the unit's own
    /// is out of the table) and the buildings' layers.
    pub(crate) parts: &'a Parts,
    /// The unit's master's terrain restriction (`+0x606`).
    pub(crate) restriction: Option<RestrictionId>,
    /// The unit itself.
    pub(crate) unit: &'a Unit,
    /// Its action layer, out of the table with it.
    pub(crate) actions: &'a Action,
    /// Its owner's attributes, diplomacy and AI settings.
    pub(crate) attributes: &'a [Amount],
    /// Its owner's diplomacy (`player+0x260`).
    pub(crate) relations: &'a Relations,
    /// Its owner's AI settings.
    pub(crate) ai_settings: AiSettings,
    /// The world's time.
    pub(crate) now: WorldTime,
    /// The game's setting `+0x680`.
    pub(crate) mode_680: Mode680,
}

/// An object the picker reads: its owner, the object, its owner's attributes and its master.
type Held<'a> = (PlayerId, &'a Unit, &'a [Amount], &'a Master);

impl WorldPicker<'_> {
    /// The action layer of object `id`: the unit's own (out of the table with it, as its row is
    /// out of its list) or the table's.
    fn actions_of(&self, id: ObjectId) -> &Action {
        if id == self.me {
            self.actions
        } else {
            self.parts.actions.of(id)
        }
    }

    /// The object `id` (another player's, the owner's others or the unit itself) with its owner,
    /// its owner's attributes and its master (its own, else its owner's).
    pub(super) fn held(&self, id: ObjectId) -> GenieResult<Option<Held<'_>>> {
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
    /// convert object `whom`, [`TargetRules::Any`] not asking its faith
    /// ([`crate::unit_ai::can_convert`]).
    pub(super) fn converts(
        &self,
        who: ObjectId,
        whom: ObjectId,
        rules: TargetRules,
        roles: &Roles,
        unit_ai_tuning: &UnitAiTuning,
    ) -> GenieResult<bool> {
        let (_, p, attributes, pm) = self.held(who)?.ok_or(GenieError::unsupported(
            "a priest's conversion test of an object the slice does not hold",
        ))?;
        if self
            .parts
            .unit_ai
            .get(p.id)
            .is_none_or(|a| a.ai_kind != UnitAiKind::Priest)
        {
            return Ok(false);
        }
        let converter = Converter {
            class: pm.class,
            master: p.life.master_id,
            faith: p.life.amount,
            attributes,
            tasks: &pm.action_part().tasks,
        };
        let target = self.held(whom)?.map(|(q, t, _, tm)| Convertee {
            owner: q,
            class: tm.class,
            master: t.life.master_id,
            hero: tm.creatable_part().hero_flag,
        });
        Ok(can_convert(
            &converter,
            target.as_ref(),
            rules,
            roles,
            unit_ai_tuning,
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
        roles: &Roles,
        damage_tuning: &DamageTuning,
    ) -> GenieResult<HitPoints> {
        let (fq, f, fa, fm) = self.held(from)?.ok_or(NOT_HELD)?;
        let (tq, t, ta, tm) = self.held(to)?.ok_or(NOT_HELD)?;
        let terrain = spatial.terrain.as_ref();
        let target = victim_of(
            (
                t,
                self.parts.unit_ai.contains(t.id),
                self.parts.building_of(t.id),
                self.parts.combat_of(t.id),
                self.actions_of(t.id),
                self.parts.unread.get(t.id),
            ),
            tm,
            ta,
            tq,
            terrain,
            self.mode_680,
            roles,
        );
        let unit = victim_of(
            (
                f,
                self.parts.unit_ai.contains(f.id),
                self.parts.building_of(f.id),
                self.parts.combat_of(f.id),
                self.actions_of(f.id),
                self.parts.unread.get(f.id),
            ),
            fm,
            fa,
            fq,
            terrain,
            self.mode_680,
            roles,
        );
        let mut factor = elevation_factor(
            f.position().z(),
            unit.attack_elevation,
            &target,
            damage_tuning,
        );
        if fm.kind == MasterKind::BUILDING {
            factor = building_elevation_factor(
                factor,
                self.parts.building_of(f.id).relics,
                tm.kind,
                self.parts.building_of(t.id).relics,
                damage_tuning,
            );
        }
        let bonus = bonus_damage(&unit, &target, roles);
        armor_damage(&target, &unit.weapons, factor, &unit.source, bonus, roles)
    }

    /// What a computer player's unit weighs of object `other` on a damage notify
    /// ([`crate::unit_ai::Duel`]).
    pub(crate) fn duel(
        &self,
        spatial: &Spatial,
        other: ObjectId,
        roles: &Roles,
        damage_tuning: &DamageTuning,
        unit_ai_tuning: &UnitAiTuning,
    ) -> GenieResult<Duel> {
        let (_, o, _, om) = self.held(other)?.ok_or(NOT_HELD)?;
        Ok(Duel {
            converts: self.converts(other, self.me, TargetRules::Any, roles, unit_ai_tuning)?,
            attacks: attacks(om),
            harms: self.damage(spatial, other, self.me, roles, damage_tuning)?,
            harmed: self.damage(spatial, self.me, other, roles, damage_tuning)?,
            hp: o.life.hp,
        })
    }

    /// The villager's test of its attacker `other` (`FUN_140c22bf0` case 500): one of a master
    /// with object flag 0x10 (`+0x654`) that is its own copy (`+0x158` equal to `+0x58`), which
    /// shows an attack below 5 (slot 0x878, [`crate::combat::damage::shown_attack`]; below 4, or with
    /// its master's hit points below `UnitAiTuning::harmless_hp`), a melee armor below 1 (slot 0x868) and a
    /// range of at most `UnitAiTuning::harmless_range` (slot 0x880, `COMISS` with `JNC`: not a NaN), is
    /// fought back as Gaia's attackers are. Its attack's target (an Attack's, `FUN_14107cc30`)
    /// counts when the world holds it.
    pub(crate) fn harmless(
        &self,
        spatial: &Spatial,
        other: ObjectId,
        roles: &Roles,
        unit_ai_tuning: &UnitAiTuning,
    ) -> GenieResult<bool> {
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
                self.actions_of(o.id),
                self.parts.unread.get(o.id),
            ),
            om,
            attributes,
            q,
            terrain,
            self.mode_680,
            roles,
        );
        let attacked = (self.actions_of(o.id).top_action_kind() == Some(ATTACK))
            .then(|| self.actions_of(o.id).top_target())
            .flatten();
        let target = match attacked {
            Some(t) => self.held(t)?.map(|(tq, t, ta, tm)| {
                victim_of(
                    (
                        t,
                        self.parts.unit_ai.contains(t.id),
                        self.parts.building_of(t.id),
                        self.parts.combat_of(t.id),
                        self.actions_of(t.id),
                        self.parts.unread.get(t.id),
                    ),
                    tm,
                    ta,
                    tq,
                    terrain,
                    self.mode_680,
                    roles,
                )
            }),
            None => None,
        };
        let shown = Shown {
            target: target.as_ref(),
            projectile: om
                .creatable_part()
                .charge_projectile
                .and_then(|id| player_master(self.every.players, q, id))
                .map(|m| &m.combat_part().weapons[..]),
            charge_event: om.creatable_part().charge_event,
            range_powerup: self.parts.combat_of(o.id).range_powerup,
        };
        let attack = shown_attack(&v, &shown, roles);
        let amount = DamageAmount::new;
        if amount(4) < attack
            || (amount(3) < attack
                && unit_ai_tuning.harmless_hp <= HitPoints::of_master(om.hit_points).get())
        {
            return Ok(false);
        }
        if amount(0) < shown_melee_armor(&v, roles)? {
            return Ok(false);
        }
        Ok(shown_range(&v, &shown, roles) <= Distance::new(unit_ai_tuning.harmless_range))
    }

    /// Slot 0x78 of object `target`'s unit AI (`FUN_140c48610`): of its attackers in their order,
    /// those the world holds, the one with the least footprint gap to it (`FUN_14118f680`,
    /// [`crate::movement::footprint_gap`]; the first of a tie, `COMISS`: a NaN gap takes none
    /// over; [`NearestAttacker`]).
    pub(crate) fn nearest_attacker_of(&self, target: ObjectId) -> GenieResult<NearestAttacker> {
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
            let gap = footprint_gap(t.position(), tm.radius, a.position(), am.radius);
            if best.is_none_or(|(least, _)| gap < Distance::new(least)) {
                best = Some((gap.get(), id));
            }
        }
        Ok(best.map_or(NearestAttacker::NoAttacker, |(_, id)| {
            NearestAttacker::Attacker(id)
        }))
    }

    /// The object `id` as the attack tests read it, and whether it moves (slot 0x208 above 0).
    pub(crate) fn target(&self, spatial: &Spatial, id: ObjectId) -> GenieResult<(Target, bool)> {
        let (owner, t) = self.others.find(id).ok_or(NOT_HELD)?;
        let m = t.master(self.every, owner)?;
        let placed = spatial
            .get_known(id)?
            .ok_or(GenieError::unsupported("a found object without a place"))?;
        let capture = spatial
            .sight
            .as_ref()
            .and_then(|v| v.seer(id))
            .map_or(CaptureKind::None, |s| s.capture);
        let target = Target {
            owner,
            class: m.class,
            kind: m.kind,
            master: t.life.master_id,
            combat_level: m.combat_level,
            capture,
            flags: {
                let mut flags = AttackTargetFlags::empty();
                flags.set(AttackTargetFlags::HAS_AI, self.parts.unit_ai.contains(t.id));
                flags.set(AttackTargetFlags::INSIDE, t.life.inside.is_some());
                flags.set(
                    AttackTargetFlags::TARGETING,
                    !t.flags.contains(UnitFlags::UNTARGETABLE),
                );
                flags.set(
                    AttackTargetFlags::ATTACKABLE,
                    t.flags.contains(UnitFlags::ATTACKABLE),
                );
                flags.set(
                    AttackTargetFlags::DESOLID,
                    self.parts
                        .building_of(t.id)
                        .construction
                        .as_ref()
                        .is_some_and(|c| c.desolid),
                );
                flags
            },
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

/// A unit's slot 0x468 (`FUN_141157c50`, `getPatrolInformation`): its top level's path
/// (`FUN_140f960f0`) copied into a new path ([`crate::pathing::tile_path::Path::append`]: its points, its
/// length or one measured) with its flags, and that level's waypoint; None without path data or
/// a path at the top.
pub(crate) fn patrol_information(mover: Option<&Mover>) -> Option<(PatrolPath, WaypointIndex)> {
    let top = mover?.paths.last().filter(|l| l.has_path)?;
    let mut path = Path::default();
    path.append(&Path {
        points: top.points.clone(),
        length: top.length,
    });
    let saved = PatrolPath {
        path,
        flags: top.path_flags,
    };
    Some((saved, top.waypoint))
}
