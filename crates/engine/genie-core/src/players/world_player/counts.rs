//! The counts the update reads: the town centres, the trainable and the upgradable masters,
//! and where a master trains.

use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::Master;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMasters;
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

impl World {
    /// `FUN_141658970`: player `p`'s first built town centre (state 2) among the ids its tracker
    /// files under masters 109 and 444 (`FUN_14104f560`, `FUN_14104f4b0`: the first list, then
    /// the second), the objects the world holds.
    #[must_use]
    pub fn first_town_centre(&self, p: PlayerId) -> Option<ObjectId> {
        let tracker = &self.players.get(p.index())?.tracker;
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&k| tracker.list(ListKey::master(k)).iter().copied().flatten())
            .find(|&id| {
                self.object(id)
                    .is_some_and(|(_, u)| u.life.state == ObjectState::Alive)
            })
    }

    /// `FUN_1416597e0(player, n)`: the `n`-th object the world holds (`FUN_1415417a0`, in any
    /// state) among the ids player `p`'s tracker files under masters 109 and 444 (`FUN_14104f560`,
    /// `FUN_14104f4b0`: the first list, then the second).
    #[must_use]
    pub fn town_centre_at(&self, p: PlayerId, n: usize) -> Option<ObjectId> {
        let tracker = &self.players.get(p.index())?.tracker;
        // a queued unit's place (â1) names no object the world holds
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&k| tracker.list(ListKey::master(k)).iter().copied().flatten())
            .filter(|&id| self.object(id).is_some())
            .nth(n)
    }

    /// `FUN_141658a30`: player `p`'s town centres not dying (state below 3): those its tracker
    /// files under masters 109 and 444, then those waiting under 109's pending list
    /// (`FUN_14104fa50`, key + 90000).
    #[must_use]
    pub fn town_centre_count(&self, p: PlayerId) -> i32 {
        let Some(player) = self.players.get(p.index()) else {
            return 0;
        };
        let tracker = &player.tracker;
        let pending =
            tracker.list(ListKey::master(town_centres(self.model.slots.master)[0]).pending());
        town_centres(self.model.slots.master)
            .iter()
            .flat_map(|&k| tracker.list(ListKey::master(k)).iter())
            .chain(pending)
            .filter(|&&id| {
                self.object(id)
                    .is_some_and(|(_, u)| u.life.state < ObjectState::Dying)
            })
            .count() as i32
    }

    /// `FUN_141625a20`'s count with no object (its fifth argument 0): the masters of player `p`,
    /// by id, of type 70 and up that the player has (`+0x5f0` not 0), that train at `building`
    /// ([`World::trains_at`]) and whose `+0x5f1` has no bit 0; for each, the limited units'
    /// test of its linked master (`FUN_140c7e1f0`) with `+0x5f1`'s bit 3 (`FUN_1416116c0`,
    /// `FUN_141611520`). With `building` 118 (a builder), attribute 48 at 0 and the town centre
    /// 621 neither had nor its `+0x5f1` set, one more when the town centres
    /// ([`World::town_centre_count`]) are fewer than attribute 218. genie's masters keep `+0x5f0`
    /// and `+0x5f1` as booleans ([`crate::objects::masters::Master::available`],
    /// [`crate::objects::masters::Master::disabled`]), a set one as 1: the limit bits 1 to 3 are none, and
    /// the limited units' tests not reached.
    #[must_use]
    pub fn trainable_count(&self, p: PlayerId, building: MasterId) -> i16 {
        let Some(masters) = self.model.masters.of(p) else {
            return 0;
        };
        let mut count = 0i16;
        for m in masters.iter() {
            let flags = u8::from(m.disabled);
            if MasterKind::COMBAT <= m.kind
                && m.available
                && self.trains_at(m, building)
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
            let Some(player) = self.players.get(p.index()) else {
                return 0;
            };
            let a = &player.attributes;
            let centre = masters.get(self.model.slots.master.town_centre_621);
            if get(a, self.model.slots.attribute.no_town_centre) == Amount::ZERO
                && centre.is_some_and(|c| !c.disabled && !c.available)
                && (self.town_centre_count(p) as f32)
                    < get(a, self.model.slots.attribute.town_centre_limit).get()
            {
                count = count.wrapping_add(1);
            }
        }
        count
    }

    /// `FUN_141610470`'s count with its fifth argument 1: the masters of player `p`, by id, of
    /// type 70 and up that train at `building` ([`World::trains_at`]): one with `+0x5f1`'s bit
    /// 1 or 2 counts; any other counts when the tech tree ([`TechTree`]) of the player's
    /// civilisation (the full tech tree's, `FUN_140c7e6f0`, under it) has it at the building
    /// within the ages (unless it is the civilisation's unique unit under the full tech tree:
    /// its elite one once the elite technology is researched, state 3), its link
    /// ([`TechTree::link`]) is no master of the player's or one it has, and its technology
    /// ([`TechTree::tech`]) is none of the player's or researched. With `building` 118,
    /// attribute 48 at 0 and the town centre 621 had with no `+0x5f1`, one more when the town
    /// centres are fewer than attribute 218. genie's masters keep `+0x5f0` and `+0x5f1` as
    /// booleans ([`World::trainable_count`]).
    ///
    /// [`TechTree::link`]: crate::players::world_player::tech_tree::TechTree::link
    /// [`TechTree::tech`]: crate::players::world_player::tech_tree::TechTree::tech
    /// [`TechTree`]: crate::players::world_player::tech_tree::TechTree
    #[must_use]
    pub fn upgradable_count(&self, p: PlayerId, building: MasterId) -> i16 {
        let Some(masters) = self.model.masters.of(p) else {
            return 0;
        };
        let Some(player) = self.players.get(p.index()) else {
            return 0;
        };
        let full = self.settings.full_tech_tree;
        let fallback = full_tree_civ(self.mode_680);
        let civ = if full { fallback } else { player.civ };
        let state = |t: TreeTech| t.index().and_then(|t| player.techs.get(t)).map(|t| t.state);
        let ages_out = AGES.map(|t| state(TreeTech::of_tech(t)) == Some(ResearchState::Disabled));
        let unique = self
            .model
            .tech_tree
            .unique_units
            .get(player.civ.index())
            .copied()
            .flatten()
            .map_or(TreeMaster::NONE, |u| match state(u.elite_tech) {
                // A technology's state once researched.
                Some(ResearchState::Researched) => u.elite,
                Some(_) => u.unit,
                None => TreeMaster::NONE,
            });
        let mut count = 0i16;
        for m in masters.iter() {
            if m.kind < MasterKind::COMBAT || !self.trains_at(m, building) {
                continue;
            }
            let flags = u8::from(m.disabled);
            if flags & 6 != 0 {
                count = count.wrapping_add(1);
                continue;
            }
            let q = TreeQuery {
                civ,
                building: TreeMaster::of_master(building),
                unit: TreeMaster::of_master(m.id),
                ages_out,
                settings: &self.settings,
                fallback,
            };
            if (!full || TreeMaster::of_master(m.id) != unique) && !self.model.tech_tree.has(&q) {
                continue;
            }
            let link = self.model.tech_tree.link(&q);
            let had = link
                .master()
                .and_then(|l| masters.get(l))
                .map(|l| l.available);
            if had.is_none_or(|a| a) {
                let tech = self.model.tech_tree.tech(&q);
                if state(tech).is_none_or(|s| s == ResearchState::Researched) {
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
            let a = &player.attributes;
            let centre = masters.get(self.model.slots.master.town_centre_621);
            if get(a, self.model.slots.attribute.no_town_centre) == Amount::ZERO
                && centre.is_some_and(|c| !c.disabled && c.available)
                && (self.town_centre_count(p) as f32)
                    < get(a, self.model.slots.attribute.town_centre_limit).get()
            {
                count = count.wrapping_add(1);
            }
        }
        count
    }

    /// `FUN_141263610`: master `m` trains at `building`: one of its train locations (`+0x898`)
    /// is `building`, or the building's stack unit (`+0x986`) when the local player's master of
    /// it ([`World::local_player`], `FUN_140c88440`) is of class 3, disappears when built
    /// (`+0x984`) and has one (0 or more). A master without train locations trains nowhere.
    #[must_use]
    pub fn trains_at(&self, m: &Master, building: MasterId) -> bool {
        if m.creatable_part().trained_at.is_empty() {
            return false;
        }
        let mut at = building;
        let local = player_master(&self.model.masters, self.local_player, building);
        if let Some(b) = local.filter(|b| {
            self.model
                .roles
                .of_class(b.class)
                .contains(ClassRoles::BUILDING)
        }) && let Some(site) = b.building.as_ref()
            && site.disappears_when_built
            && let Some(stack) = site.stack_unit
        {
            at = stack;
        }
        m.creatable_part().trained_at.contains(&at)
    }
}
