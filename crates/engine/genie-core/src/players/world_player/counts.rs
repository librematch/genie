//! The counts the update reads: the town centres, the trainable and the upgradable masters,
//! and where a master trains.

use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Availability;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::player_master;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::tracker::ListKey;
use crate::players::world_player::{
    AGES, TreeMaster, TreeQuery, TreeTech, full_tree_civ, get, town_centres,
};
use crate::technology::ResearchState;
use crate::world::World;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::settings::OptionFlags;

impl World {
    /// `FUN_141658970`: player `player_id`'s first built town centre (state 2) among the ids its
    /// tracker files under masters 109 and 444 (`FUN_14104f560`, `FUN_14104f4b0`: the first list,
    /// then the second), the objects the world holds.
    #[must_use]
    pub fn first_town_centre(&self, player_id: PlayerId) -> Option<ObjectId> {
        let tracker = &self.players.get(player_id.index())?.tracker;
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&master_id| {
                tracker
                    .list(ListKey::master(master_id))
                    .iter()
                    .copied()
                    .flatten()
            })
            .find(|&id| {
                self.object(id)
                    .is_some_and(|(_, unit)| unit.life.state == ObjectState::Alive)
            })
    }

    /// `FUN_1416597e0(player, n)`: the `nth`-th object the world holds (`FUN_1415417a0`, in any
    /// state) among the ids player `player_id`'s tracker files under masters 109 and 444
    /// (`FUN_14104f560`, `FUN_14104f4b0`: the first list, then the second).
    #[must_use]
    pub fn town_centre_at(&self, player_id: PlayerId, nth: usize) -> Option<ObjectId> {
        let tracker = &self.players.get(player_id.index())?.tracker;
        // a queued unit's place (−1) names no object the world holds
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&master_id| {
                tracker
                    .list(ListKey::master(master_id))
                    .iter()
                    .copied()
                    .flatten()
            })
            .filter(|&id| self.object(id).is_some())
            .nth(nth)
    }

    /// `FUN_141658a30`: player `player_id`'s town centres not dying (state below 3): those its
    /// tracker files under masters 109 and 444, then those waiting under 109's pending list
    /// (`FUN_14104fa50`, key + 90000).
    #[must_use]
    pub fn town_centre_count(&self, player_id: PlayerId) -> i32 {
        let Some(player) = self.players.get(player_id.index()) else {
            return 0;
        };
        let tracker = &player.tracker;
        let pending =
            tracker.list(ListKey::master(town_centres(self.model.slots.master)[0]).pending());
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&master_id| tracker.list(ListKey::master(master_id)).iter())
            .chain(pending)
            .filter(|&&id| {
                self.object(id)
                    .is_some_and(|(_, unit)| unit.life.state < ObjectState::Dying)
            })
            .count() as i32
    }

    /// `FUN_141625a20`'s count with no object (its fifth argument 0): the masters of player
    /// `player_id`, by id, of type 70 and up that the player has (`+0x5f0` not 0), that train at
    /// `building` ([`World::trains_at`]) and whose `+0x5f1` has no bit 0; for each, the limited
    /// units' test of its linked master (`FUN_140c7e1f0`) with `+0x5f1`'s bit 3 (`FUN_1416116c0`,
    /// `FUN_141611520`). With `building` 118 (a builder), attribute 48 at 0 and the town centre 621
    /// neither had nor its `+0x5f1` set, one more when the town centres
    /// ([`World::town_centre_count`]) are fewer than attribute 218. genie's masters keep `+0x5f0`
    /// and `+0x5f1` as the data's bytes ([`crate::objects::masters::Master::available`],
    /// [`crate::objects::masters::Master::disabled`]); the `.dat` and the saves hold 0 and 1 only,
    /// so only an effect on the disabled flag sets the limit bits 1 to 3, and the limited units'
    /// test of bit 3 is not ported (MISSING.md).
    #[must_use]
    pub fn trainable_count(&self, player_id: PlayerId, building: MasterId) -> i16 {
        let Some(masters) = self.model.masters.of(player_id) else {
            return 0;
        };
        let mut count = 0i16;
        for master in masters {
            let flags = master.disabled.get();
            if MasterKind::COMBAT <= master.kind
                && master.available.is_available()
                && self.trains_at(master, building)
                && flags & 1 == 0
            {
                count = count.wrapping_add(1);
            }
        }
        // The wonder's master (`0x61b0` raw, `0x14163bb68`).
        // The builder's master (`FUN_141625a20`'s 0x76).
        // The town centre a builder may place (`FUN_141625a20`'s 0x26d).
        if self
            .model
            .roles
            .of_master(building)
            .contains(MasterRoles::BUILDER)
        {
            let Some(player) = self.players.get(player_id.index()) else {
                return 0;
            };
            let amounts = &player.attributes;
            let centre = masters.get(self.model.slots.master.town_centre_621);
            if get(amounts, self.model.slots.attribute.no_town_centre) == Amount::ZERO
                && centre.is_some_and(|master| {
                    !master.disabled.is_disabled() && !master.available.is_available()
                })
                && (self.town_centre_count(player_id) as f32)
                    < get(amounts, self.model.slots.attribute.town_centre_limit).get()
            {
                count = count.wrapping_add(1);
            }
        }
        count
    }

    /// `FUN_141610470`'s count with its fifth argument 1: the masters of player `player_id`, by id,
    /// of type 70 and up that train at `building` ([`World::trains_at`]): one with `+0x5f1`'s bit 1
    /// or 2 counts; any other counts when the tech tree ([`TechTree`]) of the player's civilisation
    /// (the full tech tree's, `FUN_140c7e6f0`, under it) has it at the building within the ages
    /// (unless it is the civilisation's unique unit under the full tech tree: its elite one once
    /// the elite technology is researched, state 3), its link ([`TechTree::link`]) is no master of
    /// the player's or one it has, and its technology ([`TechTree::tech`]) is none of the player's
    /// or researched. With `building` 118, attribute 48 at 0 and the town centre 621 had with no
    /// `+0x5f1`, one more when the town centres are fewer than attribute 218. genie's masters keep
    /// `+0x5f0` and `+0x5f1` as booleans ([`World::trainable_count`]).
    ///
    /// [`TechTree::link`]: crate::players::world_player::tech_tree::TechTree::link
    /// [`TechTree::tech`]: crate::players::world_player::tech_tree::TechTree::tech
    /// [`TechTree`]: crate::players::world_player::tech_tree::TechTree
    #[must_use]
    pub fn upgradable_count(&self, player_id: PlayerId, building: MasterId) -> i16 {
        let Some(masters) = self.model.masters.of(player_id) else {
            return 0;
        };
        let Some(player) = self.players.get(player_id.index()) else {
            return 0;
        };
        let full = self.settings.flags.contains(OptionFlags::FULL_TECH_TREE);
        let fallback = full_tree_civ(self.mode_680);
        let civ = if full { fallback } else { player.civ };
        let state = |tree_tech: TreeTech| {
            tree_tech
                .index()
                .and_then(|index| player.techs.get(index))
                .map(|tech_state| tech_state.state)
        };
        let ages_out =
            AGES.map(|tech_id| state(TreeTech::of_tech(tech_id)) == Some(ResearchState::Disabled));
        let unique = self
            .model
            .tech_tree
            .unique_units
            .get(player.civ.index())
            .copied()
            .flatten()
            .map_or(TreeMaster::NONE, |unique_unit| {
                match state(unique_unit.elite_tech) {
                    // A technology's state once researched.
                    Some(ResearchState::Researched) => unique_unit.elite,
                    Some(_) => unique_unit.unit,
                    None => TreeMaster::NONE,
                }
            });
        let mut count = 0i16;
        for trained in masters {
            if trained.kind < MasterKind::COMBAT || !self.trains_at(trained, building) {
                continue;
            }
            let flags = trained.disabled.get();
            if flags & 6 != 0 {
                count = count.wrapping_add(1);
                continue;
            }
            let tree_query = TreeQuery {
                civ,
                building: TreeMaster::of_master(building),
                unit: TreeMaster::of_master(trained.id),
                ages_out,
                settings: &self.settings,
                fallback,
            };
            if (!full || TreeMaster::of_master(trained.id) != unique)
                && !self.model.tech_tree.has(&tree_query)
            {
                continue;
            }
            let link = self.model.tech_tree.link(&tree_query);
            let had = link
                .master()
                .and_then(|master_id| masters.get(master_id))
                .map(|master| master.available);
            if had.is_none_or(Availability::is_available) {
                let tech = self.model.tech_tree.tech(&tree_query);
                if state(tech)
                    .is_none_or(|research_state| research_state == ResearchState::Researched)
                {
                    count = count.wrapping_add(1);
                }
            }
        }
        if self
            .model
            .roles
            .of_master(building)
            .contains(MasterRoles::BUILDER)
        {
            let amounts = &player.attributes;
            let centre = masters.get(self.model.slots.master.town_centre_621);
            if get(amounts, self.model.slots.attribute.no_town_centre) == Amount::ZERO
                && centre.is_some_and(|master| {
                    !master.disabled.is_disabled() && master.available.is_available()
                })
                && (self.town_centre_count(player_id) as f32)
                    < get(amounts, self.model.slots.attribute.town_centre_limit).get()
            {
                count = count.wrapping_add(1);
            }
        }
        count
    }

    /// `FUN_141263610`: master `trained` trains at `building`: one of its train locations
    /// (`+0x898`) is `building`, or the building's stack unit (`+0x986`) when the local player's
    /// master of it ([`World::local_player`], `FUN_140c88440`) is of class 3, disappears when built
    /// (`+0x984`) and has one (0 or more). A master without train locations trains nowhere.
    #[must_use]
    pub fn trains_at(&self, trained: &Master, building: MasterId) -> bool {
        if trained.creatable_part().trained_at.is_empty() {
            return false;
        }
        let mut at = building;
        let local = player_master(&self.model.masters, self.local_player, building);
        if let Some(master) = local.filter(|master| {
            self.model
                .roles
                .of_class(master.class)
                .contains(ClassRoles::BUILDING)
        }) && let Some(site) = master.building.as_ref()
            && site.disappears_when_built.disappears()
            && let Some(stack) = site.stack_unit
        {
            at = stack;
        }
        trained.creatable_part().trained_at.contains(&at)
    }
}
