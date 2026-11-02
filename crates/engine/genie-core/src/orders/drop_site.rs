//! The drop-offs: an unasked one, the ordered drop site, the table's, one now and elsewhere,
//! and the way back.

use crate::GenieError;
use crate::gathering::DropSiteId;
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
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::class;
use crate::objects::masters::ids;
use crate::orders::Dispatch;
use crate::orders::object_gone;
use crate::orders::{HUNTED_LOADS, NO_POINT, Work, nearest_table_site};
use crate::players::Amount;
use crate::players::PlayerId;
use crate::players::attribute::AttributeId;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderTargetType;
use crate::unit_ai::QueueEnd;
use crate::unit_ai::world_view::DropWorld;
use crate::unit_ai::{Notify, NotifyKind, OrderPriority, QueuedOrder};
use crate::world::Seconds;
use crate::world::World;
use crate::world::index::ObjectsRef;
use crate::world::others::OthersRef;
use crate::world::slots::PlayerSlots;

impl World {
    /// `UNIT_ORDER`'s drop-off of a carrying unit without a gather action on top
    /// (`FUN_14161e370`'s case 8, past the Gather, Hunt and Build): a master with drop sites
    /// (`FUN_14109e210`: one not below 0) searches them from the unit ([`World::ordered_drop_site`]);
    /// with none found, or without drop sites, the drop-site table's nearest building for the
    /// resource it holds from its class ([`World::table_drop_site`]); with none, nothing. A unit
    /// (type 70 and up) then works on it (slot 0x7e8 at (−1, −1, −1): no carried-resource test,
    /// the task filter 110 for a load of meat, wood or food, kinds 15, 1 and 0, else 5;
    /// [`World::order_work`]) and, for the drop-off 8 (`way_back`), queues its way back to work
    /// ([`World::queue_way_back`]).
    pub(super) fn drop_off_unasked(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        way_back: bool,
    ) -> Result<(), GenieError> {
        let unit = &self.players.live(p, idx)?;
        let master = unit.master(self.master_tables(), p)?;
        let (kind, class, at) = (master.kind, master.class, unit.position().ground());
        let held = unit.life.attribute_type;
        let mut site = None;
        if !master.drop_sites.is_empty() {
            site = self.ordered_drop_site(p, idx)?;
        }
        if site.is_none() {
            site = self.table_drop_site(p, at, held, class)?;
        }
        let Some(site) = site else {
            return Ok(());
        };
        if kind < MasterKind::COMBAT {
            return Ok(());
        }
        let filter = if held.is_some_and(|h| HUNTED_LOADS.contains(&h)) {
            HUNT
        } else {
            GATHER
        };
        let work = Work {
            target: Some(site.id()),
            point: Position::from([NO_POINT; 3]),
            carry_test: false,
            filter: Some(filter),
            notified: false,
        };
        self.order_work(p, idx, &work)?;
        if way_back {
            self.queue_way_back(p, idx)?;
        }
        Ok(())
    }

    /// `FUN_1411671e0` as `UNIT_ORDER`'s drop-off calls it for the unit at (`p`, `idx`): over its
    /// master's drop sites, of its player's live objects in state 2, inside others too, at work
    /// or not, by the squared distance (not by path); the unit the seeker (its zone, its
    /// planner) where its slot 0x2c0 answers 1: any unit below type 70, else one whose master
    /// drops at neither 45 nor 2172 and of a class other than 21 and 2 (`FUN_141226580`).
    pub(super) fn ordered_drop_site(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<Option<DropSiteId>, GenieError> {
        let by_id = ObjectsRef::new(&self.players, &self.index, &self.masters, &self.parts);
        let unit = &self.players.live(p, idx)?;
        let master = unit.master(self.master_tables(), p)?;
        let seeks = master.kind < MasterKind::COMBAT
            || (!master
                .drop_sites
                .iter()
                .any(|&d| d == ids::ID_45 || d == ids::ID_2172)
                && !matches!(master.class, class::FISHING_SHIP | class::TRADE_BOAT));
        let masters = self.masters.of(p).ok_or(GenieError::unsupported(
            "a drop-off of a player without masters",
        ))?;
        if !seeks {
            return drop_site_search(
                &self.players.player(p)?.objects,
                OwnerMasters::new(&self.parts.own_master, Some(masters)),
                &master.drop_sites,
                unit.position(),
                None,
                (false, None),
                by_id,
            );
        }
        let state = self
            .parts
            .actions_of(unit.id)
            .top_action()
            .map_or(ActionState::NO_ACTION, |a| a.state);
        let player = &self.players.player(p)?;
        let mut world = DropWorld {
            masters: OwnerMasters::new(&self.parts.own_master, self.masters.of(p)),
            spatial: &mut self.spatial,
            zones: &mut self.zones,
            pathing: &mut self.pathing,
            object: self.streams.get(Stream::Object),
            world_time: self.world_time,
            player_type: player.kind,
            groups: &self.groups,
        };
        let others = OthersRef::of(&self.players, &self.index, p)?;
        player.live(idx)?.drop_site_with(
            (self.parts.actions_of(unit.id), self.parts.mover_of(unit.id)),
            &mut world,
            others,
            state,
            (false, false, None),
            by_id,
        )
    }

    /// `FUN_141658e20` for player `p` at `at`: of the buildings the drop-site table gives for the
    /// resource `held` from workers of `class` ([`crate::unit_ai::task_finder::table_drop_buildings`] for the
    /// player's civ), the objects of each master's done list in the player's tracker in turn
    /// (`FUN_14104f600`, the master itself the key) that the world holds in state 2, the nearest
    /// by the squared distance of the centres (the first of equals; none from
    /// [`TABLE_DROP_FAR`] on).
    ///
    /// [`TABLE_DROP_FAR`]: crate::orders::TABLE_DROP_FAR
    pub(crate) fn table_drop_site(
        &self,
        p: PlayerId,
        at: Point,
        held: Option<AttributeId>,
        class: ClassId,
    ) -> Result<Option<DropSiteId>, GenieError> {
        let player = self.players.player(p)?;
        let look = |id: ObjectId| self.object(id).map(|(_, o)| (o.life.state, o.position()));
        Ok(nearest_table_site(
            (&self.drop_sites, player.civ, &player.tracker),
            (at, held, class),
            look,
        ))
    }

    /// The Build's `set_state(8)` as `UNIT_ORDER`'s drop-off (slot 0x78) gives it to the builder
    /// at (`p`, `idx`), on the builder's step ([`World::run_on_step`]): a builder that carries something (`+0x310` above 0, COMISS:
    /// not a NaN) searches the drop-site table ([`World::table_drop_site`] for what it carries
    /// and its class) and its slot 0x820 the task on the site found (any action type, no switch,
    /// [`World::find_work_task`]: its range, else the Build task's); then
    /// [`crate::construction::build::BuildAction::set_drop`]: a counted builder leaves its
    /// foundation's builders, the second targets go and come at once, notify 0x203 goes to the
    /// unit AI.
    pub(crate) fn build_drop_off_now(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
    ) -> Result<(), GenieError> {
        use crate::construction::build::DropSearch;
        let by = self.releaser(p, idx)?;
        let unit = &self.players.live(p, idx)?;
        let task_range = self
            .parts
            .actions_of(unit.id)
            .list
            .build()
            .ok_or(GenieError::unreachable(
                "a drop-off order without its Build",
            ))?
            .task
            .range;
        let (amount, held, at) = (
            unit.life.amount,
            unit.life.attribute_type,
            unit.position().ground(),
        );
        let class = unit.master(self.master_tables(), p)?.class;
        let search = if Amount::ZERO < amount {
            match self.table_drop_site(p, at, held, class)? {
                None => DropSearch::Nothing,
                Some(id) => {
                    let found = self.find_work_task(p, idx, id.id(), None, false)?;
                    let range = found.map_or(task_range, |t| t.work_range);
                    let sprite = found.and_then(|t| t.sprites.carrying);
                    DropSearch::Site { id, range, sprite }
                }
            }
        } else {
            DropSearch::Empty
        };
        // the Build's set_state(8) on the builder's step: its prelude leaves the builders, its
        // second targets and its notify go to the world and the unit AI at once
        self.run_on_step(p, idx, |unit, actions, c| {
            let mut build = *actions.list.build().ok_or(GenieError::unreachable(
                "a drop-off order without its Build",
            ))?;
            let dropped = build.set_drop(c, unit, search, by);
            if let Some(b) = actions.list.build_mut() {
                *b = build;
            }
            dropped
        })
    }

    /// Slot 0x78's answer to a load of another resource than its task's (`FUN_140a78060`): the
    /// unit AI's slot 0x38 takes notify 0x21c ([`crate::unit_ai::NotifyKind::OTHER_LOAD`]) from and to the
    /// unit at (`p`, `idx`); a unit without a unit AI, none.
    ///
    /// # Errors
    ///
    /// A unit the world does not hold at (`p`, `idx`), or a notify the AI's queue does not take.
    pub(super) fn other_load(&mut self, p: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        let unit = self.players.live_mut(p, idx)?;
        let (id, state) = (unit.id, unit.life.state);
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let n = Notify::own(id, NotifyKind::OTHER_LOAD, [0; 3]);
            let _ = ai.accept(n, state);
        }
        Ok(())
    }

    /// The villager's case 0x21c ([`crate::unit_ai::AiCommand::DropElsewhere`]) for the unit at
    /// (`p`, `idx`) with the drop-site table's building `site` (`FUN_141658e20`) and the target
    /// its top action had (`back`, slot 0x190: the action's target id, `+0x30`, that the world
    /// holds): it works on the site at its place (slot 0x128: the carried-resource test, any task,
    /// `+0xa14` 0, [`World::order_work`]); with a target its unit AI then queues the order on it
    /// (slot 0x30: `0x2db` issued by the unit, the target's master as its type and its owner's
    /// number, its place, shown at its x and y, range 0, priority 100, ready, neither at once nor
    /// in front).
    ///
    /// # Errors
    ///
    /// What the work and the queue do not model.
    pub(crate) fn drop_elsewhere(
        &mut self,
        p: PlayerId,
        idx: ListIndex,
        site: DropSiteId,
        back: Option<ObjectId>,
    ) -> Result<(), GenieError> {
        let back = back.filter(|t| !t.is_recyclable()).and_then(|t| {
            let (q, o) = self.object(t)?;
            Some((t, o.life.master_id, q, o.position()))
        });
        let point = self
            .object(site)
            .map(|(_, o)| o.position())
            .ok_or(GenieError::unreachable("the drop site found"))?;
        let work = Work {
            target: Some(site.id()),
            point,
            carry_test: true,
            filter: None,
            notified: false,
        };
        self.order_work(p, idx, &work)?;
        let Some((target, master, owner, place)) = back else {
            return Ok(());
        };
        let unit = self.players.live_mut(p, idx)?;
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
                target_type: Some(OrderTargetType::of_master(master)),
            };
            let _ = ai.queue_order(state, order, Dispatch::Queued, QueueEnd::Back)?;
        }
        Ok(())
    }

    /// `FUN_141205ac0`: the unit at (`p`, `idx`) queues its way back to work: the work its
    /// town-bell records keep ([`crate::orders::town_bell::TownBell::back_to_work`]: the object the world
    /// holds, the second record moved up when it holds the object; else the record as it is)
    /// goes in front of its unit AI's queue (`FUN_140c41ab0` -> `FUN_140c42640`): the order on a
    /// target (`0x325`) issued by its owner on the object (its master the target type, its owner
    /// the target's, its place the point) or on the record's target (its target type, owner −1,
    /// its point); height 0, range 0, priority 100, shown at the point, ready. A unit without a
    /// unit AI only looks the records up.
    pub(crate) fn queue_way_back(&mut self, p: PlayerId, idx: ListIndex) -> Result<(), GenieError> {
        use crate::orders::town_bell::RecordTarget;
        let mut bell = self.parts.town_bell_of(self.players.live(p, idx)?.id);
        let lookup = |t: ObjectId| -> Result<Option<RecordTarget>, GenieError> {
            let Some((op, o)) = self.object(t) else {
                return Ok(None);
            };
            let m = o.master(self.master_tables(), op)?;
            Ok(Some(RecordTarget {
                id: t,
                building: m.kind == MasterKind::BUILDING,
                class: m.class,
                built: self
                    .parts
                    .building_of(o.id)
                    .construction
                    .is_some_and(|c| Seconds::of_master(m.build_time) <= c.progress),
            }))
        };
        let (found, record) = bell.back_to_work(&lookup)?;
        let target = match found {
            Some(f) => {
                let (op, o) = self.object(f.id).ok_or_else(|| object_gone(f.id))?;
                let master = o.master(self.master_tables(), op)?.id;
                (
                    Some(f.id),
                    Some(OrderTargetType::of_master(master)),
                    Some(op),
                    o.position().ground(),
                )
            }
            None => (
                record.target,
                record.target_class.and_then(OrderTargetType::of_class),
                None,
                record.point,
            ),
        };
        let unit = self.players.live_mut(p, idx)?;
        self.parts.set_town_bell(unit.id, bell);
        let Some(ai) = self.parts.unit_ai.get_mut(unit.id) else {
            return Ok(());
        };
        let (id, target_type, owner, at) = target;
        let (x, y) = (at.x(), at.y());
        ai.insert_order(
            QueuedOrder {
                issuer: Some(Issuer::Player(p)),
                order: OrderKind::OnTarget2,
                priority: OrderPriority::COMMAND,
                target: id,
                target_owner: owner,
                point: Position::new(x, y, 0.0),
                range: Distance::ZERO,
                shown: Point::new(x, y),
                ready: true,
                target_type,
            },
            QueueEnd::Front,
        );
        Ok(())
    }
}
