//! The repair part of the update and the unit's attackers and seen target.

use crate::GenieError;
use crate::construction::repair::payment::PaidAttributes;
use crate::construction::repair::{CountedIn, RepairReads, Repairee};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::map::spatial::Spatial;
use crate::math::fmath::BVector;
use crate::movement::Me;
use crate::movement::Speed;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ChargeType;
use crate::objects::masters::MasterKind;
use crate::objects::masters::class;
use crate::objects::masters::master_type;
use crate::objects::masters::master_type::BUILDING;
use crate::objects::masters::master_type::MOVING;
use crate::objects::masters::{Master, Masters};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::unit_ai::AiMove;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::UnitStance;
use crate::world::World;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The Repair's update ([`crate::construction::repair::RepairAction::update`]) with what it
    /// reads as it starts ([`Unit::repair_reads`]), on the Repair copied out of the action list
    /// and put back after it; its code (the end's 1). Nothing without a Repair.
    pub(crate) fn repair_part(&mut self, c: &mut Step<'_>, me: Me) -> Result<HeadCode, GenieError> {
        let Some(mut repair) = self.actions.repair().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.repair_reads(c, repair.target)?;
        let repaired = repair.update(c, self, me, &reads);
        if let Some(r) = self.actions.repair_mut() {
            *r = repair;
        }
        // its end's 1 deletes it; the destructor (FUN_140ad8fd0) finds it out of the repairers
        // (set_state left them) and lets the target it still holds go
        repaired
    }

    /// What the Repair's update reads as it starts ([`RepairReads`]): its `target` and the
    /// object its repairers count in, as the world holds them (none for a target it does not
    /// hold), the repairer's master's radii, work rate, standing sprite and terrain restriction, the
    /// game mode, the owner's type and the unit AI's stance.
    pub(crate) fn repair_reads(
        &self,
        c: &Step<'_>,
        target: Option<ObjectId>,
    ) -> Result<RepairReads, GenieError> {
        let own = repair_master(&c.w.masters, c.player, self)?;
        let (repairee, counted_in) =
            repairee_of(c, OthersRef::of(&c.w.players, c.player)?, target)?;
        Ok(RepairReads {
            target: repairee,
            counted_in,
            radius: own.radius,
            scenario: c.w.settings.game_mode.scenario(),
            rate: self.work_rate_powerup + own.work_rate,
            waits: own.class == class::VILLAGER && !c.w.players.player(c.player)?.computer,
            stance: self
                .ai
                .as_ref()
                .map_or(UnitStance::AGGRESSIVE, |ai| ai.stance),
            standing: own.standing_sprite,
            restriction: own.terrain_restriction,
        })
    }

    /// `+0x9e8` (`num_builders`): a building's in its construction, any other's its own.
    pub(crate) const fn repairers(&self) -> crate::construction::BuilderCount {
        match &self.construction {
            Some(k) => k.builders,
            None => self.builders,
        }
    }

    /// [`Unit::repairers`] to change.
    pub(crate) const fn repairers_mut(&mut self) -> &mut crate::construction::BuilderCount {
        match &mut self.construction {
            Some(k) => &mut k.builders,
            None => &mut self.builders,
        }
    }

    /// What the attack tests read of the unit as an attacker ([`crate::unit_ai::Attacker`]) with
    /// a unit AI of type `ai_type`, its master `master`, its owner and the owner's type, the game
    /// mode (a scenario's) and its `+0x924`.
    pub(crate) fn attacker_of(
        &self,
        ai_type: UnitAiType,
        master: Option<&Master>,
        (player, computer): (PlayerId, bool),
        scenario: bool,
        capture: u8,
    ) -> crate::unit_ai::Attacker {
        crate::unit_ai::Attacker {
            ai_type,
            player,
            computer,
            state: self.life.state,
            held: self.life.attribute_type,
            top_action: self.top_action_kind(),
            master: self.life.master_id,
            kind: master.map_or(MasterKind::of_data(0), |m| m.kind),
            class: master.map(|m| m.class),
            attack_priority: master
                .map_or(crate::objects::masters::AttackPriority::PRIORITY_0, |m| {
                    m.attack_priority
                }),
            missile: master.and_then(|m| m.missile.master()),
            range: master.map_or(Distance::ZERO, |m| m.weapon_range),
            min_range: master.map_or(Distance::ZERO, |m| m.min_range),
            charge_type: master.map_or(ChargeType::NONE, |m| m.charge_type),
            area_effect: master.map_or(Distance::ZERO, |m| m.area_effect),
            blast_level: master.map_or(BlastLevel::NONE, |m| m.blast_level),
            los: master.map_or(Distance::ZERO, |m| m.line_of_sight),
            position: self.position(),
            radius: master.map_or(Footprint::ZERO, |m| m.radius),
            speed: self.body.mover().map_or(Speed::ZERO, |m| m.stats.speed),
            scenario,
            firepower: self.firepower,
            capture,
        }
    }

    /// Slot 400 (`FUN_14106d090`) as the unit AI reads it: its top action's target as the world
    /// holds it (its owner's other objects as they stand, the other players' lists), with its
    /// type, place and its master's radii; None without one.
    pub(crate) fn top_target_seen(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
    ) -> Result<Option<crate::unit_ai::TopTarget>, GenieError> {
        let Some(id) = self.top_target() else {
            return Ok(None);
        };
        let Some((owner, u)) = others.find(id) else {
            return Ok(None);
        };
        let m = u.master(&c.w.masters, owner)?;
        Ok(Some(crate::unit_ai::TopTarget {
            id,
            kind: m.kind,
            position: u.position(),
            radius: m.radius,
        }))
    }

    /// `restorePatrolInformation`'s unit part ([`crate::unit_ai::Effect::RestorePatrol`]): its own
    /// waypoints go (slot 0x450) and become the saved patrol's points (slot 0x448 each, as x, y
    /// and height), and it moves to the saved waypoint (slot 0x340: range 0, forced, running; a
    /// waypoint outside the path is the game's static zero point): that move, the caller's
    /// ([`Unit::ai_move`]). None without a saved patrol.
    pub(crate) fn restore_patrol(&mut self) -> Option<AiMove> {
        let ai = self.ai.as_ref()?;
        let saved = ai.patrol_path.as_ref()?;
        let points = saved.path.points.clone();
        let w = ai
            .patrol_waypoint
            .slot()
            .and_then(|i| points.get(i))
            .copied()
            .unwrap_or(BVector::ZERO);
        if let Some(m) = self.body.mover_mut() {
            m.clear_user_waypoints();
            for p in &points {
                m.add_user_waypoint(p.x(), p.y(), p.height());
            }
        }
        Some(AiMove {
            point: Position::new(w.x(), w.y(), w.height()),
            range: Distance::ZERO,
            force: true,
            walk: false,
        })
    }
}

/// Object `u`'s master: its own (`+0x6c0`), else player `q`'s of its id.
fn repair_master(every: &[Masters], q: PlayerId, u: &Unit) -> Result<Master, GenieError> {
    Ok(u.master(every, q)?.clone())
}

/// The Repair's `target` as its update reads it ([`Repairee`]) and the object `FUN_140ad9050`
/// counts its repairers in (the target, an annex's main building) with their count: none for a
/// target the world does not hold.
fn repairee_of(
    c: &Step<'_>,
    others: OthersRef<'_>,
    target: Option<ObjectId>,
) -> Result<(Option<Repairee>, CountedIn), GenieError> {
    let held = |id: ObjectId| others.find(id);
    let Some((target, (q, t))) = target.and_then(|id| Some((id, held(id)?))) else {
        return Ok((None, None));
    };
    let tm = repair_master(&c.w.masters, q, t)?;
    let repairee = Repairee {
        state: t.life.state,
        hp: t.life.hp,
        owner: q,
        allied: crate::unit_ai::task_finder::allied(
            &c.w.players.player(c.player)?.relations,
            c.player,
            q,
            true,
        ),
        position: t.position(),
        radius: tm.radius,
        class: tm.class,
        moves: MOVING <= tm.kind && tm.moving.as_ref().is_some_and(|m| Speed::ZERO < m.speed),
    };
    let counting = if tm.kind == BUILDING {
        t.annex_of.unwrap_or(target)
    } else {
        target
    };
    let counted_in = held(counting).map(|(_, k)| (counting, k.repairers()));
    Ok((Some(repairee), counted_in))
}

/// The repair of `target` by `amount` (a building's or a combat unit's slot 0x8c8, any other's
/// slot 0x110) over the objects it reaches ([`reached_ids`]) as they stand, paid from what the
/// repairer's player holds now: each object it changed takes its hit points at once, the
/// payment joins the player's attributes. Returns the code (0 to go on, 1 once the target
/// holds its most hit points, 2 when the player cannot pay) and the target's master's type.
///
/// # Errors
///
/// A target the world does not hold or of a type below 70, an object of a master the slice
/// lacks, and an attribute the player has not got.
pub(crate) fn repair_target(
    c: &mut Step<'_>,
    target: Option<ObjectId>,
    amount: HitPoints,
) -> Result<(u8, MasterKind), GenieError> {
    let player = c.player;
    let mut set = Vec::new();
    let others = OthersRef::of(&c.w.players, player)?;
    if let Some(target) = target
        && let Some((q, t)) = others.find(target)
    {
        let tm = repair_master(&c.w.masters, q, t)?;
        for id in reached_ids(&c.w.spatial, t, target, tm.kind == BUILDING) {
            let Some(o) = others.find(id) else {
                continue;
            };
            set.push(repaired(c, others, o, id)?);
        }
    }
    let original = set.clone();
    let kind = set.first().map_or_else(MasterKind::default, |o| o.kind);
    if set.is_empty() {
        return Err(GenieError::unreachable(
            "a Repair working on an object the world does not hold",
        ));
    }
    let masters =
        c.w.masters
            .get(player.index())
            .ok_or(GenieError::missing_data("a repairer's player's masters"))?;
    // what the owner holds now: the update may have paid or gained before the repair
    let owner = c.w.players.player(player)?;
    let mut paid = PaidAttributes::new(&owner.attributes);
    let mut payer = crate::construction::repair::Payer {
        masters,
        attributes: &mut paid,
    };
    let answer = match kind {
        BUILDING => {
            crate::construction::repair::repair_building(&mut set, 0, amount, true, &mut payer)
        }
        master_type::COMBAT => {
            crate::construction::repair::repair_unit(&mut set, 0, amount, &mut payer)
        }
        // the task finder repairs buildings and combat units of its classes alone
        _ => Err(GenieError::unreachable(
            "a repaired object of a type below 70 (slot 0x110)",
        )),
    }?;
    c.w.players.player_mut(player)?.add_attributes(paid.adds)?;
    repair_write_back(c.w, &set, &original);
    Ok((answer, kind))
}

/// The objects a repair of `target` (`t`) reaches, in order: the target; for a building, an
/// annex's main building, then the main building's annexes by id, the target left out.
fn reached_ids(spatial: &Spatial, t: &Unit, target: ObjectId, building: bool) -> Vec<ObjectId> {
    let mut ids = vec![target];
    if building {
        let head = t.annex_of.unwrap_or(target);
        if head != target {
            ids.push(head);
        }
        let mut annexes = spatial.annexes_of(head);
        annexes.sort_unstable();
        ids.extend(annexes.iter().copied().filter(|&a| a != target));
    }
    ids
}

/// Object `o` of player `oq` (id `id`) as the repair reads and changes it: its master's type,
/// class, id and hit points, its hit points and powerups, a building's main building or annexes
/// (sorted), and its owner's repair cost rates.
fn repaired(
    c: &Step<'_>,
    others: OthersRef<'_>,
    (oq, o): (PlayerId, &Unit),
    id: ObjectId,
) -> Result<crate::construction::repair::Repaired, GenieError> {
    let om = repair_master(&c.w.masters, oq, o)?;
    let attributes: &[Amount] = if oq == c.player {
        &c.w.players.player(c.player)?.attributes
    } else {
        others
            .players()
            .player(oq)
            .map_or(&[][..], |p| &p.attributes[..])
    };
    let rate = |k: usize| attributes.get(k).copied().map_or(0.0, Amount::get);
    let annexes = if om.kind == BUILDING && o.annex_of.is_none() {
        let mut a = c.w.spatial.annexes_of(id);
        a.sort_unstable();
        a
    } else {
        Vec::new()
    };
    Ok(crate::construction::repair::Repaired {
        id,
        kind: om.kind,
        class: om.class,
        master: om.id,
        master_hp: om.hit_points,
        hp: o.life.hp,
        hp_powerup: o.hp_powerup,
        main: if om.kind == BUILDING {
            o.annex_of
        } else {
            None
        },
        annexes,
        unit_repair_cost: rate(crate::construction::repair::UNIT_REPAIR_COST),
        building_repair_cost: rate(crate::construction::repair::BUILDING_REPAIR_COST),
    })
}

/// The repair's changes to the objects it reached (`set` against `original`): each takes its
/// hit points at once, any player's.
fn repair_write_back(
    w: &mut World,
    set: &[crate::construction::repair::Repaired],
    original: &[crate::construction::repair::Repaired],
) {
    for (now, was) in set.iter().zip(original) {
        if now == was {
            continue;
        }
        if let Some((_, u)) = w.object_mut(now.id) {
            u.life.hp = now.hp;
            u.hp_powerup = now.hp_powerup;
        }
    }
}
