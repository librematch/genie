//! The repair part of the update and the unit's attackers and seen target.

use crate::GenieError;
use crate::combat::Firepower;
use crate::combat::damage::{Heal, heal_base_hp};
use crate::construction::MainBuildingId;
use crate::construction::RepairTargetId;
use crate::construction::repair::RepairFrom;
use crate::construction::repair::RepairOutcome;
use crate::construction::repair::payment::PaidAttributes;
use crate::construction::repair::{
    BUILDING_REPAIR_COST, Payer, Repaired, UNIT_REPAIR_COST, repair_building, repair_unit,
};
use crate::construction::repair::{CountedIn, RepairReads, Repairee};
use crate::map::Distance;
use crate::map::Footprint;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::movement::Me;
use crate::movement::Mover;
use crate::movement::Speed;
use crate::movement::move_to::Patience;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::RoleId;
use crate::objects::layers::Action;
use crate::objects::masters::BlastLevel;
use crate::objects::masters::ChargeKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::masters::{AttackPriority, MasterTables};
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::ownership::CaptureKind;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::PlayerKind;
use crate::unit_ai::AiMove;
use crate::unit_ai::OrderForce;
use crate::unit_ai::Pace;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::UnitStance;
use crate::unit_ai::task_finder::allied;
use crate::unit_ai::{Attacker, TopTarget};
use crate::world::GameMode;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::layer::Roles;
use crate::world::layer::generated::MovementTuning;
use crate::world::others::OthersRef;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;

impl Unit {
    /// The Repair's update ([`crate::construction::repair::RepairAction::update`]) with what it
    /// reads as it starts ([`Unit::repair_reads`]), on the Repair copied out of the action list
    /// and put back after it; its code (the end's 1). Nothing without a Repair.
    pub(crate) fn repair_part(
        &mut self,
        actions: &mut Action,
        c: &mut Step<'_>,
        me: Me,
        roles: &Roles,
    ) -> Result<HeadCode, GenieError> {
        let Some(mut repair) = actions.list.repair().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let reads = self.repair_reads(c, repair.target, roles)?;
        let repaired = repair.update(c, self, me, &reads, roles);
        if let Some(r) = actions.list.repair_mut() {
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
        target: Option<RepairTargetId>,
        roles: &Roles,
    ) -> Result<RepairReads, GenieError> {
        let own = repair_master(
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            c.player,
            self,
        )?;
        let (repairee, counted_in) = repairee_of(
            c,
            OthersRef::of(&c.w.players, &c.w.index, c.player)?,
            target.map(RepairTargetId::id),
        )?;
        Ok(RepairReads {
            target: repairee,
            counted_in,
            radius: own.radius,
            scenario: c.w.settings.game_mode.scenario(),
            rate: c.w.parts.combat_of(self.id).work_rate_powerup + own.action_part().work_rate,
            patience: if roles.of_class(own.class).contains(ClassRoles::VILLAGER)
                && !c.w.players.player(c.player)?.kind.is_computer()
            {
                Patience::WaitsOnce
            } else {
                Patience::Fails
            },
            stance: c
                .w
                .parts
                .unit_ai
                .get(self.id)
                .map_or(UnitStance::Aggressive, |ai| ai.stance),
            standing: own.standing_sprite,
            restriction: own.terrain_restriction,
        })
    }

    /// What the attack tests read of the unit as an attacker ([`crate::unit_ai::Attacker`]) with
    /// a unit AI of type `ai_kind`, its master `master`, its owner and the owner's type, the game
    /// mode (a scenario's), its `+0x924`, its missiles per volley (`+0xa00`) and its top speed
    /// (its mover's, 0 for an object that does not move).
    pub(crate) fn attacker_of(
        &self,
        actions: &Action,
        ai_kind: UnitAiKind,
        master: Option<&Master>,
        (player, player_kind): (PlayerId, PlayerKind),
        mode: GameMode,
        (capture, firepower, speed): (CaptureKind, Firepower, Speed),
    ) -> Attacker {
        Attacker {
            ai_kind,
            player,
            player_kind,
            state: self.life.state,
            held: self.life.attribute_kind,
            top_action: actions.top_action_kind(),
            master: self.life.master_id,
            kind: master.map_or(MasterKind::of_data(0), |m| m.kind),
            class: master.map(|m| m.class),
            attack_priority: master.map_or(AttackPriority::PRIORITY_0, |m| {
                m.creatable_part().attack_priority
            }),
            missile: master.and_then(|m| m.combat_part().missile.master()),
            range: master.map_or(Distance::ZERO, |m| m.combat_part().weapon_range),
            min_range: master.map_or(Distance::ZERO, |m| m.combat_part().min_range),
            charge_kind: master.map_or(ChargeKind::NONE, |m| m.creatable_part().charge_kind),
            area_effect: master.map_or(Distance::ZERO, |m| m.combat_part().area_effect),
            blast_level: master.map_or(BlastLevel::NONE, |m| m.combat_part().blast_level),
            los: master.map_or(Distance::ZERO, |m| m.line_of_sight),
            position: self.position(),
            radius: master.map_or(Footprint::ZERO, |m| m.radius),
            speed,
            scenario: mode.scenario(),
            firepower,
            capture,
        }
    }

    /// `restorePatrolInformation`'s unit part ([`crate::unit_ai::Effect::RestorePatrol`]): its own
    /// waypoints go (slot 0x450) and become the saved patrol's points (slot 0x448 each, as x, y
    /// and height), and it moves to the saved waypoint (slot 0x340: range 0, forced, running; a
    /// waypoint outside the path is the game's static zero point): that move, the caller's
    /// ([`Unit::ai_move`]). None without a saved patrol. `mover` is the unit's (none for one
    /// that does not move).
    pub(crate) fn restore_patrol(
        ai: Option<&UnitAi>,
        mover: Option<&mut Mover>,
        movement_tuning: &MovementTuning,
    ) -> Option<AiMove> {
        let ai = ai?;
        let saved = ai.patrol_path.as_ref()?;
        let points = saved.path.points.clone();
        let w = ai
            .patrol_waypoint
            .slot()
            .and_then(|i| points.get(i))
            .copied()
            .unwrap_or(BVector::ZERO);
        if let Some(m) = mover {
            m.clear_user_waypoints();
            for p in &points {
                m.add_user_waypoint(p.position(), movement_tuning);
            }
        }
        Some(AiMove {
            point: Position::new(w.x(), w.y(), w.height()),
            range: Distance::ZERO,
            force: OrderForce::Forced,
            pace: Pace::Run,
        })
    }
}

/// Object `u`'s master: its own (`+0x6c0`), else player `q`'s of its id.
fn repair_master(every: MasterTables<'_>, q: PlayerId, u: &Unit) -> Result<Master, GenieError> {
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
    let tm = repair_master(
        MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
        q,
        t,
    )?;
    let repairee = Repairee {
        state: t.life.state,
        hp: t.life.hp,
        owner: q,
        allied: allied(
            &c.w.players.player(c.player)?.relations,
            c.player,
            q,
            AllyTest::WithRelation4,
        ),
        position: t.position(),
        radius: tm.radius,
        class: tm.class,
        moves: MasterKind::MOVING <= tm.kind
            && tm
                .move_stats
                .as_ref()
                .is_some_and(|m| Speed::ZERO < m.speed),
    };
    let counting = if tm.kind == MasterKind::BUILDING {
        c.w.parts
            .building_of(t.id)
            .annex_of
            .map_or(target, MainBuildingId::id)
    } else {
        target
    };
    let counted_in = held(counting).map(|(_, k)| (counting, c.w.parts.repairers_of(k.id)));
    Ok((Some(repairee), counted_in))
}

/// The repair of `target` by `amount` (a building's or a combat unit's slot 0x8c8, any other's slot
/// 0x110, [`heal_base_hp`]) over the objects it reaches ([`reached_ids`]) as they stand, paid from
/// what the repairer's player holds now: each object it changed takes its hit points at once, the
/// payment joins the player's attributes. Returns the outcome ([`RepairOutcome`]: healed, full once
/// the target holds its most hit points, unpaid when the player cannot pay) and the target's
/// master's type.
///
/// [`RepairOutcome`]: RepairOutcome
///
/// # Errors
///
/// A target the world does not hold, one of class 51 or 54 below type 70, an object of a master
/// the slice lacks, and an attribute the player has not got.
pub(crate) fn repair_target(
    c: &mut Step<'_>,
    target: Option<RepairTargetId>,
    amount: HitPoints,
    roles: &Roles,
) -> Result<(RepairOutcome, MasterKind), GenieError> {
    let player = c.player;
    let mut set = Vec::new();
    let others = OthersRef::of(&c.w.players, &c.w.index, player)?;
    if let Some(target) = target.map(RoleId::id)
        && let Some((q, t)) = others.find(target)
    {
        let tm = repair_master(
            MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
            q,
            t,
        )?;
        for id in reached_ids(c.w.objects(), target, tm.kind == MasterKind::BUILDING) {
            let Some(o) = others.find(id) else {
                continue;
            };
            set.push(repaired(c, others, o, id)?);
        }
    }
    let original = set.clone();
    let Some(first) = set.first() else {
        return Err(GenieError::unreachable(
            "a Repair working on an object the world does not hold",
        ));
    };
    let (kind, kind_class) = (first.kind, first.class);
    let masters =
        c.w.model
            .masters
            .get(player.index())
            .ok_or(GenieError::missing_data("a repairer's player's masters"))?;
    // what the owner holds now: the update may have paid or gained before the repair
    let owner = c.w.players.player(player)?;
    let mut paid = PaidAttributes::new(&owner.attributes);
    let mut payer = Payer {
        masters,
        attributes: &mut paid,
    };
    let answer = match kind {
        MasterKind::BUILDING => repair_building(
            &mut set,
            0,
            amount,
            RepairFrom::Repairer,
            &mut payer,
            roles,
            c.w.model.slots,
        ),
        MasterKind::COMBAT => repair_unit(&mut set, 0, amount, &mut payer, roles, c.w.model.slots),
        // the packed and the unpacked trebuchet's classes take slot 0x8c8 of any type: below 70
        // its class's table has no such slot
        _ if roles
            .of_class(kind_class)
            .intersects(ClassRoles::PACKED_TREBUCHET | ClassRoles::UNPACKED_TREBUCHET) =>
        {
            Err(GenieError::invalid_input(
                "a repaired object of class 51 or 54 of a type without slot 0x8c8",
            )
            .with("type", kind))
        }
        // any other type's slot 0x110, its class's heal without a payment
        _ => {
            let o = set.checked_mut(0)?;
            Ok(match heal_base_hp(&mut o.hp, o.master_hp, amount) {
                Heal::Full => RepairOutcome::Full,
                Heal::Partial => RepairOutcome::Healed,
            })
        }
    }?;
    c.w.players.player_mut(player)?.add_attributes(paid.adds)?;
    repair_write_back(c.w, &set, &original)?;
    Ok((answer, kind))
}

/// The objects a repair of `target` reaches, in order: the target; for a building, an annex's
/// main building, then the main building's annexes by id, the target left out.
fn reached_ids(by_id: ObjectsRef<'_>, target: ObjectId, building: bool) -> Vec<ObjectId> {
    let mut ids = vec![target];
    if building {
        // the annex's main building, else the target is its own
        let head = by_id
            .annex_of(target)
            .unwrap_or_else(|| MainBuildingId::new(target));
        if head.id() != target {
            ids.push(head.id());
        }
        // the target's own slots, else its main building's
        let mut annexes: Vec<ObjectId> = if head.id() == target {
            by_id.annexes(target).ids().collect()
        } else {
            by_id.annexes(head.id()).ids().collect()
        };
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
) -> Result<Repaired, GenieError> {
    let om = repair_master(
        MasterTables::new(&c.w.model.masters, &c.w.parts.own_master),
        oq,
        o,
    )?;
    let attributes: &[Amount] = if oq == c.player {
        &c.w.players.player(c.player)?.attributes
    } else {
        others
            .players()
            .player(oq)
            .map_or(&[][..], |p| &p.attributes[..])
    };
    let rate = |k: usize| attributes.get(k).copied().unwrap_or(Amount::ZERO);
    let annexes =
        if om.kind == MasterKind::BUILDING && c.w.parts.building_of(o.id).annex_of.is_none() {
            let mut a: Vec<ObjectId> = c.w.parts.building_of(o.id).annexes.ids().collect();
            a.sort_unstable();
            a
        } else {
            Vec::new()
        };
    Ok(Repaired {
        id,
        kind: om.kind,
        class: om.class,
        master: om.id,
        master_hp: om.hit_points,
        hp: o.life.hp,
        hp_powerup: c.w.parts.combat_of(o.id).hp_powerup,
        main: if om.kind == MasterKind::BUILDING {
            c.w.parts.building_of(o.id).annex_of
        } else {
            None
        },
        annexes,
        unit_repair_cost: rate(UNIT_REPAIR_COST),
        building_repair_cost: rate(BUILDING_REPAIR_COST),
    })
}

/// The repair's changes to the objects it reached (`set` against `original`): each takes its
/// hit points and hit point powerups at once, any player's.
///
/// # Errors
///
/// None (an unreachable error where a table lost the part it took).
fn repair_write_back(
    w: &mut World,
    set: &[Repaired],
    original: &[Repaired],
) -> Result<(), GenieError> {
    for (now, was) in set.iter().zip(original) {
        if now == was {
            continue;
        }
        if let Some((_, u)) = w.object_mut(now.id) {
            u.life.hp = now.hp;
            w.parts.combat_mut(now.id)?.hp_powerup = now.hp_powerup;
        }
    }
    Ok(())
}

impl Action {
    /// Slot 400 (`FUN_14106d090`) as the unit AI reads it: its top action's target as the world
    /// holds it (its owner's other objects as they stand, the other players' lists), with its
    /// type, place and its master's radii; None without one.
    pub(crate) fn top_target_seen(
        &self,
        c: &Step<'_>,
        others: OthersRef<'_>,
    ) -> Result<Option<TopTarget>, GenieError> {
        let Some(id) = self.top_target() else {
            return Ok(None);
        };
        let Some((owner, u)) = others.find(id) else {
            return Ok(None);
        };
        let m = u.master(c.w.master_tables(), owner)?;
        Ok(Some(TopTarget {
            id,
            kind: m.kind,
            position: u.position(),
            radius: m.radius,
        }))
    }
}
