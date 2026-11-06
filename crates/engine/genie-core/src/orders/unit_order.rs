//! An artifact's and a herd's take, an order's cancel and a unit's order.

use crate::commands::UnitOrderAction;
use crate::construction::FoundationId;
use crate::construction::build::BuildAction;
use crate::construction::build::NOT_HELD;
use crate::gathering::drop_sites::DROP_SITE_UNPLACED;
use crate::gathering::gather::{GATHER, GatherAction};
use crate::gathering::hunt::asked_drop_off as hunt_asked_drop_off;
use crate::gathering::hunt::{HUNT, HuntAction};
use crate::gathering::{DropSiteId, GatherTargetId, HuntTargetId};
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::rng::Stream;
use crate::movement::move_to::WalkTo;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::teardown::TornWalk;
use crate::orders::object_gone;
use crate::orders::{HERD_MOST, NO_POINT};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::production::Batch;
use crate::production::UnitIndex;
use crate::unit_ai::world_view::DropWorld;
use crate::world::WALKER_UNMOVING;
use crate::world::World;
use crate::world::index::Place;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

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
    ) -> GenieResult<()> {
        let by = self.releaser(player, idx)?;
        let unit = &self.players.live(player, idx)?;
        let livestock = self
            .model
            .roles
            .of_class(unit.master(self.master_tables(), player)?.class)
            .contains(ClassRoles::LIVESTOCK);
        let (tp, target_unit) = self.object(target).ok_or(GenieError::unsupported(
            "an Artifact's move to an object the slice does not hold",
        ))?;
        let target_master = target_unit.master(self.master_tables(), tp)?;
        let to_building = livestock && target_master.kind == MasterKind::BUILDING;
        let to_town_centre = livestock
            && self
                .model
                .roles
                .of_master(target_master.id)
                .contains(MasterRoles::TOWN_CENTRE);
        let at = self
            .spatial
            .get(target)
            .map(|placed| placed.position)
            .ok_or(GenieError::unsupported(
                "an Artifact's move to an object without a place",
            ))?;
        let unit = &self.players.live(player, idx)?;
        // what its MoveTo's constructor (FUN_140aa0df0) reads of the unit and the target
        let reads = unit.move_to_reads(
            self.top_speed(player, unit)?,
            MasterTables::new(&self.model.masters, &self.parts.own_master),
            player,
            (
                &self.model.graphics,
                self.parts.combat_of(unit.id).abilities.look.as_ref(),
            ),
            &self.model.roles,
        )?;
        let to = reads.walk_to(self, Some(target))?.ok_or(NOT_HELD)?;
        let to = WalkTo { at, ..to };
        let unit = self.players.live_mut(player, idx)?;
        let (targets, gone) = match self.parts.actions_mut(unit.id)?.list.artifact_mut() {
            Some(artifact) => artifact.take_object(
                to,
                (to_building, to_town_centre),
                &reads,
                self.model.tuning.artifact,
                &self.model.tuning.move_to,
            )?,
            None => Default::default(),
        };
        if let Some(object_id) = targets.released {
            self.release(object_id, by)?;
        }
        if let Some(object_id) = targets.taken {
            self.take(object_id, by)?;
        }
        // set_state(5)'s sub-actions went: the Enter it held lets its building or transport go
        if let Some(object_id) = gone.released {
            self.release(object_id, by)?;
        }
        if gone.walk == TornWalk::Stops {
            self.parts
                .mover_mut(self.players.live(player, idx)?.id)
                .ok_or(WALKER_UNMOVING)?
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
        player_id: PlayerId,
        sheep: ObjectId,
        site: DropSiteId,
    ) -> GenieResult<()> {
        self.send_to_site(player_id, sheep, site)?;
        let mut listed = Vec::new();
        self.herd_from(player_id, sheep, site, &mut listed)
    }

    /// `FUN_140ac4c60` from the animal `sheep` (`listed` the animals sent so far, a static list of
    /// 16): it joins the list; while it holds fewer than 16, the tiles within 3 of the animal's (x
    /// outer, y inner, held to the map) that player `owner` sees (`FUN_141663500` 0xf), each in its
    /// list's order, give the next: livestock (class 58) of the player in state 2 whose top action
    /// has neither a target nor a point
    /// ([`Action::top_action_idle`](crate::objects::layers::Action::top_action_idle)) and not
    /// listed yet is sent to the site (slot 0x138) and herds from it in turn; once 16 are listed it
    /// ends.
    pub(super) fn herd_from(
        &mut self,
        owner: PlayerId,
        sheep: ObjectId,
        site: DropSiteId,
        listed: &mut Vec<ObjectId>,
    ) -> GenieResult<()> {
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
        let (x, y) = (cvttss2si(at.x()), cvttss2si(at.y()));
        let (width, height) = (self.spatial.map.width(), self.spatial.map.height());
        let (x0, x1) = (
            (x - 3).max(0),
            if x + 3 < width { x + 3 } else { width - 1 },
        );
        let (y0, y1) = (
            (y - 3).max(0),
            if y + 3 < height { y + 3 } else { height - 1 },
        );
        for tx in x0..=x1 {
            for ty in y0..=y1 {
                if !self.spatial.sees_tile(owner, Tile::new(tx, ty)) {
                    continue;
                }
                let ids = self.spatial.tiles().list(Tile::new(tx, ty)).to_vec();
                for id in ids {
                    let Some(Place::Live(player_id, list_index)) = self.place(id) else {
                        continue;
                    };
                    let unit = &self.players.live(player_id, list_index)?;
                    if player_id != owner
                        || unit.life.state != ObjectState::Alive
                        || !self
                            .model
                            .roles
                            .of_class(unit.master(self.master_tables(), player_id)?.class)
                            .contains(ClassRoles::LIVESTOCK)
                        || !self
                            .parts
                            .actions_of(unit.id)
                            .top_action_idle(self.parts.unread.get(unit.id))?
                        || listed.contains(&id)
                    {
                        continue;
                    }
                    self.send_to_site(owner, id, site)?;
                    self.herd_from(owner, id, site, listed)?;
                    if HERD_MOST <= listed.len() {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Slot 0x138 of player `player`'s animal `sheep` to the drop site `site` at its place.
    pub(super) fn send_to_site(
        &mut self,
        player: PlayerId,
        sheep: ObjectId,
        site: DropSiteId,
    ) -> GenieResult<()> {
        let Some(Place::Live(player_id, idx)) = self.place(sheep) else {
            return Err(GenieError::unsupported(
                "a led animal the slice does not hold live",
            ));
        };
        if player_id != player {
            return Err(GenieError::unsupported("a led animal of another player"));
        }
        // the hunt's drop site is a raw field yet
        let (site, point) = Some(site)
            .and_then(|id| self.spatial.get(id).map(|placed| (id, placed.position)))
            .ok_or(DROP_SITE_UNPLACED)?;
        self.move_to_object(player, idx, site.id(), point).map(drop)
    }

    /// `UNIT_ORDER`'s action 4 (`FUN_14161e370` case 4) to the objects `ids`: each the world
    /// holds (any player's) that is a building in state 2 cancels the `index`-th unit of its
    /// queue ([`World::cancel_queued`], `batch` the command's `+0x1d`), then game mode 16's tail
    /// ([`World::cancel_tail`]).
    pub(crate) fn cancel_order(
        &mut self,
        ids: &[Option<ObjectId>],
        index: UnitIndex,
        batch: Batch,
    ) -> GenieResult<()> {
        for &id in ids.iter().flatten() {
            let Some(place) = self.place(id) else {
                continue;
            };
            let Place::Live(player_id, idx) = place else {
                return Err(GenieError::unsupported("a cancel at a sleeping object"));
            };
            let unit = &self.players.live(player_id, idx)?;
            if unit.life.state != ObjectState::Alive
                || unit.master(self.master_tables(), player_id)?.kind < MasterKind::BUILDING
            {
                continue;
            }
            self.cancel_queued(player_id, idx, index, batch)?;
            self.cancel_tail(player_id, idx, index)?;
        }
        Ok(())
    }

    /// `UNIT_ORDER`'s order `action` to the units `ids` (`FUN_14161e370`) of `player`'s command,
    /// any player's: the drop-offs 6, 7 and 8 (one case): each unit the world holds in state 2 of a
    /// type above 39 that carries something (`+0x310` above 0). For 7 and 8 a top Hunt (110),
    /// Gather (5) or Build (101) gives it its drop-off (slot 0x78, [`HuntAction::drop_off_asked`],
    /// then `set_state(8)`: [`Unit::drop_off_in`](crate::objects::unit::Unit::drop_off_in),
    /// [`Unit::gather_drop_off_in`](crate::objects::unit::Unit::gather_drop_off_in),
    /// [`World::build_drop_off_now`]); for 8 one without a target or a point records its work for
    /// the town bell first (`FUN_141206a20`) and queues the way back after (`FUN_141205ac0`). For
    /// 6, and a unit without one of these actions: [`World::drop_off_unasked`] (the way back for 8
    /// alone).
    ///
    /// [`HuntAction::drop_off_asked`]: crate::gathering::hunt::HuntAction::drop_off_asked
    pub(crate) fn unit_order(
        &mut self,
        _player: PlayerId,
        ids: &[Option<ObjectId>],
        action: UnitOrderAction,
    ) -> GenieResult<()> {
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
            let Place::Live(player_id, idx) = place else {
                return Err(GenieError::unsupported("a UNIT_ORDER to a sleeping object"));
            };
            // any player's unit the world holds: the command's list (`FUN_140e6b750`) and the
            // order filter none by owner
            let unit = &self.players.live(player_id, idx)?;
            let kind = unit.master(self.master_tables(), player_id)?.kind;
            if unit.life.state != ObjectState::Alive
                || kind < MasterKind::ACTION
                || unit.life.amount <= Amount::ZERO
            {
                continue;
            }
            if self.parts.unread.contains(unit.id) {
                return Err(GenieError::unsupported(
                    "a UNIT_ORDER drop-off of an action not held",
                ));
            }
            match self.drop_off_ask(player_id, idx, (asks, way_back))? {
                Some(ask) => self.asked_drop_off(player_id, idx, &ask)?,
                None => self.drop_off_unasked(player_id, idx, action)?,
            }
        }
        Ok(())
    }
    /// The drop-off order's ask of object `idx` of player `player_id` (`asks` for the orders 7 and
    /// 8): a top Gather (5), Build (101) or Hunt (110), with whether it asks for the drop-off of
    /// the load (slot 0x78: the task's resource, any for −1, of the load's kind) and whether it is
    /// unaimed (`way_back`, the order 8, of an action without a target or a point); None for
    /// another action, for none and for the order 6.
    pub(super) fn drop_off_ask(
        &self,
        player_id: PlayerId,
        idx: ListIndex,
        (asks, way_back): (bool, bool),
    ) -> GenieResult<Option<DropOffAsk>> {
        let unit = self.players.live(player_id, idx)?;
        let top = if asks {
            self.parts.actions_of(unit.id).top_action_kind()
        } else {
            None
        };
        let found = match top {
            Some(GATHER) => self
                .parts
                .actions_of(unit.id)
                .list
                .gather()
                .map(|gather_action| {
                    (
                        DropOffBy::Gather(*gather_action),
                        gather_action.target.map(GatherTargetId::id),
                        gather_action.point,
                        gather_action.task.resource,
                    )
                }),
            Some(ActionKind::BUILD) => {
                self.parts
                    .actions_of(unit.id)
                    .list
                    .build()
                    .map(|build_action| {
                        (
                            DropOffBy::Build(*build_action),
                            build_action.target.map(FoundationId::id),
                            build_action.point,
                            build_action.task.resource,
                        )
                    })
            }
            Some(HUNT) => self
                .parts
                .actions_of(unit.id)
                .list
                .hunt()
                .map(|hunt_action| {
                    (
                        DropOffBy::Hunt(*hunt_action),
                        hunt_action.target.map(HuntTargetId::id),
                        hunt_action.point,
                        hunt_action.task.resource,
                    )
                }),
            _ => None,
        };
        let Some((by, target, point, resource)) = found else {
            return Ok(None);
        };
        Ok(Some(DropOffAsk {
            by,
            asked: hunt_asked_drop_off(
                resource,
                unit.life.attribute_kind,
                self.players.player(player_id)?.attributes.len(),
                self.model.slots,
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
        player_id: PlayerId,
        idx: ListIndex,
        ask: &DropOffAsk,
    ) -> GenieResult<()> {
        if !ask.asked {
            self.other_load(player_id, idx)?;
        }
        if ask.unaimed {
            self.record_town_bell(player_id, idx)?;
        }
        if ask.asked {
            match ask.by {
                DropOffBy::Build(build) => self.build_drop_off_now((player_id, idx), build)?,
                DropOffBy::Gather(gather) => {
                    self.drop_off_at_site(player_id, idx, &SiteWalker::Gather(gather))?;
                }
                DropOffBy::Hunt(hunt) => {
                    self.drop_off_at_site(player_id, idx, &SiteWalker::Hunt(hunt))?;
                }
            }
        }
        if ask.unaimed {
            self.queue_way_back(player_id, idx)?;
        }
        Ok(())
    }

    /// A Gather's or a Hunt's drop-off at its drop site (`set_state(8)`:
    /// [`Unit::gather_drop_off_in`](crate::objects::unit::Unit::gather_drop_off_in),
    /// [`Unit::drop_off_in`](crate::objects::unit::Unit::drop_off_in)): the site found is taken
    /// (slot 0x178, [`World::take`]).
    pub(super) fn drop_off_at_site(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        walker: &SiteWalker,
    ) -> GenieResult<()> {
        let top_speed = {
            let unit = self.players.live(player_id, idx)?;
            self.top_speed_terms(player_id, unit)?
        };
        let unit = self.take_out(player_id, idx)?;
        // its action layer goes out of the table with it, as the updating object's
        let mut actions = self.parts.take_actions(unit.id);
        let player = &self.players.player(player_id)?;
        let (mover, by_id) = self
            .parts
            .lend_mover(unit.id, (&self.players, &self.index, &self.model));
        let world = DropWorld {
            top_speed,
            masters: by_id.masters().of(player_id),
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            pathing: &mut self.pathing,
            object: self.streams.get_mut(Stream::Object),
            world_time: self.world_time,
            player_kind: player.kind,
            groups: &self.groups,
        };
        let others = OthersRef::of(&self.players, &self.index, player_id)?;
        let result = match *walker {
            SiteWalker::Hunt(mut hunt) => unit.drop_off_in(
                (&mut hunt, &mut actions),
                mover,
                world,
                others,
                &self.model.graphics,
                by_id,
            ),
            SiteWalker::Gather(mut gather) => unit.gather_drop_off_in(
                (&mut gather, &mut actions),
                mover,
                world,
                others,
                &self.model.graphics,
                by_id,
            ),
        };
        self.parts.put_actions(unit.id, actions);
        let idx = self.put_back_own(player_id, unit)?;
        if let Some(site) = result? {
            self.take(site, self.releaser(player_id, idx)?)?;
        }
        Ok(())
    }
}

/// The top action a drop-off order asks ([`World::drop_off_ask`]), the one the ask found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum DropOffBy {
    /// A Gather (5).
    Gather(GatherAction),
    /// A Build (101).
    Build(BuildAction),
    /// A Hunt (110).
    Hunt(HuntAction),
}

/// The top action a drop-off order walks to a drop site ([`World::drop_off_at_site`]), the
/// one its ask found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum SiteWalker {
    /// A Gather (5).
    Gather(GatherAction),
    /// A Hunt (110).
    Hunt(HuntAction),
}

/// A drop-off order's ask of a unit's top action ([`World::drop_off_ask`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DropOffAsk {
    /// The action asked.
    pub(super) by: DropOffBy,
    /// The action asks for the drop-off of the load (slot 0x78).
    pub(super) asked: bool,
    /// The order 8 to an action without a target or a point.
    pub(super) unaimed: bool,
}
