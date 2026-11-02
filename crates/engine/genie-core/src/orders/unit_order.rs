//! An artifact's and a herd's take, an order's cancel and a unit's order.

use crate::GenieError;
use crate::commands::UnitOrderAction;
use crate::gathering::hunt::HUNT;
use crate::map::Tile;
use crate::math::rng::Stream;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::action_type;
use crate::objects::masters::class;
use crate::objects::masters::ids::TOWN_CENTRE;
use crate::objects::masters::master_type::ACTION;
use crate::objects::masters::master_type::BUILDING;
use crate::orders::object_gone;
use crate::orders::{HERD_MOST, NO_POINT};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::UnitIndex;
use crate::world::World;
use crate::world::index::Place;
use crate::world::slots::PlayerSlots;

impl World {
    /// An animal's Artifact takes a move to the object `target` (its slot 8, `FUN_140aaca30`,
    /// [`crate::ownership::artifact::Artifact::take_object`]): its old target is let go (slot 0x180,
    /// [`World::release`]), then the target, in a state below 7, taken (slot 0x178,
    /// [`World::take`]); livestock (class 58) goes to a building within 1, to the town centre
    /// (master 109) at its point; a `MoveTo` it walked goes, whose teardown stops the unit (slot
    /// 0x5d0).
    pub(crate) fn artifact_take_object(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        target: ObjectId,
    ) -> Result<(), GenieError> {
        let by = self.releaser(player, idx)?;
        let unit = &self.players.live(player, idx)?;
        let livestock = unit.master(self.master_tables(), player)?.class == class::LIVESTOCK;
        let (tp, t) = self.object(target).ok_or(GenieError::unsupported(
            "an Artifact's move to an object the slice does not hold",
        ))?;
        let t_master = t.master(self.master_tables(), tp)?;
        let to_building = livestock && t_master.kind == BUILDING;
        let to_town_centre = livestock && t_master.id == TOWN_CENTRE;
        let at = self
            .spatial
            .get(target)
            .map(|p| p.position)
            .ok_or(GenieError::unsupported(
                "an Artifact's move to an object without a place",
            ))?;
        let unit = &self.players.live(player, idx)?;
        // what its MoveTo's constructor (FUN_140aa0df0) reads of the unit and the target
        let reads = unit.move_to_reads(
            crate::objects::masters::MasterTables::new(&self.masters, &self.parts.own_master),
            player,
            (
                &self.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
        )?;
        let to = reads
            .walk_to(self, Some(target))?
            .ok_or(crate::construction::build::NOT_HELD)?;
        let to = crate::movement::move_to::WalkTo { at, ..to };
        let unit = self.players.live_mut(player, idx)?;
        let (targets, gone) = match unit.actions.list.artifact_mut() {
            Some(a) => a.take_object(to, (to_building, to_town_centre), &reads)?,
            None => Default::default(),
        };
        if let Some(t) = targets.released {
            self.release(t, by)?;
        }
        if let Some(t) = targets.taken {
            self.take(t, by)?;
        }
        // set_state(5)'s sub-actions went: the Enter it held lets its building or transport go
        if let Some(t) = gone.released {
            self.release(t, by)?;
        }
        if gone.stops {
            self.players
                .live_mut(player, idx)?
                .body
                .mover_mut()
                .ok_or(crate::world::WALKER_UNMOVING)?
                .stop()?;
        }
        Ok(())
    }

    /// A hunt's own livestock `sheep` sent to the drop site `site` (`0x140ac60e0` case 6): the
    /// animal's slot 0x138 to the site ([`World::move_to_object`]), then `FUN_140ac4c60` from it
    /// ([`World::herd_from`]).
    ///
    /// # Errors
    ///
    /// What the moves do not model.
    pub(crate) fn lead_herd(
        &mut self,
        p: PlayerId,
        sheep: ObjectId,
        site: crate::gathering::DropSiteId,
    ) -> Result<(), GenieError> {
        self.send_to_site(p, sheep, site)?;
        let mut listed = Vec::new();
        self.herd_from(p, sheep, site, &mut listed)
    }

    /// `FUN_140ac4c60` from the animal `sheep` (`listed` the animals sent so far, a static list of
    /// 16): it joins the list; while it holds fewer than 16, the tiles within 3 of the animal's
    /// (x outer, y inner, held to the map) that player `p` sees (`FUN_141663500` 0xf), each in its
    /// list's order, give the next: livestock (class 58) of the player in state 2 whose top action
    /// has neither a target nor a point ([`Unit::top_action_idle`]) and not listed yet is sent to
    /// the site (slot 0x138) and herds from it in turn; once 16 are listed it ends.
    pub(super) fn herd_from(
        &mut self,
        p: PlayerId,
        sheep: ObjectId,
        site: crate::gathering::DropSiteId,
        listed: &mut Vec<ObjectId>,
    ) -> Result<(), GenieError> {
        listed.push(sheep);
        if HERD_MOST <= listed.len() {
            return Ok(());
        }
        // the animal's own place (+0x3c8, +0x4c8), truncated; send_to_site found it live
        let at = self
            .object(sheep)
            .ok_or_else(|| object_gone(sheep))?
            .1
            .position();
        let (x, y) = (
            crate::math::conversions::cvttss2si(at.x()),
            crate::math::conversions::cvttss2si(at.y()),
        );
        let (w, h) = (self.spatial.map.width(), self.spatial.map.height());
        let (x0, x1) = ((x - 3).max(0), if x + 3 < w { x + 3 } else { w - 1 });
        let (y0, y1) = ((y - 3).max(0), if y + 3 < h { y + 3 } else { h - 1 });
        for tx in x0..=x1 {
            for ty in y0..=y1 {
                if !self.spatial.sees_tile(p, Tile::new(tx, ty)) {
                    continue;
                }
                let ids = self.spatial.tiles().list(Tile::new(tx, ty)).to_vec();
                for id in ids {
                    let Some(Place::Live(q, i)) = self.place(id) else {
                        continue;
                    };
                    let u = &self.players.live(q, i)?;
                    if q != p
                        || u.life.state != ObjectState::Alive
                        || u.master(self.master_tables(), q)?.class != class::LIVESTOCK
                        || !u.top_action_idle()?
                        || listed.contains(&id)
                    {
                        continue;
                    }
                    self.send_to_site(p, id, site)?;
                    self.herd_from(p, id, site, listed)?;
                    if HERD_MOST <= listed.len() {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Slot 0x138 of player `p`'s animal `sheep` to the drop site `site` at its place.
    pub(super) fn send_to_site(
        &mut self,
        p: PlayerId,
        sheep: ObjectId,
        site: crate::gathering::DropSiteId,
    ) -> Result<(), GenieError> {
        let Some(Place::Live(q, idx)) = self.place(sheep) else {
            return Err(GenieError::unsupported(
                "a led animal the slice does not hold live",
            ));
        };
        if q != p {
            return Err(GenieError::unsupported("a led animal of another player"));
        }
        // the hunt's drop site is a raw field yet
        let (site, point) = Some(site)
            .and_then(|id| self.spatial.get(id).map(|s| (id, s.position)))
            .ok_or(GenieError::unsupported("a drop site without a place"))?;
        self.move_to_object(p, idx, site.id(), point)
    }

    /// `UNIT_ORDER`'s action 4 (`FUN_14161e370` case 4) to the objects `ids`: each the world
    /// holds (any player's) that is a building in state 2 cancels the `index`-th unit of its
    /// queue ([`World::cancel_queued`], `all` the command's `+0x1d`), then game mode 16's tail
    /// ([`World::cancel_tail`]).
    pub(crate) fn cancel_order(
        &mut self,
        ids: &[Option<ObjectId>],
        index: UnitIndex,
        all: bool,
    ) -> Result<(), GenieError> {
        for &id in ids.iter().flatten() {
            let Some(place) = self.place(id) else {
                continue;
            };
            let Place::Live(p, idx) = place else {
                return Err(GenieError::unsupported("a cancel at a sleeping object"));
            };
            let unit = &self.players.live(p, idx)?;
            if unit.life.state != ObjectState::Alive
                || unit.master(self.master_tables(), p)?.kind < BUILDING
            {
                continue;
            }
            self.cancel_queued(p, idx, index, all)?;
            self.cancel_tail(p, idx, index)?;
        }
        Ok(())
    }

    /// `UNIT_ORDER`'s order `action` to the units `ids` (`FUN_14161e370`) of `player`'s command,
    /// any player's: the drop-offs 6, 7 and 8 (one case): each unit the world holds in state 2 of
    /// a type above 39 that carries something (`+0x310` above 0). For 7 and 8 a top Hunt (110),
    /// Gather (5) or Build (101) gives it its drop-off (slot 0x78, [`HuntAction::drop_off_asked`],
    /// then `set_state(8)`: [`Unit::drop_off_in`], [`Unit::gather_drop_off_in`],
    /// [`World::build_drop_off_now`]); for 8 one without a target or a point records its work for
    /// the town bell first (`FUN_141206a20`) and queues the way back after (`FUN_141205ac0`). For
    /// 6, and a unit without one of these actions: [`World::drop_off_unasked`] (the way back for
    /// 8 alone).
    ///
    /// [`HuntAction::drop_off_asked`]: crate::gathering::hunt::HuntAction::drop_off_asked
    pub(crate) fn unit_order(
        &mut self,
        _player: PlayerId,
        ids: &[Option<ObjectId>],
        action: UnitOrderAction,
    ) -> Result<(), GenieError> {
        // 8 records an unaimed action's work for the town bell and queues the way back after;
        // 6 asks no gather action (the search alone)
        let (asks, way_back) = match action {
            UnitOrderAction::DropOffSearch => (false, false),
            UnitOrderAction::DropOff => (true, false),
            UnitOrderAction::DropOffAndBack => (true, true),
            _ => {
                return Err(GenieError::unsupported(
                    "a UNIT_ORDER other than the garrison (5) and the drop-offs (6 to 8)",
                ));
            }
        };
        for &id in ids {
            let Some(place) = self.place(id) else {
                continue;
            };
            let Place::Live(p, idx) = place else {
                return Err(GenieError::unsupported("a UNIT_ORDER to a sleeping object"));
            };
            // any player's unit the world holds: the command's list (`FUN_140e6b750`) and the
            // order filter none by owner
            let unit = &self.players.live(p, idx)?;
            let kind = unit.master(self.master_tables(), p)?.kind;
            if unit.life.state != ObjectState::Alive
                || kind < ACTION
                || unit.life.amount <= Amount::ZERO
            {
                continue;
            }
            if unit.actions.unmodelled_action.is_some() {
                return Err(GenieError::unsupported(
                    "a UNIT_ORDER drop-off of an action not held",
                ));
            }
            match self.drop_off_ask(p, idx, (asks, way_back))? {
                Some(ask) => self.asked_drop_off(p, idx, ask)?,
                None => self.drop_off_unasked(p, idx, way_back)?,
            }
        }
        Ok(())
    }
    /// The drop-off order's ask of object `idx` of player `p` (`asks` for the orders 7 and 8):
    /// a top Gather (5), Build (101) or Hunt (110), with whether it asks for the drop-off of the
    /// load (slot 0x78: the task's resource, any for −1, of the load's kind) and whether it is
    /// unaimed (`way_back`, the order 8, of an action without a target or a point); None for
    /// another action, for none and for the order 6.
    pub(super) fn drop_off_ask(
        &self,
        p: PlayerId,
        idx: ListIndex,
        (asks, way_back): (bool, bool),
    ) -> Result<Option<DropOffAsk>, GenieError> {
        let unit = self.players.live(p, idx)?;
        let top = if asks { unit.top_action_kind() } else { None };
        let found = match top {
            Some(crate::gathering::gather::GATHER) => unit
                .actions
                .list
                .gather()
                .map(|g| (DropOffBy::Gather, g.target, g.point, g.task.resource)),
            Some(action_type::BUILD) => unit
                .actions
                .list
                .build()
                .map(|b| (DropOffBy::Build, b.target, b.point, b.task.resource)),
            Some(HUNT) => unit
                .actions
                .list
                .hunt()
                .map(|h| (DropOffBy::Hunt, h.target, h.point, h.task.resource)),
            _ => None,
        };
        let Some((by, target, point, resource)) = found else {
            return Ok(None);
        };
        Ok(Some(DropOffAsk {
            by,
            asked: crate::gathering::hunt::asked_drop_off(
                resource,
                unit.life.attribute_type,
                self.players.player(p)?.attributes.len(),
            ),
            unaimed: way_back && target.is_none() && point.x() == NO_POINT && point.y() == NO_POINT,
        }))
    }

    /// The drop-off order to an action that `ask`s for it: a load of another resource takes
    /// notify 0x21c ([`World::other_load`]); an unaimed action's work goes into the town-bell
    /// records first (`FUN_141206a20`, [`World::record_town_bell`]); an asked drop-off goes to
    /// the drop site (`set_state(8)`: [`World::drop_off_at_site`], [`World::build_drop_off_now`]);
    /// the way back to the unaimed work is queued after (`FUN_141205ac0`,
    /// [`World::queue_way_back`]).
    pub(super) fn asked_drop_off(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        ask: DropOffAsk,
    ) -> Result<(), GenieError> {
        if !ask.asked {
            self.other_load(p, idx)?;
        }
        if ask.unaimed {
            self.record_town_bell(p, idx)?;
        }
        if ask.asked {
            match ask.by {
                DropOffBy::Build => self.build_drop_off_now(p, idx)?,
                DropOffBy::Gather | DropOffBy::Hunt => self.drop_off_at_site(p, idx, ask.by)?,
            }
        }
        if ask.unaimed {
            self.queue_way_back(p, idx)?;
        }
        Ok(())
    }

    /// A Gather's or a Hunt's drop-off at its drop site (`set_state(8)`:
    /// [`Unit::gather_drop_off_in`], [`Unit::drop_off_in`]): the site found is taken (slot
    /// 0x178, [`World::take`]).
    pub(super) fn drop_off_at_site(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        by: DropOffBy,
    ) -> Result<(), GenieError> {
        let mut unit = self.take_out(p, idx)?;
        let player = &self.players.player(p)?;
        let world = crate::unit_ai::world_view::DropWorld {
            masters: crate::objects::masters::OwnerMasters::new(
                &self.parts.own_master,
                self.masters.of(p),
            ),
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            pathing: &mut self.pathing,
            object: self.streams.get(Stream::Object),
            world_time: self.world_time,
            computer: player.computer,
            groups: &self.groups,
        };
        let others = crate::world::others::OthersRef::of(&self.players, &self.index, p)?;
        let result = if by == DropOffBy::Hunt {
            unit.drop_off_in(
                world,
                others,
                &self.graphics,
                crate::world::index::ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.masters,
                    &self.parts,
                ),
            )
        } else {
            unit.gather_drop_off_in(
                world,
                others,
                &self.graphics,
                crate::world::index::ObjectsRef::new(
                    &self.players,
                    &self.index,
                    &self.masters,
                    &self.parts,
                ),
            )
        };
        let idx = self.put_back_own(p, unit)?;
        if let Some(site) = result? {
            self.take(site, self.releaser(p, idx)?)?;
        }
        Ok(())
    }
}

/// The top action a drop-off order asks ([`World::drop_off_ask`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropOffBy {
    /// A Gather (5).
    Gather,
    /// A Build (101).
    Build,
    /// A Hunt (110).
    Hunt,
}

/// A drop-off order's ask of a unit's top action ([`World::drop_off_ask`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DropOffAsk {
    /// The action asked.
    pub(super) by: DropOffBy,
    /// The action asks for the drop-off of the load (slot 0x78).
    pub(super) asked: bool,
    /// The order 8 to an action without a target or a point.
    pub(super) unaimed: bool,
}
