//! What the task finder reads of a save ([`FinderWorld`]).

use crate::masters::MasterTable;
use crate::setup::layer;
use crate::work_masters;
use genie_core::GenieResult;
use genie_core::gathering::PastureId;
use genie_core::map::Position;
use genie_core::objects::HitPoints;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::lifecycle::Held;
use genie_core::objects::masters::ActionKind;
use genie_core::objects::masters::PlayersMastersExt;
use genie_core::objects::masters::{MasterId, Masters};
use genie_core::players::AllyTest;
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::PlayerId;
use genie_core::players::attribute::AttributeId;
use genie_core::players::{ByPlayer, Relations};
use genie_core::unit_ai::task_finder::{
    Actor, Diplomacy, DropSite, TaskTarget, allied as task_finder_allied,
    drops_at as task_finder_drops_at,
};
use genie_model::layer::Roles;
use genie_model::layer::Slots;
use genie_rec::de::chapters;
use genie_save::generated::world::{RgeStaticObject, TribeCombatObject};
use std::collections::HashMap;
use std::sync::Arc;

/// What the task finder (`genie_core::unit_ai::task_finder`) reads of a save: every player's masters as the work
/// command reads them, every object's state, holding and flags, the players' relations and their
/// attribute 95.
#[derive(Debug)]
pub struct FinderWorld {
    /// Every player's masters.
    pub masters: Vec<Arc<Masters>>,
    /// The objects by id.
    pub objects: HashMap<ObjectId, FinderObject>,
    /// `relation[i]` of each player (`player+0x260`).
    pub relations: ByPlayer<Relations>,
    /// Every player's attribute 95.
    pub attribute_95: Vec<Amount>,
    /// Every player's civ.
    pub civs: Vec<CivId>,
    /// The drop-site table.
    pub drop_sites: Vec<DropSite>,
    /// The build's roles.
    pub roles: Roles,
    /// The build's slots.
    pub slots: Slots,
}

/// One object as the task finder reads it.
#[derive(Clone, Copy, Debug)]
pub struct FinderObject {
    /// Its player.
    pub player: PlayerId,
    /// Its master.
    pub master: MasterId,
    /// Its state.
    pub state: ObjectState,
    /// What it holds: the attribute and the amount.
    pub held: Held,
    /// Its hit points.
    pub hp: HitPoints,
    /// Its most hit points.
    pub max_hp: HitPoints,
    /// It may be attacked.
    pub attackable: bool,
    /// The pasture it is in.
    pub pasture: Option<PastureId>,
    /// The main building of an annex (`+0xbb8`), if it is one.
    pub annex_of: Option<ObjectId>,
    /// Its top action's type, if it has one.
    pub top_action: Option<ActionKind>,
    /// Its place.
    pub position: Position,
}

impl Diplomacy for FinderWorld {
    /// [`genie_core::unit_ai::task_finder::allied`] with `from`'s relations: a player without
    /// them counts only itself.
    fn allied(&self, from: PlayerId, to: PlayerId, test: AllyTest) -> bool {
        self.relations.get(from).map_or(from == to, |by_player| {
            task_finder_allied(by_player, from, to, test)
        })
    }

    fn drops_at(&self, building: MasterId, actor: &Actor<'_>) -> bool {
        task_finder_drops_at(&self.drop_sites, building, actor)
    }
}

impl FinderWorld {
    /// The finder's view of object `id`, with its master from its player's masters; an annex
    /// stands for its main building (the finder's step, `+0xbb8`), `steps` times.
    #[must_use]
    pub fn target_up(&self, id: ObjectId, steps: usize) -> Option<TaskTarget<'_>> {
        let mut id = id;
        for _ in 0..steps {
            match self.objects.get(&id)?.annex_of {
                Some(main) => id = main,
                None => break,
            }
        }
        let finder_object = self.objects.get(&id)?;
        Some(TaskTarget {
            id,
            master: self
                .masters
                .of(finder_object.player)?
                .get(finder_object.master)?,
            player: finder_object.player,
            state: finder_object.state,
            held: finder_object.held,
            hp: finder_object.hp,
            max_hp: finder_object.max_hp,
            attackable: finder_object.attackable,
            pasture: finder_object.pasture,
            top_action: finder_object.top_action,
        })
    }

    /// The finder's target: an annex stands for its main building.
    #[must_use]
    pub fn target(&self, id: ObjectId) -> Option<TaskTarget<'_>> {
        self.target_up(id, 1)
    }

    /// The finder's view of unit `id` as the actor.
    #[must_use]
    pub fn actor(&self, id: ObjectId) -> Option<Actor<'_>> {
        let finder_object = self.objects.get(&id)?;
        Some(Actor {
            id,
            master: self
                .masters
                .of(finder_object.player)?
                .get(finder_object.master)?,
            player: finder_object.player,
            held: finder_object.held,
            attribute_95: self
                .attribute_95
                .get(finder_object.player.index())
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: self
                .civs
                .get(finder_object.player.index())
                .copied()
                .unwrap_or(CivId::GAIA),
        })
    }
}

/// The task finder's inputs from a save, with the `.dat`'s values for what the save does not hold
/// and the table's drop sites (`dropsites.json`, [`drop_sites`]).
///
/// [`drop_sites`]: crate::game_tables::drop_sites
pub fn finder_world(save: &[u8], table: &MasterTable) -> GenieResult<FinderWorld> {
    let chapter = chapters(save)?[0];
    let header = chapter.header()?;
    let sections = header.players();
    let masters = sections
        .iter()
        .map(|player_section| {
            work_masters(
                player_section.objects(),
                CivId::new(player_section.world_player().base.civ_id),
                table,
            )
            .map(Arc::new)
        })
        .collect::<Result<_, _>>()?;
    let objects = finder_objects(&sections)?;
    let layer = layer()?;
    Ok(FinderWorld {
        masters,
        objects,
        relations: sections
            .iter()
            .map(|player_section| {
                Relations::of_bytes(player_section.world_player().base.relation.iter().copied())
            })
            .collect(),
        attribute_95: sections
            .iter()
            .map(|player_section| {
                player_section
                    .world_player()
                    .base
                    .attributes
                    .get(95)
                    .copied()
                    .map_or(Amount::ZERO, Amount::new)
            })
            .collect(),
        civs: sections
            .iter()
            .map(|player_section| CivId::new(player_section.world_player().base.civ_id))
            .collect(),
        drop_sites: table.drop_sites.clone(),
        roles: layer.roles.clone(),
        slots: layer.slots,
    })
}

/// The finder's view of every live and sleeping object of the players' `sections`, by id.
fn finder_objects(
    sections: &[genie_save::PlayerSection<'_>],
) -> GenieResult<HashMap<ObjectId, FinderObject>> {
    let mut objects = HashMap::new();
    for (player, section) in PlayerId::each(sections.len()).zip(sections) {
        for object in section.live_objects().chain(section.sleeping_objects()) {
            if let Some(rge_static_object) = object.static_part() {
                let id = ObjectId::try_from(rge_static_object.id)?;
                let _ = objects.insert(
                    id,
                    finder_object(player, section, (object, rge_static_object))?,
                );
            }
        }
    }
    Ok(objects)
}

/// The finder's view of one saved object `object` (its static part `rge_static_object`) of player
/// `player`'s section.
fn finder_object(
    player: PlayerId,
    section: &genie_save::PlayerSection<'_>,
    (object, rge_static_object): (&genie_save::Object, &RgeStaticObject),
) -> GenieResult<FinderObject> {
    let combat = object.combat();
    let powerup = |field: fn(&TribeCombatObject) -> f32| combat.map_or(0.0, field);
    let master_hp = section
        .objects()
        .master(rge_static_object.master_obj)
        .and_then(|master| master.static_part())
        .map_or(0.0, |rge_master_static_object| {
            f32::from(rge_master_static_object.read_temp_6)
        });
    Ok(FinderObject {
        player,
        master: MasterId::try_from(rge_static_object.master_obj)?,
        state: ObjectState::of_byte(rge_static_object.object_state)?,
        held: Held {
            kind: AttributeId::of_field(rge_static_object.attribute_kind_held)?,
            amount: Amount::new(rge_static_object.attribute_amount_held),
        },
        hp: HitPoints::new(
            rge_static_object.hp
                + (powerup(|tribe_combat_object| tribe_combat_object.hitpoints_max_powerup)
                    - powerup(|tribe_combat_object| tribe_combat_object.hitpoints_powerup)),
        ),
        max_hp: HitPoints::new(
            master_hp + powerup(|tribe_combat_object| tribe_combat_object.hitpoints_max_powerup),
        ),
        attackable: rge_static_object.unit_attackable_flag != 0,
        pasture: PastureId::of_field(rge_static_object.parent_pasture_id),
        annex_of: object
            .tribe_building_object
            .as_deref()
            .and_then(|tribe_building_object| {
                ObjectId::of_field(
                    tribe_building_object.mupt_linked_object_references_linked_owner_id,
                )
            }),
        top_action: object
            .actions()
            .and_then(|action_list| action_list.entries().next())
            .map(|(kind, _)| ActionKind::try_from(kind))
            .transpose()?,
        position: Position::new(
            rge_static_object.tx,
            rge_static_object.ty,
            rge_static_object.tz,
        ),
    })
}

#[cfg(test)]
mod tests;
