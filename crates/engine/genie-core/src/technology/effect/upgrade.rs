//! A master's upgrade (slot 0x1a0) and its objects, the objects an effect spawns, the objects'
//! kinds and a negative cost's entries.

use crate::GenieError;
use crate::map::Point;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::MasterAttribute;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::ids::{self, RELIC, VILLAGER_FEMALE, VILLAGER_MALE};
use crate::objects::masters::{CostSlot, Master, master_type, player_master};
use crate::objects::unit::Unit;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::technology::{CostEdit, CostUnits, ObjectsKind};
use crate::world::World;
use crate::world::slots::{CheckedIndex, PlayerSlots};

/// The masters [`World::upgrade_walk`] reads: the table's old and new ones, and the dead unit
/// master whose objects take masters of their own.
#[derive(Clone, Copy)]
struct Masters<'a> {
    old: &'a Master,
    new: &'a Master,
    dead: Option<&'a Master>,
}

impl World {
    /// Slot 0x1a0 (`FUN_14160e410`) of player `p`: master `from` upgrades to `to` by `mode`
    /// ([`World::upgrade_objects`]), with the volleys' hooks around it (the abilities, `+0x930`,
    /// are not held).
    pub(super) fn upgrade_master(
        &mut self,
        p: PlayerId,
        (from, to): (Option<MasterId>, Option<MasterId>),
        mode: i16,
        object: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        self.all_garrison_volleys(p, false)?;
        if let (Some(from), Some(to)) = (from, to) {
            self.upgrade_objects(p, from, to, mode, object)?;
        }
        self.all_garrison_volleys(p, true)
    }

    /// `FUN_141657d10`, the upgrade's own part: of two masters `from` and `to` of player `p`'s
    /// table, mode 2 with the effect's `object` gives that object a master of its own
    /// (`FUN_141186d10`, a copy of the one it reads) where slot 0xa8 lets it
    /// ([`crate::technology::upgrade::upgradable`]), runs its slot 0x98 to `to` keeping its master
    /// ([`World::upgrade_object`]) and copies `to` into its own master (slot 8). Any other mode
    /// goes through the live objects in order: one reading `from` takes its slot 0x98 (its master
    /// kept in modes −1 and 3), or past its use a master of its own and its old death spawn's
    /// copy (`FUN_141201d50`); one reading `from`'s dead unit master (`+0x5e4`, when `to`'s
    /// differs by its copy id) takes a master of its own; in mode 3 one whose own master has
    /// `from`'s id takes slot 0x98 keeping it and copies `to` into it (a building's research
    /// buttons, `FUN_1411c78f0`, are the interface's). Then the sleeping objects reading `from`
    /// take slot 0x98. Upgrades to 0x67f and 0x6a1 move the tracker's list of `from` to `to`'s
    /// (`FUN_14104fd10`, [`crate::players::tracker::Tracker::move_all`]). In modes −1 and 3 two masters of
    /// one type end with `from` taking `to`'s values ([`Master::copy_from`]; the masters' slot
    /// 0xa8, their graphics' loading, is not simulation).
    pub(crate) fn upgrade_objects(
        &mut self,
        p: PlayerId,
        from: MasterId,
        to: MasterId,
        mode: i16,
        object: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        let Some(masters) = self.masters.of(p) else {
            return Ok(());
        };
        let (Some(old), Some(new)) = (masters.get(from), masters.get(to)) else {
            return Ok(());
        };
        // the dead unit master whose objects take masters of their own: `from`'s when `to`'s
        // differs by its copy id
        let dead = old.death_spawns[0]
            .and_then(|id| masters.get(id))
            .filter(|d| {
                new.death_spawns[0]
                    .and_then(|id| masters.get(id))
                    .is_some_and(|e| d.copy_id != e.copy_id)
            });
        let dead = dead.cloned();
        let keep = mode == -1 || mode == 3;
        let (old, new) = (old.clone(), new.clone());
        if mode == 2
            && let Some(id) = object
        {
            self.upgrade_named(p, id, &new)?;
        } else {
            let masters = Masters {
                old: &old,
                new: &new,
                dead: dead.as_ref(),
            };
            self.upgrade_walk(p, (from, mode, keep), masters)?;
        }
        // the upgrades to 0x67f and 0x6a1 move the tracker's list of `from` (FUN_14104fd10)
        self.track_move_upgraded(p, from, to)?;
        if keep
            && old.kind == new.kind
            && let Some(m) = self.masters.checked_mut(p.index())?.get_mut(from)
        {
            m.copy_from(&new);
        }
        Ok(())
    }

    /// Mode 2's own part of [`World::upgrade_objects`]: the object `id` gets a master of its own
    /// (a copy of the one it reads) where its kind lets it, takes slot 0x98 to `new` and copies
    /// `new` into its own master.
    fn upgrade_named(&mut self, p: PlayerId, id: ObjectId, new: &Master) -> Result<(), GenieError> {
        use crate::technology::upgrade::upgradable;
        if let Some(place) = self.place(id) {
            let u = self.object_at(place)?;
            let reads = self.master_read(p, u)?;
            if upgradable(reads.kind, u.life.state) {
                self.object_at_mut(place)?.own_master = Some(Box::new(reads.clone()));
                self.upgrade_object(p, place, (&reads, new), true)?;
                if let Some(own) = self.object_at_mut(place)?.own_master.as_mut() {
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
        p: PlayerId,
        (from, mode, keep): (MasterId, i16, bool),
        Masters { old, new, dead }: Masters<'_>,
    ) -> Result<(), GenieError> {
        use crate::orders::Place;
        use crate::technology::upgrade::upgradable;
        for i in self.players.player(p)?.objects.places() {
            let place = Place::Live(p, i);
            let u = &self.players.live(p, i)?;
            let table = u.own_master.is_none();
            if table && u.life.master_id == from {
                if upgradable(old.kind, u.life.state) {
                    self.upgrade_object(p, place, (old, new), keep)?;
                } else {
                    self.own_and_dead_master(p, i, old)?;
                }
            } else if table && let Some(d) = dead.filter(|d| d.id == u.life.master_id) {
                let d = d.clone();
                self.players.live_mut(p, i)?.own_master = Some(Box::new(d));
            } else if mode == 3
                && let Some(own) = u.own_master.as_deref().filter(|m| m.id == from)
                && upgradable(own.kind, u.life.state)
            {
                let own = own.clone();
                self.upgrade_object(p, place, (&own, new), true)?;
                if let Some(m) = self.players.live_mut(p, i)?.own_master.as_mut() {
                    m.copy_from(new);
                }
            }
        }
        for i in self.players.player(p)?.sleeping.places() {
            let u = &self.players.asleep(p, i)?;
            if u.own_master.is_none() && u.life.master_id == from {
                self.upgrade_object(p, Place::Sleeping(p, i), (old, new), keep)?;
            }
        }
        Ok(())
    }

    /// The upgrades to 0x67f and 0x6a1 move the tracker's list of `from` (`FUN_14104fd10`).
    fn track_move_upgraded(
        &mut self,
        p: PlayerId,
        from: MasterId,
        to: MasterId,
    ) -> Result<(), GenieError> {
        if matches!(to, ids::ID_1663 | ids::ID_1697) {
            self.track_move_all(p, from, to)?;
        }
        Ok(())
    }

    /// The master object `u` of player `p` reads: its own, else its player's.
    pub(super) fn master_read(&self, p: PlayerId, u: &Unit) -> Result<Master, GenieError> {
        u.own_master
            .as_deref()
            .map_or_else(|| u.master(&self.masters, p).cloned(), |m| Ok(m.clone()))
    }

    /// An object of master `old` past its use (slot 0xa8 false) in an upgrade: a master of its own
    /// (`FUN_141186d10`: a copy of `old` unless it has one) and its dead master
    /// (`FUN_141201d50`: unless it has one, a copy of `old`'s death spawn, `+0x5e4`, of type 10 to
    /// 40, 70 or 80, from its player's table; its slot 0xa8, the graphics, is not simulation).
    pub(super) fn own_and_dead_master(
        &mut self,
        p: PlayerId,
        i: ListIndex,
        old: &Master,
    ) -> Result<(), GenieError> {
        let dead = old.death_spawns[0]
            .and_then(|id| player_master(&self.masters, p, id))
            .cloned();
        let u = self.players.live_mut(p, i)?;
        if u.own_master.is_none() {
            u.own_master = Some(Box::new(old.clone()));
        }
        if u.dead_master.is_none() && old.death_spawns[0].is_some() {
            u.dead_master = dead
                .filter(|d| crate::ownership::change::DEAD_COPY_TYPES.contains(&d.kind))
                .map(Box::new);
        }
        Ok(())
    }

    /// Slot 0x1a8 (`FUN_1416136f0`) of player `p`: `count` objects of master `master` (Gaia's when
    /// the player lacks it) at each of its built objects of building master `at` (state 2, in its
    /// list's order), up to attribute 234's count of buildings when above 0, in a playback (the
    /// screen's mode is not 4). Each object: a male villager (0x53) draws its sex from the Player
    /// stream ("SpawnObject": `r % 100` above 49 a female, 0x125); the owner's slot 0x1c0 makes
    /// it at the building's place with its flag ([`World::make_new_object`]); its slot 0x1a0 takes
    /// it into the building where the building has room for one more (slot 0x198,
    /// [`World::has_room`]); unless the player's attribute 283 is above 0 it comes out at once
    /// toward no point (`FUN_1411e7220`, [`World::let_out`]); a relic (0x11d) counts in the
    /// world's `+0x844` ([`World::spawned_relics`]). The local player's statistics of effect
    /// 0x318's spawns are not simulation.
    pub(super) fn spawn_at(
        &mut self,
        p: PlayerId,
        (master, at): (Option<MasterId>, Option<MasterId>),
        count: i16,
    ) -> Result<(), GenieError> {
        let has = |q: usize, id: MasterId| self.masters.get(q).is_some_and(|m| m.get(id).is_some());
        let building = at
            .and_then(|at| player_master(&self.masters, p, at))
            .is_some_and(|m| m.kind == master_type::BUILDING);
        let (Some(master), Some(at)) = (master, at) else {
            return Ok(());
        };
        if !building || !(has(p.index(), master) || has(0, master)) || count <= 0 {
            return Ok(());
        }
        let owner = if has(p.index(), master) {
            p
        } else {
            PlayerId::GAIA
        };
        let attribute = |q: &crate::world::Player, i: usize| {
            q.attributes.get(i).copied().unwrap_or(Amount::ZERO)
        };
        let limit = attribute(self.players.player(p)?, 234).get() as i32;
        let buildings: Vec<ObjectId> = self
            .players
            .player(p)?
            .objects
            .iter()
            .filter(|u| u.life.master_id == at && u.life.state == ObjectState::Alive)
            .map(|u| u.id)
            .collect();
        for (k, &b) in buildings.iter().enumerate() {
            if 0 < limit && limit <= k as i32 {
                return Ok(());
            }
            for _ in 0..count {
                let made = if master == VILLAGER_MALE {
                    let r = self.streams.get(Stream::Player).rand15() as i32;
                    if 49 < r % 100 {
                        // The male and female villagers' masters.
                        // The relic's master, which the world counts.
                        VILLAGER_FEMALE
                    } else {
                        VILLAGER_MALE
                    }
                } else {
                    master
                };
                let Some((_, building)) = self.object(b) else {
                    break;
                };
                let position = building.position();
                let Some(new) = self.make_new_object(owner, made, position, true)? else {
                    continue;
                };
                let Some((bp, bu)) = self.object(b) else {
                    break;
                };
                if self.has_room(bp, bu, None)? {
                    let Some(crate::orders::Place::Live(np, ni)) = self.place(new) else {
                        continue;
                    };
                    self.enter_forced(np, ni, b)?;
                }
                if attribute(self.players.player(p)?, 283) <= Amount::ZERO {
                    let Some(crate::orders::Place::Live(bp, bi)) = self.place(b) else {
                        break;
                    };
                    let _ = self.let_out(bp, bi, new, Point::new(-1.0, -1.0), (None, false))?;
                }
                if master == RELIC {
                    self.spawned_relics = self.spawned_relics.plus_one();
                }
            }
        }
        Ok(())
    }

    /// The objects' kinds (`FUN_140ec2ac0`'s 200 to 206): 200 to 202 change the
    /// objects with masters of their own of master `a` or class `b` (`FUN_141659dd0`; the slice
    /// holds no own masters: none); 203 to 205 the effect's object, which first takes a master of
    /// its own (`FUN_14165abe0`, `FUN_141186d10`, not held); 206 changes the effect's object's
    /// master (`FUN_14165ae40`). Without an object only 200 to 202 run.
    pub(super) const fn object_effect(
        kind: ObjectsKind,
        object: Option<ObjectId>,
    ) -> Result<(), GenieError> {
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

    /// `FUN_1415c1f70` (a multiply of a cost by a negative amount) of player `p`: the cost
    /// attribute (103 to 106) names the resource (food, wood, gold 3, stone 2); a removal
    /// (`how` −2) empties its first cost slot of the masters `units` name in its place
    /// (`FUN_141612df0`: −1, 0, unpaid), an addition (−1) fills the first empty slot with it,
    /// amount −1 and paid (`FUN_14160c450`); a class names every master of it by id. Only a
    /// master from type 70 on has costs. The game scans four slots, the fourth past the three
    /// (not modelled).
    pub(super) fn cost_entries(
        &mut self,
        p: PlayerId,
        units: Option<CostUnits>,
        attribute: MasterAttribute,
        edit: Option<CostEdit>,
    ) {
        let Some(resource) = crate::objects::masters::cost_resource(attribute) else {
            return;
        };
        let (Some(units), Some(edit)) = (units, edit) else {
            return;
        };
        let Some(masters) = self.masters.of_mut(p) else {
            return;
        };
        for m in masters.iter_mut() {
            let named = match units {
                CostUnits::Master(id) => m.id == id,
                CostUnits::Class(class) => m.class == class,
            };
            if !named || m.kind < master_type::COMBAT {
                continue;
            }
            let wanted = match edit {
                CostEdit::Remove => Some(resource),
                CostEdit::Add => None,
            };
            if let Some(slot) = m.cost.iter_mut().find(|c| c.attribute == wanted) {
                *slot = match edit {
                    CostEdit::Remove => CostSlot::default(),
                    CostEdit::Add => CostSlot::new(
                        resource,
                        crate::objects::masters::CostAmount::of_data(-1),
                        true,
                    ),
                };
            }
        }
    }
}
