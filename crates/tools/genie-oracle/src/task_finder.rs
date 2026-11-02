//! What the task finder reads of a save ([`FinderWorld`]).

use crate::masters::MasterTable;
use crate::{Error, work_masters};
use genie_core::gathering::PastureId;
use genie_core::map::Position;
use genie_core::objects::HitPoints;
use genie_core::objects::ObjectId;
use genie_core::objects::ObjectState;
use genie_core::objects::lifecycle::Held;
use genie_core::objects::masters::ActionType;
use genie_core::objects::masters::PlayersMasters;
use genie_core::objects::masters::{MasterId, Masters};
use genie_core::players::Amount;
use genie_core::players::CivId;
use genie_core::players::PlayerId;
use genie_core::players::attribute::AttributeId;
use genie_core::players::{ByPlayer, Relations};
use genie_core::unit_ai::task_finder::{
    Actor, Diplomacy, DropSite, TaskTarget, allied as task_finder_allied,
    drops_at as task_finder_drops_at,
};
use genie_rec::de;
use genie_save::generated::world::{RgeStaticObject, TribeCombatObject};
use std::collections::HashMap;

/// What the task finder (`genie_core::unit_ai::task_finder`) reads of a save: every player's masters as the work
/// command reads them, every object's state, holding and flags, the players' relations and their
/// attribute 95.
#[derive(Debug)]
pub struct FinderWorld {
    /// Every player's masters.
    pub masters: Vec<Masters>,
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
    pub top_action: Option<ActionType>,
    /// Its place.
    pub position: Position,
}

impl Diplomacy for FinderWorld {
    /// [`genie_core::unit_ai::task_finder::allied`] with `from`'s relations: a player without
    /// them counts only itself.
    fn allied(&self, from: PlayerId, to: PlayerId, with_ally: bool) -> bool {
        self.relations
            .get(from)
            .map_or(from == to, |r| task_finder_allied(r, from, to, with_ally))
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
        let o = self.objects.get(&id)?;
        Some(TaskTarget {
            id,
            master: self.masters.of(o.player)?.get(o.master)?,
            player: o.player,
            state: o.state,
            held: o.held,
            hp: o.hp,
            max_hp: o.max_hp,
            attackable: o.attackable,
            pasture: o.pasture,
            top_action: o.top_action,
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
        let o = self.objects.get(&id)?;
        Some(Actor {
            id,
            master: self.masters.of(o.player)?.get(o.master)?,
            player: o.player,
            held: o.held,
            attribute_95: self
                .attribute_95
                .get(o.player.index())
                .copied()
                .unwrap_or(Amount::ZERO),
            civ: self
                .civs
                .get(o.player.index())
                .copied()
                .unwrap_or(CivId::GAIA),
        })
    }
}

/// The task finder's inputs from a save, with the `.dat`'s values for what the save does not hold
/// and the table's drop sites (`dropsites.json`, [`drop_sites`]).
///
/// [`drop_sites`]: crate::game_tables::drop_sites
pub fn finder_world(save: &[u8], table: &MasterTable) -> Result<FinderWorld, Error> {
    let chapter = de::chapters(save)?[0];
    let header = chapter.header()?;
    let sections = header.players();
    let masters = sections
        .iter()
        .map(|p| work_masters(p.objects(), CivId::new(p.world_player().base.civ_id), table))
        .collect::<Result<_, _>>()?;
    let objects = finder_objects(&sections)?;
    Ok(FinderWorld {
        masters,
        objects,
        relations: sections
            .iter()
            .map(|p| Relations::of_bytes(p.world_player().base.relation.iter().copied()))
            .collect(),
        attribute_95: sections
            .iter()
            .map(|p| {
                p.world_player()
                    .base
                    .attributes
                    .get(95)
                    .copied()
                    .map_or(Amount::ZERO, Amount::new)
            })
            .collect(),
        civs: sections
            .iter()
            .map(|p| CivId::new(p.world_player().base.civ_id))
            .collect(),
        drop_sites: table.drop_sites.clone(),
    })
}

/// The finder's view of every live and sleeping object of the players' `sections`, by id.
fn finder_objects(
    sections: &[genie_save::PlayerSection<'_>],
) -> Result<HashMap<ObjectId, FinderObject>, Error> {
    let mut objects = HashMap::new();
    for (player, section) in PlayerId::each(sections.len()).zip(sections) {
        for o in section.live_objects().chain(section.sleeping_objects()) {
            if let Some(s) = o.static_part() {
                let id = ObjectId::try_from(s.id).map_err(Error::Unmodelled)?;
                let _ = objects.insert(id, finder_object(player, section, (o, s))?);
            }
        }
    }
    Ok(objects)
}

/// The finder's view of one saved object `o` (its static part `s`) of player `player`'s section.
fn finder_object(
    player: PlayerId,
    section: &genie_save::PlayerSection<'_>,
    (o, s): (&genie_save::Object, &RgeStaticObject),
) -> Result<FinderObject, Error> {
    let combat = o.combat();
    let powerup = |f: fn(&TribeCombatObject) -> f32| combat.map_or(0.0, f);
    let master_hp = section
        .objects()
        .master(s.master_obj)
        .and_then(|m| m.static_part())
        .map_or(0.0, |m| f32::from(m.read_temp_6));
    Ok(FinderObject {
        player,
        master: MasterId::try_from(s.master_obj)?,
        state: ObjectState::of_byte(s.object_state)?,
        held: Held {
            kind: AttributeId::of_field(s.attribute_type_held)?,
            amount: Amount::new(s.attribute_amount_held),
        },
        hp: HitPoints::new(
            s.hp + (powerup(|c| c.hitpoints_max_powerup) - powerup(|c| c.hitpoints_powerup)),
        ),
        max_hp: HitPoints::new(master_hp + powerup(|c| c.hitpoints_max_powerup)),
        attackable: s.unit_attackable_flag != 0,
        pasture: PastureId::of_field(s.parent_pasture_id),
        annex_of: o
            .tribe_building_object
            .as_deref()
            .and_then(|b| ObjectId::of_field(b.mupt_linked_object_references_linked_owner_id)),
        top_action: o
            .actions()
            .and_then(|a| a.entries().next())
            .map(|(kind, _)| ActionType::try_from(kind))
            .transpose()?,
        position: Position::new(s.tx, s.ty, s.tz),
    })
}

#[cfg(test)]
mod tests;
