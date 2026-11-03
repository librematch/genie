//! The mule cart: the resources a finished cart searches (its completion, `FUN_1411c6bd0`'s
//! tail) and its unit AI's own search (slot 0x1e8), then the work command toward the one it found.

use crate::GenieError;
use crate::construction::build::CART_TASK;
use crate::gathering::DropSiteId;
use crate::gathering::closest_resource::{ResourceWorld, closest_resource_of};
use crate::map::DistanceSquared;
use crate::map::Position;
use crate::math::image::turn::CART_REACH as TURN_CART_REACH;
use crate::math::rng::Stream;
use crate::movement::{Planning, footprint_gap_squared};
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::WorkerCount;
use crate::objects::masters::GatherGroup;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::{MasterTables, OwnerMasters};
use crate::orders::Work;
use crate::players::PlayerId;
use crate::unit_ai::NO_POINT;
use crate::unit_ai::world_view::Foreign;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::layer::ClassRoles;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;
use std::mem::replace;

impl World {
    /// A mule cart's completion (`FUN_1411c6bd0`'s tail, a building whose master has a task of
    /// action type 0x99, [`crate::construction::build::Built::cart`]) with a unit AI: with no current target a
    /// human player's runs its AI's slot 0x1e8 ([`World::cart_work_search`]; the game mode's
    /// test, `FUN_140c87f20` not 4, holds in every recording probed), all of it inside the
    /// completion's `set_state(state)`; then a cart with a target moves to it (slot 0x138 at the
    /// target's place).
    pub(crate) fn cart_search(
        &mut self,
        p: PlayerId,
        id: ObjectId,
        state: ObjectState,
    ) -> Result<(), GenieError> {
        let _ = self.players.player(p)?;
        let Some(idx) = self.live_index(p, id) else {
            return Ok(());
        };
        let Some(current) = self
            .parts
            .unit_ai
            .get(self.players.live(p, idx)?.id)
            .map(|ai| ai.current_target)
        else {
            return Ok(());
        };
        let found = if current.is_none() && !self.players.player(p)?.kind.is_computer() {
            Some(self.cart_find(p, idx)?)
        } else {
            None
        };
        // all of it runs inside the completion's set_state(2), before the cart is in state 2
        let built = replace(&mut self.players.live_mut(p, idx)?.life.state, state);
        let result = (|| {
            if let Some(found) = found {
                self.cart_work(p, idx, found)?;
            }
            // with a target (the one the work command gave the unit AI) the cart moves to it
            // (slot 0x138 at its place)
            let target = self
                .parts
                .unit_ai
                .get(self.players.live(p, idx)?.id)
                .and_then(|ai| ai.current_target);
            if let Some(target) = target
                && let Some(at) = self.spatial.get(target).map(|o| o.position)
            {
                self.move_to_object(p, idx, target, at)?;
            }
            Ok::<_, GenieError>(())
        })();
        self.players.live_mut(p, idx)?.life.state = built;
        result
    }

    /// The mule cart's own search for resources, its unit AI's slot 0x1e8 (`FUN_140c16240`) for
    /// the cart at `idx` of player `p`: [`World::cart_find`], then [`World::cart_work`]. The
    /// completion runs it ([`World::cart_search`]), and so does the Empire Wars start
    /// ([`crate::orders::start_work`]).
    pub(crate) fn cart_work_search(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        let found = self.cart_find(p, idx)?;
        self.cart_work(p, idx, found)
    }

    /// Slot 0x1e8's search (`FUN_140c16240`, its world test `FUN_141533b60` held): per task of
    /// the cart's master in order, the target class of the last of type 0x99 so far names a
    /// gather group (`FUN_140fdd5c0` with 1: 1 for 7, 3 for 8, 0 for 15, 4 for 32 and 63), each
    /// searched by `ClosestResourceObject` (slot 0x80) from the cart as the drop site, any
    /// workers, from state 0 on; of those found, the nearest by the squared footprint gap
    /// (`FUN_14118f8c0`, below 99999, `DAT_142ef9008`), with its place; none at the cart's own.
    pub(crate) fn cart_find(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<(Option<ObjectId>, Position), GenieError> {
        const CART_REACH: DistanceSquared = DistanceSquared::new(TURN_CART_REACH);
        let id = self.players.live(p, idx)?.id;
        let current = self
            .parts
            .unit_ai
            .get(self.players.live(p, idx)?.id)
            .and_then(|ai| ai.current_target);
        if self.parts.mover_of(self.players.live(p, idx)?.id).is_none() {
            return Err(GenieError::unsupported(
                "a mule cart's search without its movement state (a moving building's setup)",
            ));
        }
        let master = self.players.live(p, idx)?.master(self.master_tables(), p)?;
        let mut kinds = Vec::new();
        let mut cart_class = None;
        for t in &master.tasks {
            if t.action_kind == CART_TASK {
                cart_class = t.object_group;
            }
            let kind = {
                let held = self.model.roles.of_some_class(cart_class);
                if held.contains(ClassRoles::FORAGE_BUSH) {
                    GatherGroup::FORAGE
                } else if held.contains(ClassRoles::STONE_MINE) {
                    GatherGroup::STONE
                } else if held.contains(ClassRoles::TREE) {
                    GatherGroup::WOOD
                } else if held.intersects(ClassRoles::GOLD_MINE | ClassRoles::GOLD_FISH) {
                    GatherGroup::GOLD
                } else {
                    continue;
                }
            };
            kinds.push(kind);
        }
        let cart = self.take_out(p, idx)?;
        let result = (|| {
            let (before, rest) = self.players.split_at(p.index());
            let (player, after) = rest.split_first().ok_or(GenieError::unreachable(
                "a cart's player outside the players",
            ))?;
            let mut w = ResourceWorld {
                player: p,
                spatial: &mut self.spatial,
                zones: &mut self.zones,
                foreign: Foreign {
                    before,
                    after,
                    left: None,
                },
                every: MasterTables::new(&self.model.masters, &self.parts.own_master),
                masters: OwnerMasters::new(&self.parts.own_master, self.model.masters.of(p)),
                plan: Planning {
                    pathing: &mut self.pathing,
                    now: self.world_time,
                    player_type: player.kind,
                    object: self.streams.get(Stream::Object),
                    groups: &self.groups,
                },
            };
            let others = OthersRef::new(p, &player.objects, w.foreign, &self.index);
            let mut found = Vec::new();
            for kind in kinds {
                let (f, _) = closest_resource_of(
                    (
                        &cart,
                        self.parts.actions_of(cart.id),
                        self.parts.mover_of(cart.id),
                    ),
                    &mut w,
                    others,
                    (kind, WorkerCount::NO_CAP, Some(DropSiteId::new(id))),
                    (current, ObjectState::Foundation, NO_POINT),
                    ObjectsRef::new(
                        &self.players,
                        &self.index,
                        &self.model.masters,
                        &self.parts,
                        &self.model.roles,
                        &self.model.tuning,
                    ),
                    &self.model.roles,
                )?;
                if let Some(f) = f {
                    found.push(f);
                }
            }
            Ok::<_, GenieError>(found)
        })();
        let idx = self.put_back_own(p, cart)?;
        let found = result?;
        let cart_master = self.players.live(p, idx)?.master(self.master_tables(), p)?;
        let at = self.players.live(p, idx)?.position();
        let mut best: Option<(ObjectId, Position)> = None;
        let mut best_d = CART_REACH;
        for f in found {
            let Some(o) = self.spatial.get(f) else {
                continue;
            };
            let d = footprint_gap_squared(at, cart_master.radius, o.position, o.radius);
            if d < best_d {
                best = Some((f, o.position));
                best_d = d;
            }
        }
        Ok(best.map_or((None, at), |(f, at)| (Some(f), at)))
    }

    /// Slot 0x1e8's end: slot 0x140 (`FUN_141070c90`), the work command toward the found
    /// target at its place, else at the cart's own (slot 0x7e8: no carried-resource test, any
    /// task; `+0x888`'s count is not held).
    pub(crate) fn cart_work(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        (target, point): (Option<ObjectId>, Position),
    ) -> Result<(), GenieError> {
        let work = Work {
            target,
            point,
            carry_test: false,
            filter: None,
            notified: true,
        };
        self.order_work(p, idx, &work)
    }
}
