//! The drop-offs: an unasked one, the ordered drop site, the table's, one now and elsewhere,
//! and the way back.

use crate::commands::UnitOrderAction;
use crate::construction::build::BuildAction;
use crate::gathering::DropSiteId;
use crate::gathering::drop_sites::DropMeasure;
use crate::gathering::drop_sites::SitePlace;
use crate::gathering::drop_sites::SiteSearch;
use crate::gathering::drop_sites::drop_site_search;
use crate::gathering::gather::GATHER;
use crate::gathering::hunt::HUNT;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::objects::ActionState;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterKind;
use crate::objects::masters::OwnerMasters;
use crate::objects::masters::PlayersMastersExt;
use crate::orders::Dispatch;
use crate::orders::MasterSwap;
use crate::orders::OrderSource;
use crate::orders::object_gone;
use crate::orders::{NO_POINT, Work, nearest_table_site};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTargetKind;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::world_view::DropWorld;
use crate::unit_ai::{Notify, NotifyKind, OrderPriority, QueuedOrder};
use crate::world::World;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::objects_ref::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

impl World {
    /// `UNIT_ORDER`'s drop-off of a carrying unit without a gather action on top (`FUN_14161e370`'s
    /// case 8, past the Gather, Hunt and Build): a master with drop sites (`FUN_14109e210`: one not
    /// below 0) searches them from the unit ([`World::ordered_drop_site`]); with none found, or
    /// without drop sites, the drop-site table's nearest building for the resource it holds from
    /// its class ([`World::table_drop_site`]); with none, nothing. A unit (type 70 and up) then
    /// works on it (slot 0x7e8 at (−1, −1, −1): the retargeting byte 0, the task filter 110 for a
    /// load of meat, wood or food, kinds 15, 1 and 0, else 5; [`World::order_work`]) and, for the
    /// drop-off 8 ([`UnitOrderAction::DropOffAndBack`]), queues its way back to work
    /// ([`World::queue_way_back`]).
    pub(super) fn drop_off_unasked(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
        action: UnitOrderAction,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player_id, idx)?;
        let master = unit.master(self.master_tables(), player_id)?;
        let (kind, class, at) = (master.kind, master.class, unit.position().ground());
        let held = unit.life.attribute_kind;
        let mut site = None;
        if !master.action_part().drop_sites.is_empty() {
            site = self.ordered_drop_site(player_id, idx)?;
        }
        if site.is_none() {
            site = self.table_drop_site(player_id, at, held, class)?;
        }
        let Some(site) = site else {
            return Ok(());
        };
        if kind < MasterKind::COMBAT {
            return Ok(());
        }
        let filter = if held.is_some_and(|attribute_id| {
            self.model
                .roles
                .of_attribute(attribute_id)
                .contains(AttributeRoles::HUNTED_LOAD)
        }) {
            HUNT
        } else {
            GATHER
        };
        let work = Work {
            target: Some(site.id()),
            point: Position::from([NO_POINT; 3]),
            disable_retargeting: false,
            filter: Some(filter),
        };
        self.order_work(player_id, idx, &work)?;
        if action == UnitOrderAction::DropOffAndBack {
            self.queue_way_back(player_id, idx)?;
        }
        Ok(())
    }

    /// `FUN_1411671e0` as `UNIT_ORDER`'s drop-off calls it for the unit at (`player_id`, `idx`):
    /// over its master's drop sites, of its player's live objects in state 2, inside others too, at
    /// work or not, by the squared distance (not by path); the unit the seeker (its zone, its
    /// planner) where its slot 0x2c0 answers 1: any unit below type 70, else one whose master drops
    /// at neither 45 nor 2172 and of a class other than 21 and 2 (`FUN_141226580`).
    pub(super) fn ordered_drop_site(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<Option<DropSiteId>> {
        let by_id = ObjectsRef::new(
            &self.players,
            &self.index,
            &self.model.masters,
            &self.parts,
            &self.model.roles,
            &self.model.tuning,
        );
        let unit = &self.players.live(player_id, idx)?;
        let master = unit.master(self.master_tables(), player_id)?;
        let seeks = master.kind < MasterKind::COMBAT
            || (!master.action_part().drop_sites.iter().any(|&master_id| {
                self.model
                    .roles
                    .of_master(master_id)
                    .contains(MasterRoles::DOCK)
                    || self
                        .model
                        .roles
                        .of_master(master_id)
                        .contains(MasterRoles::PORT)
            }) && !self
                .model
                .roles
                .of_class(master.class)
                .intersects(ClassRoles::FISHING_SHIP | ClassRoles::TRADE_BOAT));
        let masters = self
            .model
            .masters
            .of(player_id)
            .ok_or(GenieError::unsupported(
                "a drop-off of a player without masters",
            ))?;
        if !seeks {
            return drop_site_search(
                &self.players.player(player_id)?.objects,
                OwnerMasters::new(&self.parts.own_master, Some(masters)),
                &master.action_part().drop_sites,
                unit.position(),
                None,
                (SitePlace::Anywhere, None),
                by_id,
            );
        }
        let state = self
            .parts
            .actions_of(unit.id)
            .top_action()
            .map_or(ActionState::NO_ACTION, |top_action| top_action.state);
        let player = &self.players.player(player_id)?;
        let mut world = DropWorld {
            masters: OwnerMasters::new(&self.parts.own_master, self.model.masters.of(player_id)),
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            pathing: &mut self.pathing,
            object: self.streams.get_mut(Stream::Object),
            world_time: self.world_time,
            player_kind: player.kind,
            groups: &self.groups,
        };
        let others = OthersRef::of(&self.players, &self.index, player_id)?;
        player.live(idx)?.drop_site_with(
            (self.parts.actions_of(unit.id), self.parts.mover_of(unit.id)),
            &mut world,
            others,
            state,
            SiteSearch {
                place: SitePlace::Anywhere,
                nearest: DropMeasure::ByDistance,
                at_work: None,
            },
            by_id,
        )
    }

    /// `FUN_141658e20` for player `player_id` at `at`: of the buildings the drop-site table gives
    /// for the resource `held` from workers of `class`
    /// ([`crate::unit_ai::task_finder::table_drop_buildings`] for the player's civ), the objects of
    /// each master's done list in the player's tracker in turn (`FUN_14104f600`, the master itself
    /// the key) that the world holds in state 2, the nearest by the squared distance of the centres
    /// (the first of equals; none from [`OrderTuning::table_drop_far`] on).
    ///
    /// [`OrderTuning::table_drop_far`]: crate::world::layer::generated::OrderTuning::table_drop_far
    pub(crate) fn table_drop_site(
        &self,
        player_id: PlayerId,
        at: Point,
        held: Option<AttributeId>,
        class: ClassId,
    ) -> GenieResult<Option<DropSiteId>> {
        let player = self.players.player(player_id)?;
        let look = |id: ObjectId| {
            self.object(id)
                .map(|(_, unit)| (unit.life.state, unit.position()))
        };
        Ok(nearest_table_site(
            (&self.model.drop_sites, player.civ, &player.tracker),
            (at, held, class),
            look,
            &self.model.tuning.order,
        )
        .map(|(id, _)| id))
    }

    /// The Build's `set_state(8)` as `UNIT_ORDER`'s drop-off (slot 0x78) gives it to the builder at
    /// (`player_id`, `idx`), on the builder's step ([`World::run_on_step`]): a builder that carries
    /// something (`+0x310` above 0, COMISS: not a NaN) searches the drop-site table
    /// ([`World::table_drop_site`] for what it carries and its class) and its slot 0x820 the task
    /// on the site found (any action type, no switch,
    /// [`World::find_work_task`]: its range, else the Build task's); then
    /// [`crate::construction::build::BuildAction::set_drop`]: a counted builder leaves its
    /// foundation's builders, the second targets go and come at once, notify 0x203 goes to the
    /// unit AI. `build` is the Build on top that the order's ask found ([`World::drop_off_ask`]):
    /// nothing between the ask and the step changes it.
    pub(crate) fn build_drop_off_now(
        &mut self,
        (player_id, idx): (PlayerId, ListIndex),
        mut build: BuildAction,
    ) -> GenieResult<()> {
        use crate::construction::build::DropSearch;
        let by = self.releaser(player_id, idx)?;
        let unit = &self.players.live(player_id, idx)?;
        let task_range = build.task.range;
        let (amount, held, at) = (
            unit.life.amount,
            unit.life.attribute_kind,
            unit.position().ground(),
        );
        let class = unit.master(self.master_tables(), player_id)?.class;
        let search = if Amount::ZERO < amount {
            match self.table_drop_site(player_id, at, held, class)? {
                None => DropSearch::Nothing,
                Some(id) => {
                    let found =
                        self.find_work_task(player_id, idx, id.id(), None, MasterSwap::Keep)?;
                    let range = found.map_or(task_range, |task| task.work_range);
                    let sprite = found.and_then(|task| task.sprites.carrying);
                    DropSearch::Site { id, range, sprite }
                }
            }
        } else {
            DropSearch::Empty
        };
        // the Build's set_state(8) on the builder's step: its prelude leaves the builders, its
        // second targets and its notify go to the world and the unit AI at once
        let roles = self.model.roles.clone();
        self.run_on_step(player_id, idx, |unit, actions, step| {
            let dropped = build.set_drop(step, unit, search, by, &roles);
            actions.list.set_build(Some(build));
            dropped
        })
    }

    /// Slot 0x78's answer to a load of another resource than its task's (`FUN_140a78060`): the
    /// unit AI's slot 0x38 takes notify 0x21c ([`crate::unit_ai::NotifyKind::OTHER_LOAD`]) from and to the
    /// unit at (`player_id`, `idx`); a unit without a unit AI, none.
    ///
    /// # Errors
    ///
    /// A unit the world does not hold at (`player_id`, `idx`), or a notify the AI's queue does not
    /// take.
    pub(super) fn other_load(&mut self, player_id: PlayerId, idx: ListIndex) -> GenieResult<()> {
        let unit = self.players.live_mut(player_id, idx)?;
        let (id, state) = (unit.id, unit.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let notify = Notify::own(id, NotifyKind::OTHER_LOAD, [0; 3]);
            let _ = ai.accept(notify, state);
        }
        Ok(())
    }

    /// The villager's case 0x21c ([`crate::unit_ai::AiCommand::DropElsewhere`]) for the unit at
    /// (`unit_owner`, `idx`) with the drop-site table's building `site` at `point`, its place where
    /// the table's search found it (`FUN_141658e20`), and the target its top action had (`back`,
    /// slot 0x190: the action's target id, `+0x30`, that the world holds): it works on the site at
    /// its place (slot 0x128 with 1, the retargeting byte 0 and any task, [`World::give_work`]:
    /// its unit AI's order queue empties first); with a target its unit AI then queues the order
    /// on it (slot 0x30:
    /// `0x2db` issued by the unit, the target's master as its type and its owner's number, its
    /// place, shown at its x and y, range 0, priority 100, ready, neither at once nor in front).
    ///
    /// # Errors
    ///
    /// What the work and the queue do not model.
    pub(crate) fn drop_elsewhere(
        &mut self,
        unit_owner: PlayerId,
        idx: ListIndex,
        (site, point): (DropSiteId, Position),
        back: Option<ObjectId>,
    ) -> GenieResult<()> {
        let back = back
            .filter(|object_id| !object_id.is_recyclable())
            .and_then(|object_id| {
                let (player_id, unit) = self.object(object_id)?;
                Some((object_id, unit.life.master_id, player_id, unit.position()))
            });
        let id = self.players.live(unit_owner, idx)?.id;
        self.give_work(id, &Work::on(Some(site.id()), point), OrderSource::Direct)?;
        let Some((target, master, owner, place)) = back else {
            return Ok(());
        };
        let unit = self.players.live_mut(unit_owner, idx)?;
        let (id, state) = (unit.id, unit.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let order = QueuedOrder {
                issuer: Some(Issuer::Object(id)),
                order: OrderKind::OnTarget,
                priority: OrderPriority::COMMAND,
                target: Some(target),
                target_owner: Some(owner),
                point: place,
                range: Distance::ZERO,
                shown: place.ground(),
                ready: true,
                target_kind: Some(OrderTargetKind::of_master(master)),
            };
            let _ = ai.queue_order(state, order, Dispatch::Queued, QueueEnd::Back)?;
        }
        Ok(())
    }

    /// `FUN_141205ac0`: the unit at (`player_id`, `idx`) queues its way back to work: the work its
    /// town-bell records keep ([`crate::orders::town_bell::TownBell::back_to_work`]: the object the world
    /// holds, the second record moved up when it holds the object; else the record as it is)
    /// goes in front of its unit AI's queue (`FUN_140c41ab0` -> `FUN_140c42640`): the order on a
    /// target (`0x325`) issued by its owner on the object (its master the target type, its owner
    /// the target's, its place the point) or on the record's target (its target type, owner −1,
    /// its point); height 0, range 0, priority 100, shown at the point, ready. A unit without a
    /// unit AI only looks the records up.
    pub(crate) fn queue_way_back(
        &mut self,
        player_id: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<()> {
        let mut bell = self
            .parts
            .town_bell_of(self.players.live(player_id, idx)?.id);
        let (found, record) = bell.back_to_work(self.objects(), &self.model.roles)?;
        let target = match found {
            Some(record_target) => {
                let (op, unit) = self
                    .object(record_target.id)
                    .ok_or_else(|| object_gone(record_target.id))?;
                let master = unit.master(self.master_tables(), op)?.id;
                (
                    Some(record_target.id),
                    Some(OrderTargetKind::of_master(master)),
                    Some(op),
                    unit.position().ground(),
                )
            }
            None => (
                record.target,
                record.target_class.and_then(OrderTargetKind::of_class),
                None,
                record.point,
            ),
        };
        let unit = self.players.live_mut(player_id, idx)?;
        self.parts.set_town_bell(unit.id, bell);
        let Some(ai) = self.parts.unit_ai.get_mut(unit.id) else {
            return Ok(());
        };
        let (id, target_kind, owner, at) = target;
        let (x, y) = (at.x(), at.y());
        ai.insert_order(
            QueuedOrder {
                issuer: Some(Issuer::Player(player_id)),
                order: OrderKind::OnTarget2,
                priority: OrderPriority::COMMAND,
                target: id,
                target_owner: owner,
                point: Position::new(x, y, 0.0),
                range: Distance::ZERO,
                shown: Point::new(x, y),
                ready: true,
                target_kind,
            },
            QueueEnd::Front,
        );
        Ok(())
    }
}
