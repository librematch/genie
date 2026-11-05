//! What a hit reads of its victim: the object's values and its relations to the attacker.

use crate::GenieResult;
use crate::combat::abilities::REMOVAL_REWARD;
use crate::combat::damage::Allied;
use crate::combat::damage::{Armored, Attacker, BONUS_DAMAGE, Building as DamageBuilding};
use crate::combat::damage::{ElevationBonus, SpecialCharge, Victim};
use crate::combat::hp_transforms::{HitMaster, HitTransform, HitTransforms, hp_transforms};
use crate::combat::powerup::InfantryCount;
use crate::combat::stingers::{StingerAttackers, stinger_tasks};
use crate::combat::victims::{is_trebuchet, paid_cost};
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::map::terrain::Terrain;
use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::layers::Action;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::masters::ActionKind;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::Masters;
use crate::objects::unit::Unit;
use crate::players::AllyTest;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Diplomacy;
use crate::unit_ai::task_finder::enabled;
use crate::world::layer::Roles;
use crate::world::parts::Parts;
use crate::world::{Mode680, Unread};
use std::fmt::{self, Debug, Formatter};

/// An object's layers as a hit reads them ([`VictimView::layers`]): whether it has a unit AI, its
/// building, combat and action layers and its unread notifies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VictimLayers<'world> {
    /// It has a unit AI in the world's table.
    pub(crate) has_unit_ai: bool,
    /// Its building layer.
    pub(crate) building: &'world Building,
    /// Its combat layer.
    pub(crate) combat: &'world Combat,
    /// Its action layer.
    pub(crate) actions: &'world Action,
    /// Its unread notifies.
    pub(crate) unread: Option<&'world Unread>,
}

impl<'world> VictimLayers<'world> {
    /// The layers of object `id` in the world's `parts`, with its action layer `actions` (the
    /// table's, or the object's own while its update holds it out of the table).
    pub(crate) fn of(parts: &'world Parts, id: ObjectId, actions: &'world Action) -> Self {
        Self {
            has_unit_ai: parts.unit_ai.contains(id),
            building: parts.building_of(id),
            combat: parts.combat_of(id),
            actions,
            unread: parts.unread.get(id),
        }
    }
}

/// What a hit reads of an object ([`victim_of`]): the object, its layers, its master,
/// `FUN_1412161b0` as its caller reads it with the unit AI's target ([`ChargeOf`]), its player,
/// that player's attributes and masters, the map's terrain and the game's setting `+0x680`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VictimView<'world> {
    /// The object.
    pub(crate) unit: &'world Unit,
    /// Its layers.
    pub(crate) layers: VictimLayers<'world>,
    /// Its master.
    pub(crate) master: &'world Master,
    /// Whether its charge of special attack 3 is ready with a master (`FUN_1412161b0`).
    pub(crate) charge_of: ChargeOf<'world>,
    /// Its player.
    pub(crate) player: PlayerId,
    /// Its player's attributes.
    pub(crate) attributes: &'world [Amount],
    /// Its player's masters, the masters its hit point transforms give (`FUN_141659d20`).
    pub(crate) masters: &'world Masters,
    /// The map's terrain, which its tile's defense reads.
    pub(crate) terrain: Option<&'world Terrain>,
    /// The game's setting `+0x680`.
    pub(crate) mode_680: Mode680,
}

/// `FUN_1412161b0` of a hit's object with a master: its own, or one its hit point transforms
/// give it ([`VictimView::charge_of`]), as its caller reads it with the unit AI's target
/// ([`crate::movement::SpeedReads::special_charge`]).
#[derive(Clone, Copy)]
pub(crate) struct ChargeOf<'world>(
    pub(crate) &'world dyn Fn(&Master) -> GenieResult<SpecialCharge>,
);

impl Debug for ChargeOf<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChargeOf(FUN_1412161b0)")
    }
}

/// What a hit of an attacker of player `attacker` reads of the object of `view` ([`victim_of`]):
/// its relations to the attacker's player both ways (`dip`, [`Diplomacy::allied`] with allies),
/// whether its place is a guess (the world `spatial` holds it as unknown, or not at all), its
/// place in its tile's list and a building's annexes (the world's links).
pub(crate) fn victim_in(
    view: VictimView<'_>,
    attacker: PlayerId,
    dip: &dyn Diplomacy,
    spatial: &Spatial,
    roles: &Roles,
) -> GenieResult<Victim> {
    let mut victim = victim_of(view, roles)?;
    let (id, player_id) = (view.unit.id, view.player);
    victim.allied = Allied {
        by_attacker: dip.allied(attacker, player_id, AllyTest::WithRelation4),
        by_target: dip.allied(player_id, attacker, AllyTest::WithRelation4),
    };
    victim.uncertain = spatial
        .get(id)
        .is_none_or(|placed| placed.unknown.is_some());
    victim.tile_place = spatial.tile_place(id);
    if let Some(victim_building) = victim.building.as_mut() {
        victim_building.annexes = view.layers.building.annexes.ids().collect();
    }
    Ok(victim)
}

/// What a hit reads of the object of `view` ([`Victim`]): its master's values and its own
/// ([`master_read`]) and its hit point transforms as the hit runs them ([`hit_transforms`]).
///
/// # Errors
///
/// `FUN_1412161b0`'s read of a master, and an attribute a transform reads that its player does
/// not hold.
pub(crate) fn victim_of(view: VictimView<'_>, roles: &Roles) -> GenieResult<Victim> {
    let mut victim = master_read(view, roles)?;
    victim.transforms = hit_transforms(view, &victim, roles)?;
    Ok(victim)
}

/// The hit point transforms of the object of `view` as its hit runs them ([`HitTransforms`]),
/// `own` its read ([`master_read`]): a combat unit's or a building's (types 70 and 80,
/// `TRIBE_Combat_Object`'s slot 22), from its master on, each master a transform gives in turn
/// read as the object with it.
///
/// # Errors
///
/// As [`victim_of`].
fn hit_transforms(view: VictimView<'_>, own: &Victim, roles: &Roles) -> GenieResult<HitTransforms> {
    let own_transforms = resolved_transforms(view, view.master)?;
    if own_transforms.is_empty() {
        return Ok(HitTransforms::default());
    }
    let mut masters = vec![HitMaster {
        id: view.master.id,
        reads: own.clone(),
        transforms: own_transforms,
    }];
    let mut next = 0;
    while let Some(held) = masters.get(next) {
        let intos: Vec<MasterId> = held
            .transforms
            .iter()
            .map(|transform| transform.into)
            .collect();
        for into in intos {
            if masters.iter().any(|master| master.id == into) {
                continue;
            }
            // a transform resolves only to a master the table has
            let Some(master) = view.masters.get(into) else {
                continue;
            };
            let with = VictimView { master, ..view };
            masters.push(HitMaster {
                id: into,
                reads: master_read(with, roles)?,
                transforms: resolved_transforms(with, master)?,
            });
        }
        next += 1;
    }
    Ok(HitTransforms::new(masters))
}

/// The hit point transforms of an object of `master` ([`hp_transforms`]) with `view`'s player:
/// each whose enablement holds and whose master its player has, its threshold resolved; none for
/// an object below type 70 or above 80, whose slot 22 runs no transforms.
///
/// # Errors
///
/// An attribute a transform reads that its player does not hold.
fn resolved_transforms(view: VictimView<'_>, master: &Master) -> GenieResult<Vec<HitTransform>> {
    if !matches!(master.kind, MasterKind::COMBAT | MasterKind::BUILDING) {
        return Ok(Vec::new());
    }
    let mut resolved = Vec::new();
    for transform in hp_transforms(master) {
        let Some(threshold) = transform.threshold_of(master, view.attributes)? else {
            continue;
        };
        if let Some(into) = transform.master_given(view.masters) {
            resolved.push(HitTransform {
                threshold,
                into: into.id,
            });
        }
    }
    Ok(resolved)
}

/// What a hit reads of the object of `view` ([`Victim`]) but its hit point transforms: its
/// master's values and its own, its player's elevation attributes, the terrain defense of its
/// tile (the map's; 1 for an object inside another, which has no tile) and the game's setting
/// `+0x680`. Its relations to an attacker (`allied`) and whether its place is a guess
/// (`uncertain`) are the caller's. The slice holds neither its unit AI's alert mode and
/// attackers nor its facing.
///
/// # Errors
///
/// `FUN_1412161b0`'s read of its master.
fn master_read(view: VictimView<'_>, roles: &Roles) -> GenieResult<Victim> {
    let VictimView {
        unit,
        layers:
            VictimLayers {
                has_unit_ai,
                building,
                combat,
                actions,
                unread,
            },
        master,
        charge_of,
        player: player_id,
        attributes,
        masters: _,
        terrain,
        mode_680,
    } = view;
    let special_charge = (charge_of.0)(master)?;
    let at = |attribute: usize| attributes.get(attribute).copied().unwrap_or(Amount::ZERO);
    let kind = master.kind;
    let attack_elevation = if 212 < attributes.len() {
        ElevationBonus {
            higher: at(211).get(),
            lower: at(212).get(),
        }
    } else {
        ElevationBonus::default()
    };
    let defense = if master.combat_part().terrain_bonus.restriction().is_none()
        || unit.life.inside.is_some()
    {
        Some(1.0)
    } else {
        terrain.and_then(|terrain| {
            terrain.defense(
                master.combat_part().terrain_bonus,
                Tile::new(unit.position().x() as i32, unit.position().y() as i32),
            )
        })
    };
    let tasks = |kind: ActionKind| {
        master
            .action_part()
            .tasks
            .iter()
            .any(|task| task.action_kind == kind)
    };
    Ok(Victim {
        id: unit.id,
        // the caller's, which holds the tile lists
        tile_place: 0,
        hp: unit.life.hp,
        amount: unit.life.amount,
        state: unit.life.state,
        position: unit.position(),
        kind,
        class: master.class,
        master_id: master.id,
        radius: master.radius,
        height: master.height,
        storage: master
            .storage
            .first()
            .map_or(Amount::ZERO, |stored| stored.1),
        capacity: master.capacity,
        workers: unit.workers,
        blast_defense: master.blast_defense,
        fog_visibility: master.fog_visibility,
        goto_sleep: unit.life.goto_sleep,
        decay: master
            .life
            .map_or(AmountRate::ZERO, |master_data| master_data.decay),
        player: player_id,
        elevation: ElevationBonus {
            higher: at(272).get(),
            lower: at(273).get(),
        },
        attack_elevation,
        allied: Allied::NONE,
        uncertain: false,
        inside: unit.life.inside.is_some(),
        source: Attacker {
            id: unit.id,
            kind,
            class: master.class,
            master_id: master.id,
            ability: master.combat_part().combat_ability,
            missile: master.combat_part().missile.master(),
            weapon_range: master.combat_part().weapon_range,
            area_effect: master.combat_part().area_effect,
            blast_level: master.combat_part().blast_level,
            area_damage: master.combat_part().area_damage,
            charge_kind: master.creatable_part().charge_kind,
            max_charge: master.creatable_part().max_charge,
            charge_target: master.creatable_part().charge_target,
            special_attack: master.creatable_part().special_attack,
            charge: combat.charge,
            special_charge,
            charge_hit: combat.charge_hit,
            attack_powerup: combat.attack_powerup,
            attack_stinger: combat.attack_stinger,
            // slot 0x1b8: RGE's (FUN_14106a6b0) is 0
            infantry_inside: if matches!(kind, MasterKind::COMBAT | MasterKind::BUILDING) {
                combat.infantry_inside
            } else {
                InfantryCount::ZERO
            },
            mode_680,
            player: player_id,
            kill_bonus_tasks: tasks(ActionKind::LOOT),
            // a missile's own (its part): its own strike sets it ([`Unit::own_strike`])
            fired_from: None,
        },
        weapons: master.combat_part().weapons.clone(),
        master_hp: master.hit_points,
        bonus_tasks: master
            .action_part()
            .tasks
            .iter()
            .filter(|task| task.action_kind == BONUS_DAMAGE)
            .copied()
            .collect(),
        stinger_tasks: stinger_tasks(master)
            .into_iter()
            .filter(|task| enabled(attributes, task.enablement))
            .collect(),
        stung: Vec::new(),
        stinger_attackers: StingerAttackers::of(&combat.active_stingers),
        // a combat object's slot 0xf0 is FUN_141082530 (types 50 and up)
        armored: (MasterKind::RGE_COMBAT <= kind).then(|| Armored {
            armors: master.combat_part().armors.clone(),
            base_armor: master.combat_part().base_armor,
            terrain_bonus: master.combat_part().terrain_bonus,
            resistance: master.combat_part().resistance,
            ability: master.combat_part().combat_ability,
            reflection: master.combat_part().reflection,
            invulnerability: master.creatable_part().invulnerability,
            special_attack: master.creatable_part().special_attack,
            friendly_fire: master.combat_part().friendly_fire,
            special_sprite: master.creatable_part().special_sprite,
            terrain: defense,
            armor_powerups: combat.armor_powerups,
            armor_stingers: combat.armor_stingers,
            armor_removed: combat.armor_removed,
            hp_powerup: combat.hp_powerup,
            hp_stinger: combat.hp_stinger,
            kill_tasks: master
                .action_part()
                .tasks
                .iter()
                .any(|task| task.action_kind == REMOVAL_REWARD && 1e-7 < task.work_range.get()),
            busy: actions.top_action_kind().is_some() || unread.is_some(),
            unit_ai: has_unit_ai,
            alert_idle: None,
            facing: None,
        }),
        damaged: Vec::new(),
        killed: None,
        kill_state: combat.kill_state,
        kill_counts: Vec::new(),
        // a building's slot 0xe8 is FUN_1411cea90
        building: (kind == MasterKind::BUILDING).then(|| DamageBuilding {
            main: building.annex_of,
            // the caller's, which holds the world's annex links
            annexes: Vec::new(),
            // +0xb90 is not saved and not held: a blast resets it on every building of its tiles
            // before it reads it (FUN_14107f690's first pass), the only other one it reads being
            // the main building of an annex it reaches, which lies within the annex offset of
            // that annex, inside the six tiles the pass reaches past the radius; a trample reads
            // its target's or its main building's after its own blast
            blast_taken: HitPoints::ZERO,
            trebuchet: is_trebuchet(master, roles),
            cost: paid_cost(master),
            damaged_lately: unit.damaged_lately,
            struck: false,
            relics: building.relics,
            razed: Vec::new(),
        }),
        // the caller's (`victim_of`), which reads the masters the transforms give
        transforms: HitTransforms::default(),
        master_changes: Vec::new(),
    })
}
