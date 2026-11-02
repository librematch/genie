//! What a hit reads of its victim: the object's values and its relations to the attacker.

use crate::combat::abilities::REMOVAL_REWARD;
use crate::combat::damage::Allied;
use crate::combat::damage::{Armored, Attacker, BONUS_DAMAGE, Building as DamageBuilding};
use crate::combat::damage::{ElevationBonus, Victim};
use crate::combat::victims::{is_trebuchet, paid_cost};
use crate::map::Tile;
use crate::map::spatial::Spatial;
use crate::map::terrain::Terrain;
use crate::objects::HitPoints;
use crate::objects::layers::Action;
use crate::objects::layers::Building;
use crate::objects::layers::Combat;
use crate::objects::masters::ActionType;
use crate::objects::masters::Master;
use crate::objects::masters::action_type::LOOT;
use crate::objects::masters::master_type::{BUILDING, RGE_COMBAT};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::AmountRate;
use crate::players::PlayerId;
use crate::unit_ai::task_finder::Diplomacy;
use crate::world::{Mode680, Unread};

/// What a hit of an attacker of player `attacker` reads of the object `u` of player `p`, of
/// `master`, its player's `attributes` ([`victim_of`]): its relations to the attacker's player
/// both ways (`dip`, [`Diplomacy::allied`] with allies), whether its place is a guess (the world
/// `spatial` holds it as unknown, or not at all), its place in its tile's list and a building's
/// annexes (the world's links).
pub(crate) fn victim_in(
    (u, unit_ai, building, combat, actions, unread): (
        &Unit,
        bool,
        &Building,
        &Combat,
        &Action,
        Option<&Unread>,
    ),
    (p, master, attributes): (PlayerId, &Master, &[Amount]),
    (attacker, dip): (PlayerId, &dyn Diplomacy),
    spatial: &Spatial,
    mode_680: Mode680,
) -> Victim {
    let mut v = victim_of(
        (u, unit_ai, building, combat, actions, unread),
        master,
        attributes,
        p,
        spatial.terrain.as_ref(),
        mode_680,
    );
    v.allied = Allied::new(dip.allied(attacker, p, true), dip.allied(p, attacker, true));
    v.uncertain = spatial.get(u.id).is_none_or(|o| o.unknown.is_some());
    v.tile_place = spatial.tile_place(u.id);
    if let Some(b) = v.building.as_mut() {
        b.annexes = building.annexes.ids().collect();
    }
    v
}

/// What a hit reads of the object `u` of player `p` ([`Victim`]), whether it has a unit AI
/// (`unit_ai`), its building layer (`building`), its combat layer (`combat`) and its action
/// layer (`actions`): its master's values and its own, its
/// player's elevation attributes, the terrain defense of its tile (`terrain`, the map's;
/// 1 for an object inside another, which has no tile) and the game's setting `mode_680`. Its
/// relations to an attacker (`allied`) and whether its place is a guess (`uncertain`) are the
/// caller's. The slice holds neither its unit AI's alert mode and attackers nor its facing.
pub(crate) fn victim_of(
    (u, unit_ai, building, combat, actions, unread): (
        &Unit,
        bool,
        &Building,
        &Combat,
        &Action,
        Option<&Unread>,
    ),
    master: &Master,
    attributes: &[Amount],
    p: PlayerId,
    terrain: Option<&Terrain>,
    mode_680: Mode680,
) -> Victim {
    let at = |i: usize| attributes.get(i).copied().unwrap_or(Amount::ZERO);
    let kind = master.kind;
    let attack_elevation = if 212 < attributes.len() {
        ElevationBonus {
            higher: at(211).get(),
            lower: at(212).get(),
        }
    } else {
        ElevationBonus::default()
    };
    let defense = if master.terrain_bonus.restriction().is_none() || u.life.inside.is_some() {
        Some(1.0)
    } else {
        terrain.and_then(|t| {
            t.defense(
                master.terrain_bonus,
                Tile::new(u.position().x() as i32, u.position().y() as i32),
            )
        })
    };
    let tasks = |kind: ActionType| master.tasks.iter().any(|t| t.action_type == kind);
    Victim {
        id: u.id,
        // the caller's, which holds the tile lists
        tile_place: 0,
        hp: u.life.hp,
        amount: u.life.amount,
        state: u.life.state,
        position: u.position(),
        kind,
        class: master.class,
        master_id: master.id,
        radius: master.radius,
        height: master.height,
        storage: master.storage.first().map_or(Amount::ZERO, |s| s.1),
        capacity: master.capacity,
        workers: u.workers,
        blast_defense: master.blast_defense,
        fog_visibility: master.fog_visibility,
        goto_sleep: u.life.goto_sleep,
        decay: u.life.master.map_or(AmountRate::ZERO, |m| m.decay),
        player: p,
        elevation: ElevationBonus {
            higher: at(272).get(),
            lower: at(273).get(),
        },
        attack_elevation,
        allied: Allied::NONE,
        uncertain: false,
        inside: u.life.inside.is_some(),
        source: Attacker {
            id: u.id,
            kind,
            class: master.class,
            master_id: master.id,
            ability: master.combat_ability,
            missile: master.missile.master(),
            weapon_range: master.weapon_range,
            area_effect: master.area_effect,
            blast_level: master.blast_level,
            area_damage: master.area_damage,
            charge_type: master.charge_type,
            max_charge: master.max_charge,
            charge_target: master.charge_target,
            special_attack: master.special_attack,
            charge: combat.charge,
            charge_hit: combat.charge_hit,
            attack_powerup: combat.attack_powerup,
            mode_680,
            player: p,
            kill_bonus_tasks: tasks(LOOT),
            // a missile's own (its part): its own strike sets it ([`Unit::own_strike`])
            fired_from: None,
        },
        weapons: master.weapons.clone(),
        master_hp: master.hit_points,
        bonus_tasks: master
            .tasks
            .iter()
            .filter(|t| t.action_type == BONUS_DAMAGE)
            .copied()
            .collect(),
        // a combat object's slot 0xf0 is FUN_141082530 (types 50 and up)
        armored: (RGE_COMBAT <= kind).then(|| Armored {
            armors: master.armors.clone(),
            base_armor: master.base_armor,
            terrain_bonus: master.terrain_bonus,
            resistance: master.resistance,
            ability: master.combat_ability,
            reflection: master.reflection,
            invulnerability: master.invulnerability,
            special_attack: master.special_attack,
            friendly_fire: master.friendly_fire,
            special_graphic: master.special_graphic,
            terrain: defense,
            armor_powerups: combat.armor_powerups,
            armor_removed: combat.armor_removed,
            hp_powerup: combat.hp_powerup,
            kill_tasks: master
                .tasks
                .iter()
                .any(|t| t.action_type == REMOVAL_REWARD && 1e-7 < t.work_val1.get()),
            busy: actions.top_action_kind().is_some() || unread.is_some(),
            unit_ai,
            alert_idle: None,
            facing: None,
        }),
        damaged: Vec::new(),
        killed: None,
        // a building's slot 0xe8 is FUN_1411cea90
        building: (kind == BUILDING).then(|| DamageBuilding {
            main: building.annex_of,
            // the caller's, which holds the world's annex links
            annexes: Vec::new(),
            // +0xb90 is not saved and not held: a blast resets it on every building of its tiles
            // before it reads it (FUN_14107f690's first pass), the only other one it reads being
            // the main building of an annex it reaches, which lies within the annex offset of
            // that annex, inside the six tiles the pass reaches past the radius; a trample reads
            // its target's or its main building's after its own blast
            blast_taken: HitPoints::ZERO,
            trebuchet: is_trebuchet(master),
            cost: paid_cost(master),
            damaged_lately: u.damaged_lately,
            struck: false,
            relics: building.relics,
            razed: Vec::new(),
        }),
    }
}
