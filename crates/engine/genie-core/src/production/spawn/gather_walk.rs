//! A new unit's walk to its building's gather point (`FUN_140ace9c0`).

use crate::GenieError;
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
use crate::objects::unit::Unit;
use crate::orders::OrderSource;
use crate::orders::Work;
use crate::orders::town_bell::BellShelter;
use crate::players::PlayerId;
use crate::world::slots::PlayerSlots;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinderUse {
    /// It asks the finder.
    Asks,
    /// It skips it.
    Skips,
}

impl World {
    /// `FUN_140ace9c0` with a gather point without a target, and an unloaded unit's move
    /// (`FUN_1411e7220`): the unit waits to move (slot 0x2a0 with 1) and takes slot 0x128
    /// (`FUN_141211b80`) with no target at the point (z 0; its `+0xa14` `source`: 1 for a new
    /// unit, whose unit AI's order queue empties, `FUN_140c5e010`; 0 for an unloaded one); slot
    /// 0x7e8 (`FUN_141227190`) runs the work command (`FUN_14106ff70`), which finds no task: a
    /// unit with speed in state 2 notifies its unit AI with the `MoveTo`'s order and action, no
    /// target and the point, its notifies cleared with `+0xa14` ([`UnitAi::notify_work`], slot
    /// 0x828), and moves (slot 0x138, `FUN_141216f10`: a villager is no longer under the town bell,
    /// `+0x9dc`; with `+0xa14` the order queue empties again; `FUN_14106d340`: the notifies
    /// cleared with the world time, then, the unit holding no action, the `MoveTo` to the point,
    /// range 0, installed). The command count (`+0x888`) is not held.
    ///
    /// [`UnitAi::notify_work`]: crate::unit_ai::UnitAi::notify_work
    pub(crate) fn gather_move(
        &mut self,
        p: PlayerId,
        new: ListIndex,
        point: Position,
        source: OrderSource,
    ) -> Result<(), GenieError> {
        let world_time = self.world_time;
        let sprite = self.point_move_sprite(p, new)?;
        let villager =
            // A building's master type (`'P'`).
            // A combat unit's master type (`'F'`, `TRIBE_Master_Combat_Object`).
            // The wall class, which the search checks the building's corners for (`FUN_1415417f0`).
            // Classes the placement tests treat apart: a tree placed (15) passes over farms (49); buildings
            // of classes 51 and 54 block though desolid.
            // The gate class, whose own walls BUILD's placement scan passes over (`FUN_14123aee0`).
            // The relic's master, which always stays (`FUN_141194e80`).
            // The villager class, whose new units count in attribute 248.
            self.model.roles.of_class(self.players.live(p, new)?.master(self.master_tables(), p)?.class).contains(ClassRoles::VILLAGER);
        let unit = self.players.live_mut(p, new)?;
        if let Some(m) = self.parts.mover_mut(unit.id) {
            m.waiting = 1;
        }
        if self.parts.actions_of(unit.id).top_action_kind().is_some() {
            return Err(GenieError::unsupported(
                "a gather point's move of a unit with an action",
            ));
        }
        // a unit without speed takes nothing; one not in state 2 would notify without moving
        if self
            .parts
            .mover_of(unit.id)
            .map_or(Speed::ZERO, |m| m.stats.speed)
            <= Speed::ZERO
        {
            return Ok(());
        }
        if unit.life.state != ObjectState::Alive {
            return Err(GenieError::unsupported(
                "a gather point's move of a unit not in state 2",
            ));
        }
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            if source == OrderSource::Direct {
                ai.orders.clear();
            }
            ai.notify_work(
                Some(OrderKind::Move),
                Some(AiAction::Move),
                None,
                point,
                source.notifies(),
                self.streams.get(Stream::Ai),
            )?;
            if source == OrderSource::Direct {
                ai.orders.clear();
            }
            ai.clear_notifies(world_time.since_start())?;
        }
        if villager {
            self.parts.set_town_bell_flag(unit.id, BellShelter::Free);
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
        Ok(())
    }

    /// `FUN_140ace9c0` with a gather point (`+0xc30` 1) and a building whose unit AI holds no
    /// command queue location (its slot 0x218): the new unit waits to move (slot 0x2a0 with 1,
    /// `+0x7f9`) and takes the work command on the target at the point (slot 0x128: z 0, its
    /// `+0xa14` 1, no carry test, any task). A target that is gone is looked for again
    /// ([`World::lost_gather_target`]); one on the building itself (a garrison) and a building
    /// target (slot 0x7e8) are not modelled.
    /// `FUN_140ace9c0`'s search for a gather point's target that is gone: the closest (by its
    /// centre, the first of equals) of Gaia's live objects of the class of Gaia's master the
    /// point names (`FUN_141166670` over `+0x30`), unless it lies farther than the new unit's
    /// line of sight (squared) from the point, measured from its footprint (`FUN_140a847c0`);
    /// then, but for the classes 5, 29 and 33 (the unit AI's slot 0x80, not modelled), the same
    /// over Gaia's sleeping objects (`+0x38`). None found: the unit moves to the point.
    pub(super) fn lost_gather_target(
        &self,
        p: PlayerId,
        new: ListIndex,
        g: GatherPoint,
    ) -> Result<Option<ObjectId>, GenieError> {
        let class = self
            .model
            .masters
            .first()
            .and_then(|m| g.target_master.and_then(|t| m.get(t)))
            .map(|m| m.class);
        let los = self
            .players
            .live(p, new)?
            .master(self.master_tables(), p)?
            .line_of_sight;
        let (x, y) = (g.point.x(), g.point.y());
        let gaia = self
            .players
            .first()
            .ok_or(GenieError::unsupported("a world without Gaia"))?;
        let closest = |list: &[Unit]| -> Result<Option<ObjectId>, GenieError> {
            let mut best: Option<(&Unit, f32)> = None;
            for u in list {
                if Some(u.master(self.master_tables(), PlayerId::GAIA)?.class) != class {
                    continue;
                }
                let (dx, dy) = (x - u.position().x(), y - u.position().y());
                let d = dy * dy + dx * dx;
                if best.is_none_or(|(_, b)| d < b) {
                    best = Some((u, d));
                }
            }
            let Some((u, _)) = best else {
                return Ok(None);
            };
            let radius = u.master(self.master_tables(), PlayerId::GAIA)?.radius;
            let gap = |d: f32, r: f32| {
                let g = d.abs() - r;
                if 0.0 <= g { g } else { 0.0 }
            };
            let (gx, gy) = (
                gap(x - u.position().x(), radius.x()),
                gap(y - u.position().y(), radius.y()),
            );
            Ok((gy * gy + gx * gx <= los.squared().get()).then_some(u.id))
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
        p: PlayerId,
        new: ListIndex,
        g: GatherPoint,
        building: ObjectId,
        source: OrderSource,
        finder: FinderUse,
    ) -> Result<(), GenieError> {
        // the walk's place is the point on the ground (height 0)
        let at = g.point.with_z(0.0);
        if g.target.is_none() {
            return self.gather_move(p, new, at, source);
        }
        // its own building takes it at once (slot 0x1a0 forced) while it holds fewer than 10 or
        // fewer than its master's capacity (its slot 0x498 against `+0x5d4`); else the building
        // is a target as any other
        if g.target == Some(building)
            && let Some((bp, b)) = self.object(building)
        {
            let room = b.master(self.master_tables(), bp)?.garrison;
            let held = self.garrisoned(b.id)?;
            if held < FORCED_ENTRY_BELOW || held.below(room) {
                return self.enter_forced(p, new, ContainerId::new(building));
            }
        }
        let target = match g.target.filter(|&t| self.object(t).is_some()) {
            Some(t) => t,
            None => match self.lost_gather_target(p, new, g)? {
                Some(id) => id,
                None => return self.gather_move(p, new, at, source),
            },
        };
        let (owner, t) = self.object(target).ok_or(GenieError::unreachable(
            "unreachable: a gather point's target found",
        ))?;
        let on_building = t.master(self.master_tables(), owner)?.kind == MasterKind::BUILDING;
        if let Some(m) = self.parts.mover_mut(self.players.live(p, new)?.id) {
            m.waiting = 1;
        }
        let garrison = finder == FinderUse::Asks
            && on_building
            && self.garrison_task_found(p, new, target, at)?;
        let work = Work {
            target: Some(target),
            point: at,
            carry_test: false,
            filter: if garrison { Some(ENTER) } else { None },
            source: if garrison {
                OrderSource::Direct
            } else {
                source
            },
        };
        self.order_work(p, new, &work)
    }
}
