//! A master's upgrade (slot 0x1a0) and its objects, the objects an effect spawns, the objects'
//! kinds and a negative cost's entries.

use crate::construction::build::world_side::MakeMode;
use crate::garrison::ContainerId;
use crate::map::Point;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::CostAmount;
use crate::objects::masters::CostSlot;
use crate::objects::masters::Master;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::cost_resource;
use crate::objects::masters::player_master;
use crate::objects::unit::Unit;
use crate::ownership::change::DEAD_COPY_KINDS;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::technology::UpgradeMode;
use crate::technology::effect::attribute_modifier::EffectHook;
use crate::technology::upgrade::UpgradeMaster;
use crate::technology::{CostEdit, CostUnits, ObjectsKind};
use crate::world::Player;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::MasterRoles;
use crate::world::parts::Part;
use crate::world::slots::{CheckedIndexExt, PlayerSlotsExt};
use crate::{GenieError, GenieResult};
use std::sync::Arc;

/// The masters [`World::upgrade_walk`] reads: the table's old and new ones, and the dead unit
/// master whose objects take masters of their own.
#[derive(Clone, Copy)]
struct Masters<'master> {
    old: &'master Master,
    new: &'master Master,
    dead: Option<&'master Master>,
}

impl World {
    /// Slot 0x1a0 (`FUN_14160e410`) of player `player_id`: master `from` upgrades to `to` by `mode`
    /// ([`World::upgrade_objects`]), with the volleys' hooks around it (the abilities, `+0x930`,
    /// are not held).
    pub(super) fn upgrade_master(
        &mut self,
        player_id: PlayerId,
        (from, to): (Option<MasterId>, Option<MasterId>),
        mode: UpgradeMode,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        self.all_garrison_volleys(player_id, EffectHook::Before)?;
        if let (Some(from), Some(to)) = (from, to) {
            self.upgrade_objects(player_id, from, to, mode, object)?;
        }
        self.all_garrison_volleys(player_id, EffectHook::After)
    }

    /// `FUN_141657d10`, the upgrade's own part: of two masters `from` and `to` of player
    /// `player_id`'s table, mode 2 with the effect's `object` gives that object a master of its own
    /// (`FUN_141186d10`, a copy of the one it reads) where slot 0xa8 lets it
    /// ([`crate::technology::upgrade::upgradable`]), runs its slot 0x98 to `to` keeping its master
    /// ([`World::upgrade_object`]) and copies `to` into its own master (slot 8). Any other mode
    /// goes through the live objects in order: one reading `from` takes its slot 0x98 (its master
    /// kept in modes −1 and 3), or past its use a master of its own and its old death spawn's copy
    /// (`FUN_141201d50`); one reading `from`'s dead unit master (`+0x5e4`, when `to`'s differs by
    /// its copy id) takes a master of its own; in mode 3 one whose own master has `from`'s id takes
    /// slot 0x98 keeping it and copies `to` into it (a building's research buttons,
    /// `FUN_1411c78f0`, are the interface's). Then the sleeping objects reading `from` take slot
    /// 0x98. Upgrades to 0x67f and 0x6a1 move the tracker's list of `from` to `to`'s
    /// (`FUN_14104fd10`, [`crate::players::tracker::Tracker::move_all`]). In modes −1 and 3 two
    /// masters of one type end with `from` taking `to`'s values ([`Master::copy_from`]; the
    /// masters' slot 0xa8, their graphics' loading, is not simulation).
    pub(crate) fn upgrade_objects(
        &mut self,
        player_id: PlayerId,
        from: MasterId,
        to: MasterId,
        mode: UpgradeMode,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        let Some(masters) = self.model.masters.of(player_id) else {
            return Ok(());
        };
        let (Some(old), Some(new)) = (masters.get(from), masters.get(to)) else {
            return Ok(());
        };
        // the dead unit master whose objects take masters of their own: `from`'s when `to`'s
        // differs by its copy id
        let dead = old.death_spawns[0]
            .and_then(|id| masters.get(id))
            .filter(|other| {
                new.death_spawns[0]
                    .and_then(|id| masters.get(id))
                    .is_some_and(|master| other.copy_id != master.copy_id)
            });
        let dead = dead.cloned();
        let keep = if mode.keeps_masters() {
            UpgradeMaster::Kept
        } else {
            UpgradeMaster::Replaced
        };
        let (old, new) = (old.clone(), new.clone());
        if mode.one_object()
            && let Some(id) = object
        {
            self.upgrade_named(player_id, id, &new)?;
        } else {
            let masters = Masters {
                old: &old,
                new: &new,
                dead: dead.as_ref(),
            };
            self.upgrade_walk(player_id, (from, mode, keep), masters)?;
        }
        // the upgrades to 0x67f and 0x6a1 move the tracker's list of `from` (FUN_14104fd10)
        self.track_move_upgraded(player_id, from, to)?;
        if keep == UpgradeMaster::Kept
            && old.kind == new.kind
            && let Some(master) =
                Arc::make_mut(self.model.masters.checked_mut(player_id.index())?).get_mut(from)
        {
            master.copy_from(&new);
        }
        Ok(())
    }

    /// Mode 2's own part of [`World::upgrade_objects`]: the object `id` gets a master of its own
    /// (a copy of the one it reads) where its kind lets it, takes slot 0x98 to `new` and copies
    /// `new` into its own master.
    fn upgrade_named(
        &mut self,
        player_id: PlayerId,
        id: ObjectId,
        new: &Master,
    ) -> GenieResult<()> {
        use crate::technology::upgrade::upgradable;
        if let Some(place) = self.place(id) {
            let unit = self.object_at(place)?;
            let reads = self.master_read(player_id, unit)?;
            if upgradable(reads.kind, unit.life.state) {
                self.set_part(id, Part::OwnMaster(Some(Box::new(reads.clone()))))?;
                self.upgrade_object(player_id, place, (&reads, new), UpgradeMaster::Kept)?;
                if let Some(own) = self.parts.own_master.get_mut(id) {
                    own.copy_from(new);
                }
            }
        }
        Ok(())
    }

    /// The other modes of [`World::upgrade_objects`]: the live objects in order, then the
    /// sleeping ones, reading `from` (`keep` its master, `mode` 3 also those whose own master has
    /// `from`'s id); `masters` holds the table's old and new masters and the dead unit master.
    fn upgrade_walk(
        &mut self,
        player_id: PlayerId,
        (from, mode, keep): (MasterId, UpgradeMode, UpgradeMaster),
        Masters { old, new, dead }: Masters<'_>,
    ) -> GenieResult<()> {
        use crate::technology::upgrade::upgradable;
        use crate::world::index::Place;
        for list_index in self.players.player(player_id)?.objects.places() {
            let place = Place::Live(player_id, list_index);
            let (id, master_id, state) = {
                let live = self.players.live(player_id, list_index)?;
                (live.id, live.life.master_id, live.life.state)
            };
            let table = !self.parts.own_master.contains(id);
            if table && master_id == from {
                if upgradable(old.kind, state) {
                    self.upgrade_object(player_id, place, (old, new), keep)?;
                } else {
                    self.own_and_dead_master(player_id, list_index, old)?;
                }
            } else if table && let Some(master) = dead.filter(|master| master.id == master_id) {
                self.set_part(id, Part::OwnMaster(Some(Box::new(master.clone()))))?;
            } else if mode.own_masters()
                && let Some(own) = self
                    .parts
                    .own_master
                    .get(id)
                    .filter(|own_master| own_master.id == from)
                && upgradable(own.kind, state)
            {
                let own = own.clone();
                self.upgrade_object(player_id, place, (&own, new), UpgradeMaster::Kept)?;
                if let Some(master) = self.parts.own_master.get_mut(id) {
                    master.copy_from(new);
                }
            }
        }
        for list_index in self.players.player(player_id)?.sleeping.places() {
            let unit = &self.players.asleep(player_id, list_index)?;
            if !self.parts.own_master.contains(unit.id) && unit.life.master_id == from {
                self.upgrade_object(
                    player_id,
                    Place::Sleeping(player_id, list_index),
                    (old, new),
                    keep,
                )?;
            }
        }
        Ok(())
    }

    /// The upgrades to 0x67f and 0x6a1 move the tracker's list of `from` (`FUN_14104fd10`).
    fn track_move_upgraded(
        &mut self,
        player_id: PlayerId,
        from: MasterId,
        to: MasterId,
    ) -> GenieResult<()> {
        if self
            .model
            .roles
            .of_master(to)
            .intersects(MasterRoles::FLEMISH_MILITIA | MasterRoles::FLEMISH_MILITIA_FEMALE)
        {
            self.track_move_all(player_id, from, to)?;
        }
        Ok(())
    }

    /// The master object `unit` of player `player_id` reads: its own, else its player's.
    pub(super) fn master_read(&self, player_id: PlayerId, unit: &Unit) -> GenieResult<Master> {
        unit.master(self.master_tables(), player_id).cloned()
    }

    /// An object of master `old` past its use (slot 0xa8 false) in an upgrade: a master of its own
    /// (`FUN_141186d10`: a copy of `old` unless it has one) and its dead master
    /// (`FUN_141201d50`: unless it has one, a copy of `old`'s death spawn, `+0x5e4`, of type 10 to
    /// 40, 70 or 80, from its player's table; its slot 0xa8, the graphics, is not simulation).
    pub(super) fn own_and_dead_master(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        old: &Master,
    ) -> GenieResult<()> {
        let dead = old.death_spawns[0]
            .and_then(|id| player_master(&self.model.masters, player_id, id))
            .cloned();
        let id = self.players.live(player_id, list_index)?.id;
        if !self.parts.own_master.contains(id) {
            self.set_part(id, Part::OwnMaster(Some(Box::new(old.clone()))))?;
        }
        if !self.parts.dead_master.contains(id)
            && old.death_spawns[0].is_some()
            && let Some(master) = dead.filter(|master| DEAD_COPY_KINDS.contains(&master.kind))
        {
            self.set_part(id, Part::DeadMaster(Box::new(master)))?;
        }
        Ok(())
    }

    /// Slot 0x1a8 (`FUN_1416136f0`) of player `player_id`: `count` objects of master `master`
    /// (Gaia's when the player lacks it) at each of its built objects of building master `at`
    /// (state 2, in its list's order), up to attribute 234's count of buildings when above 0, in a
    /// playback (the screen's mode is not 4). Each object: a male villager (0x53) draws its sex
    /// from the Player stream ("SpawnObject": `draw % 100` above 49 a female, 0x125); the owner's
    /// slot 0x1c0 makes it at the building's place with its flag ([`World::make_new_object`]); its
    /// slot 0x1a0 takes it into the building where the building has room for one more (slot 0x198,
    /// [`World::has_room`]); unless the player's attribute 283 is above 0 it comes out at once
    /// toward no point (`FUN_1411e7220`, [`World::let_out`]); a relic (0x11d) counts in the world's
    /// `+0x844` ([`World::spawned_relics`]). The local player's statistics of effect 0x318's spawns
    /// are not simulation.
    pub(super) fn spawn_at(
        &mut self,
        player_id: PlayerId,
        (master, at): (Option<MasterId>, Option<MasterId>),
        count: i16,
    ) -> GenieResult<()> {
        let has = |player: usize, id: MasterId| {
            self.model
                .masters
                .get(player)
                .is_some_and(|masters| masters.get(id).is_some())
        };
        let building = at
            .and_then(|at| player_master(&self.model.masters, player_id, at))
            .is_some_and(|master| master.kind == MasterKind::BUILDING);
        let (Some(master), Some(at)) = (master, at) else {
            return Ok(());
        };
        if !building || !(has(player_id.index(), master) || has(0, master)) || count <= 0 {
            return Ok(());
        }
        let owner = if has(player_id.index(), master) {
            player_id
        } else {
            PlayerId::GAIA
        };
        let attribute = |player: &Player, attribute: usize| {
            player
                .attributes
                .get(attribute)
                .copied()
                .unwrap_or(Amount::ZERO)
        };
        let limit = attribute(self.players.player(player_id)?, 234).get() as i32;
        let buildings: Vec<ObjectId> = self
            .players
            .player(player_id)?
            .objects
            .iter()
            .filter(|unit| unit.life.master_id == at && unit.life.state == ObjectState::Alive)
            .map(|unit| unit.id)
            .collect();
        for (index, &object_id) in buildings.iter().enumerate() {
            if 0 < limit && limit <= index as i32 {
                return Ok(());
            }
            for _ in 0..count {
                let made = if self
                    .model
                    .roles
                    .of_master(master)
                    .contains(MasterRoles::VILLAGER_MALE)
                {
                    let draw = self.streams.get_mut(Stream::Player).rand15() as i32;
                    if 49 < draw % 100 {
                        // The male and female villagers' masters.
                        // The relic's master, which the world counts.
                        self.model.slots.master.villager_female
                    } else {
                        self.model.slots.master.villager_male
                    }
                } else {
                    master
                };
                let Some((_, building)) = self.object(object_id) else {
                    break;
                };
                let position = building.position();
                let Some(new) = self.make_new_object(owner, made, position, MakeMode::Finished)?
                else {
                    continue;
                };
                let Some((bp, bu)) = self.object(object_id) else {
                    break;
                };
                if self.has_room(bp, bu, None)? {
                    let Some(Place::Live(np, ni)) = self.place(new) else {
                        continue;
                    };
                    self.enter_forced(np, ni, ContainerId::new(object_id))?;
                }
                if attribute(self.players.player(player_id)?, 283) <= Amount::ZERO {
                    let Some(Place::Live(bp, bi)) = self.place(object_id) else {
                        break;
                    };
                    let _ = self.let_out(bp, bi, new, Point::new(-1.0, -1.0), (None, false))?;
                }
                if self
                    .model
                    .roles
                    .of_master(master)
                    .contains(MasterRoles::RELIC)
                {
                    self.spawned_relics = self.spawned_relics.plus_one();
                }
            }
        }
        Ok(())
    }

    /// The objects' kinds (`FUN_140ec2ac0`'s 200 to 206): 200 to 202 change the objects with
    /// masters of their own of the command's master (`first`) or class (`second`) (`FUN_141659dd0`;
    /// the slice holds no own masters: none); 203 to 205 the effect's object, which first takes a
    /// master of its own (`FUN_14165abe0`, `FUN_141186d10`, not held); 206 changes the effect's
    /// object's master (`FUN_14165ae40`). Without an object only 200 to 202 run.
    pub(super) const fn object_effect(
        kind: ObjectsKind,
        object: Option<ObjectId>,
    ) -> GenieResult<()> {
        match (kind, object) {
            (ObjectsKind::OwnMasters(_), _) | (_, None) => Ok(()),
            (ObjectsKind::ObjectMaster, Some(_)) => Err(GenieError::unsupported(
                "an effect's master change of its object (kind 206, FUN_14165ae40)",
            )),
            (ObjectsKind::Object(_), Some(_)) => Err(GenieError::unsupported(
                "an effect on its object's own master (kinds 203 to 205, FUN_141186d10)",
            )),
        }
    }

    /// `FUN_1415c1f70` (a multiply of a cost by a negative amount) of player `player_id`: the cost
    /// attribute (103 to 106) names the resource (food, wood, gold 3, stone 2); a removal
    /// (`how` −2) empties its first cost slot of the masters `units` name in its place
    /// (`FUN_141612df0`: −1, 0, unpaid), an addition (−1) fills the first empty slot with it,
    /// amount −1 and paid (`FUN_14160c450`); a class names every master of it by id. Only a
    /// master from type 70 on has costs. The game scans four slots, the fourth past the three
    /// (not modelled).
    pub(super) fn cost_entries(
        &mut self,
        player_id: PlayerId,
        units: Option<CostUnits>,
        attribute: MasterAttribute,
        edit: Option<CostEdit>,
    ) {
        let Some(resource) = cost_resource(attribute) else {
            return;
        };
        let (Some(units), Some(edit)) = (units, edit) else {
            return;
        };
        let Some(masters) = self.model.masters.of_mut(player_id) else {
            return;
        };
        for master in masters.iter_mut() {
            let named = match units {
                CostUnits::Master(id) => master.id == id,
                CostUnits::Class(class) => master.class == class,
            };
            if !named || master.kind < MasterKind::COMBAT {
                continue;
            }
            // a tree (type 90) has no creatable layer, so no costs to edit
            let Some(part) = master.creatable.as_mut() else {
                continue;
            };
            let wanted = match edit {
                CostEdit::Remove => Some(resource),
                CostEdit::Add => None,
            };
            if let Some(slot) = part
                .cost
                .iter_mut()
                .find(|cost_slot| cost_slot.attribute == wanted)
            {
                *slot = match edit {
                    CostEdit::Remove => CostSlot::default(),
                    CostEdit::Add => CostSlot::new(resource, CostAmount::of_data(-1), true),
                };
            }
        }
    }
}
