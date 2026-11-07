//! A new unit's walk to its building's gather point (`FUN_140ace9c0`).

use crate::commands::GatherPoint;
use crate::garrison::ContainerId;
use crate::garrison::InsideCount;
use crate::map::Distance;
use crate::map::Position;
use crate::math::rng::Stream;
use crate::movement::Speed;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::Voice;
use crate::objects::masters::MasterKind;
use crate::objects::masters::MasterTables;
use crate::objects::unit::Unit;
use crate::orders::OrderSource;
use crate::orders::Work;
use crate::players::PlayerId;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

use crate::garrison::ENTER;
use crate::movement::move_to::{MoveTo, MoveToAction, MoveWait, TargetMoved};
use crate::unit_ai::AiAction;
use crate::unit_ai::OrderKind;
use crate::world::World;
use crate::world::layer::ClassRoles;

/// A building takes in its own gather point's new unit at once while it holds fewer than these
/// (10, `FUN_140ace9c0`'s test) or fewer than its capacity.
const FORCED_ENTRY_BELOW: InsideCount = InsideCount::new(10);

/// Whether a gather point's walk asks the unit's garrison finder first (a new unit's) or goes to
/// slot 0x128 at once (an unloaded unit's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FinderUse {
    /// It asks the finder.
    Asks,
    /// It skips it.
    Skips,
}

impl World {
    /// `FUN_140ace9c0` with a gather point without a target, and an unloaded unit's move
    /// (`FUN_1411e7220`): the unit waits to move (slot 0x2a0 with 1) and takes slot 0x128 with no
    /// target at the point (z 0) from `source` ([`World::with_order_source`]: 1 for a new unit,
    /// whose unit AI's order queue empties; 0 for an unloaded one); slot 0x7e8 (`FUN_141227190`)
    /// runs the work command (`FUN_14106ff70`), which finds no task: a unit with speed in state 2
    /// notifies its unit AI with the `MoveTo`'s order and action, no target and the point, its
    /// notifies cleared by its `+0xa14` ([`UnitAi::notify_work`], slot 0x828), and moves (slot
    /// 0x138: [`World::move_start`]; `FUN_14106d340`: the notifies cleared with the world time,
    /// then, the unit holding no action, the `MoveTo` to the point, range 0, installed, and the
    /// command count one more). `FUN_14106c590` counts it once more after the work.
    ///
    /// [`UnitAi::notify_work`]: crate::unit_ai::UnitAi::notify_work
    pub(crate) fn gather_move(
        &mut self,
        player_id: PlayerId,
        new: ListIndex,
        point: Position,
        source: OrderSource,
    ) -> GenieResult<()> {
        let id = self.players.live(player_id, new)?.id;
        self.with_order_source(id, source, |world| {
            world.gather_move_work(player_id, new, point)?;
            world.count_command(id)
        })
    }

    /// [`World::gather_move`]'s work command inside slot 0x128's frame.
    fn gather_move_work(
        &mut self,
        player_id: PlayerId,
        new: ListIndex,
        point: Position,
    ) -> GenieResult<()> {
        let world_time = self.world_time;
        let sprite = self.point_move_sprite(player_id, new)?;
        let unit = self.players.live_mut(player_id, new)?;
        if let Some(mover) = self.parts.mover_mut(unit.id) {
            mover.waiting = 1;
        }
        if self.parts.actions_of(unit.id).top_action_kind().is_some() {
            return Err(GenieError::unsupported(
                "a gather point's move of a unit with an action",
            ));
        }
        // FUN_14106ff70: a unit whose master does not move (slot 0xe8, its speed `+0x740`;
        // `COMISS`, `JBE`: a NaN does not) takes nothing; one not in state 2 would notify
        // without moving
        let master_speed = unit
            .master(
                MasterTables::new(&self.model.masters, &self.parts.own_master),
                player_id,
            )?
            .animated_part()
            .max_speed;
        let moves = Speed::ZERO < master_speed;
        if !moves {
            return Ok(());
        }
        if unit.life.state != ObjectState::Alive {
            return Err(GenieError::unsupported(
                "a gather point's move of a unit not in state 2",
            ));
        }
        let source = self.parts.combat_of(unit.id).order_source;
        let kind = self
            .players
            .live(player_id, new)?
            .master(self.master_tables(), player_id)?
            .kind;
        let unit = self.players.live_mut(player_id, new)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.notify_work(
                Some(OrderKind::Move),
                Some(AiAction::Move),
                None,
                point,
                source.notifies(kind),
                self.streams.get_mut(Stream::Ai),
            )?;
        }
        self.move_start(player_id, new)?;
        let unit = self.players.live(player_id, new)?;
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            ai.clear_notifies(world_time.since_start())?;
        }
        self.parts
            .actions_mut(unit.id)?
            .list
            .set_move_to(Some(MoveToAction {
                sprite,
                state: MoveTo::Start,
                point,
                target: None,
                range: Distance::ZERO,
                voice: Voice::Loud,
                on_target_moved: TargetMoved::Follows,
                wait: MoveWait::default(),
            }));
        let id = self.players.live(player_id, new)?.id;
        self.count_command(id)
    }

    /// `FUN_140ace9c0`'s search for a gather point's target that is gone: the closest (by its
    /// centre, the first of equals) of Gaia's live objects of the class of Gaia's master the
    /// point names (`FUN_141166670` over `+0x30`), unless it lies farther than the new unit's
    /// line of sight (squared) from the point, measured from its footprint (`FUN_140a847c0`);
    /// then, but for the classes 5, 29 and 33 (the unit AI's slot 0x80, not modelled), the same
    /// over Gaia's sleeping objects (`+0x38`). None found: the unit moves to the point.
    pub(super) fn lost_gather_target(
        &self,
        player_id: PlayerId,
        new: ListIndex,
        gather_point: GatherPoint,
    ) -> GenieResult<Option<ObjectId>> {
        let class = self
            .model
            .masters
            .first()
            .and_then(|masters| {
                gather_point
                    .target_master
                    .and_then(|master_id| masters.get(master_id))
            })
            .map(|master| master.class);
        let los = self
            .players
            .live(player_id, new)?
            .master(self.master_tables(), player_id)?
            .line_of_sight;
        let (x, y) = (gather_point.point.x(), gather_point.point.y());
        let gaia = self
            .players
            .first()
            .ok_or(GenieError::unsupported("a world without Gaia"))?;
        let closest = |list: &[Unit]| -> GenieResult<Option<ObjectId>> {
            let mut best: Option<(&Unit, f32)> = None;
            for unit in list {
                if Some(unit.master(self.master_tables(), PlayerId::GAIA)?.class) != class {
                    continue;
                }
                let (dx, dy) = (x - unit.position().x(), y - unit.position().y());
                let distance = dy * dy + dx * dx;
                if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                    best = Some((unit, distance));
                }
            }
            let Some((unit, _)) = best else {
                return Ok(None);
            };
            let radius = unit.master(self.master_tables(), PlayerId::GAIA)?.radius;
            let gap = |delta: f32, radius: f32| {
                let beyond = delta.abs() - radius;
                if 0.0 <= beyond { beyond } else { 0.0 }
            };
            let (gx, gy) = (
                gap(x - unit.position().x(), radius.x()),
                gap(y - unit.position().y(), radius.y()),
            );
            Ok((gy * gy + gx * gx <= los.squared().get()).then_some(unit.id))
        };
        if let Some(id) = closest(gaia.objects.as_slice())? {
            return Ok(Some(id));
        }
        if self.model.roles.of_some_class(class).intersects(
            ClassRoles::OCEAN_FISH | ClassRoles::DOMESTIC_ANIMAL | ClassRoles::SHORE_FISH,
        ) {
            return Err(GenieError::unsupported(
                "a gather point's lost target of class 5, 29 or 33 (the unit AI's slot 0x80)",
            ));
        }
        closest(gaia.sleeping.as_slice())
    }

    /// The new unit's walk to the gather point of its building `building` (`FUN_140ace9c0` with a
    /// point or a target): a point is a move; a target that is gone is looked for again
    /// ([`World::lost_gather_target`]). The unit waits to move (slot 0x2a0 with 1); on a building
    /// its own task list's garrison finder (filter 3, [`World::garrison_task_found`]) decides: a
    /// task found is worked at once (slot 0x7e8 with filter 3, outside slot 0x128: `+0xa14` 1);
    /// else, and for any other target, slot 0x128 works it (`source`, any task). An unloaded
    /// unit's caller goes to slot 0x128 at once ([`FinderUse::Skips`]). A gather point on its own
    /// building takes the unit in at once ([`World::enter_forced`]) while the building holds
    /// fewer than 10 or fewer than its capacity.
    pub(crate) fn gather(
        &mut self,
        player_id: PlayerId,
        new: ListIndex,
        gather_point: GatherPoint,
        building: ObjectId,
        source: OrderSource,
        finder: FinderUse,
    ) -> GenieResult<()> {
        // the walk's place is the point on the ground (height 0)
        let at = gather_point.point.with_z(0.0);
        if gather_point.target.is_none() {
            return self.gather_move(player_id, new, at, source);
        }
        // its own building takes it at once (slot 0x1a0 forced) while it holds fewer than 10 or
        // fewer than its master's capacity (its slot 0x498 against `+0x5d4`); else the building
        // is a target as any other
        if gather_point.target == Some(building)
            && let Some((bp, building_unit)) = self.object(building)
        {
            let room = building_unit.master(self.master_tables(), bp)?.garrison;
            let held = self.garrisoned(building_unit.id);
            if held < FORCED_ENTRY_BELOW || held.below(room) {
                return self.enter_forced(player_id, new, ContainerId::new(building));
            }
        }
        let Some(target) = self.gather_target(player_id, new, gather_point)? else {
            return self.gather_move(player_id, new, at, source);
        };
        self.gather_work(player_id, new, target, at, source, finder)
    }

    /// [`World::gather`]'s target: the gather point's while the world holds it, else the one
    /// looked for again ([`World::lost_gather_target`]); none found leaves a move.
    ///
    /// # Errors
    ///
    /// What the search for a lost target does not model.
    fn gather_target(
        &self,
        player_id: PlayerId,
        new: ListIndex,
        gather_point: GatherPoint,
    ) -> GenieResult<Option<ObjectId>> {
        gather_point
            .target
            .filter(|&object_id| self.object(object_id).is_some())
            .map_or_else(
                || self.lost_gather_target(player_id, new, gather_point),
                |object_id| Ok(Some(object_id)),
            )
    }

    /// [`World::gather`]'s work on `target` the world holds at `at`: the unit waits to move (slot
    /// 0x2a0 with 1); on a building whose garrison task the finder finds (filter 3,
    /// [`World::garrison_task_found`]) slot 0x7e8 with the garrison filter, outside slot 0x128;
    /// else slot 0x128 from `source` with any task ([`World::give_work`]).
    ///
    /// # Errors
    ///
    /// A target the world no longer holds, its master, and what the work does not model.
    fn gather_work(
        &mut self,
        player_id: PlayerId,
        new: ListIndex,
        target: ObjectId,
        at: Position,
        source: OrderSource,
        finder: FinderUse,
    ) -> GenieResult<()> {
        // both of the target's sources answer a target the world holds
        let (owner, unit) = self.object(target).ok_or(GenieError::unreachable(
            "a gather point's target the world held just before",
        ))?;
        let on_building = unit.master(self.master_tables(), owner)?.kind == MasterKind::BUILDING;
        if let Some(mover) = self.parts.mover_mut(self.players.live(player_id, new)?.id) {
            mover.waiting = 1;
        }
        let garrison = finder == FinderUse::Asks
            && on_building
            && self.garrison_task_found(player_id, new, target, at)?;
        // the garrison task found: slot 0x7e8 with the garrison filter, outside slot 0x128
        if garrison {
            let work = Work {
                filter: Some(ENTER),
                ..Work::on(Some(target), at)
            };
            return self.order_work(player_id, new, &work);
        }
        let id = self.players.live(player_id, new)?.id;
        self.give_work(id, &Work::on(Some(target), at), source)
    }
}
