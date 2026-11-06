//! PATROL, the attack move and their sweep, and the group the splitter keeps.

use crate::groups::FarMembers;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::groups::MoveFlags;
use crate::groups::{CommandMoveTo, common_group};
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::RestrictionId;
use crate::map::spatial::Tiles;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::movement::footprint_gap_squared;
use crate::objects::ObjectId;
use crate::objects::masters::Master;
use crate::objects::masters::MasterKind;
use crate::objects::unit::Unit;
use crate::orders::ControlKey;
use crate::orders::Kept;
use crate::orders::object_gone;
use crate::orders::{
    ATTACK_MOVE_QUEUED, LISTED, Place, Split, Sweep, ignore_unit, patrol_timeout,
    set_differentiator_groups, waypoints_from,
};
use crate::pathing::Pathing;
use crate::pathing::tile_path::{Found, Path, Request};
use crate::players::PlayerId;
use crate::unit_ai::AiAction;
use crate::unit_ai::AiMove;
use crate::unit_ai::OrderForce;
use crate::unit_ai::OrderKind;
use crate::unit_ai::Pace;
use crate::world::Millis;
use crate::world::World;
use crate::world::layer::generated::TilePathTuning;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// What PATROL's and `ATTACK_MOVE`'s flag `+0x8` (`FUN_14161b970`'s `param_6`) does to the listed
/// units' AIs' saved patrols (`zeroPatrolInformation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SavedPatrols {
    /// They go (1).
    Dropped,
    /// They stay (0).
    Kept,
}

impl World {
    /// PATROL (0x15): the command's check (`FUN_140e55590`, [`World::command_check`]: of the
    /// first 60 ids, 500 for a computer player, one the world lacks or another player's becomes
    /// −1, and a human player's command left with none is refused), then `FUN_140e69cc0`: the
    /// waypoints (1 to 10, the decoder's), those first ids, the gone ones null (`FUN_140e6b750`),
    /// the analytics record (`FUN_1409648c0`, no simulation state) and the order
    /// ([`World::patrol_order`]).
    pub(crate) fn patrol(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        waypoints: &[Point],
        patrols: SavedPatrols,
    ) -> GenieResult<()> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let cap = if self.players.player(player)?.kind.is_computer() {
            500
        } else {
            60
        };
        self.patrol_order(ids.split_at(ids.len().min(cap)).0, waypoints, patrols)
    }

    /// `ATTACK_MOVE` (0x21): the command's check (`FUN_140e54490`, [`World::command_check`], as
    /// PATROL's), then `FUN_140e66200`: with `+0x9` it takes the first waypoint out of the local
    /// player's list of attack-move points (`FUN_140e640a0`, the interface's: no simulation
    /// state); the first 60 ids (500 for a computer player) go, gone ones null
    /// (`FUN_140e6b750`), with the waypoints (1 to 10, the decoder's) and the analytics record
    /// (`FUN_1409648c0`) to `FUN_14161b970` without a target ([`World::sweep_order`]).
    ///
    /// # Errors
    ///
    /// What the order does not model.
    pub(crate) fn attack_move(
        &mut self,
        player: PlayerId,
        ids: &[Option<ObjectId>],
        waypoints: &[Point],
        sweep: Sweep,
    ) -> GenieResult<()> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let cap = if self.players.player(player)?.kind.is_computer() {
            500
        } else {
            60
        };
        self.sweep_order(ids.split_at(ids.len().min(cap)).0, waypoints, sweep)
    }

    /// `FUN_14161b970` for PATROL (its patrol flag 1, no target, `param_7` 0, `param_8` 1): first
    /// each listed unit's AI (the gone entries count in the list) takes an idle timer of 0 and an
    /// adjusted idle timeout and secondary timer of [`patrol_timeout`]; with
    /// [`SavedPatrols::Dropped`] (`+0x8`) its saved patrol goes (`zeroPatrolInformation`). The
    /// splitter (`FUN_140fd9ae0`, no Ctrl, [`World::split`]) parts the units. Each of its groups: a
    /// new group of the split's owner (`FUN_14152efc0`) takes them ([`World::add_to_group`]), its
    /// statistics and its place ([`World::group_statistics`], [`World::set_initial_position`]),
    /// unless they are a group the world holds whole, which the splitter keeps
    /// ([`World::keep_group`]); one the world does not hold takes nothing. Each listed group (60 at
    /// most, `DAT_143f152b8`) takes the command count as its differentiator, the waypoints its
    /// place passes ([`waypoints_from`]) but the last as its own (`FUN_14100f6b0`, the target
    /// forgotten) and a patrol's leg to the last ([`World::command_move_to`]: range 0, the target
    /// forgotten, direct). Then every listed group lists them all (`FUN_1410395a0`) and the command
    /// count goes on. Each loose unit last.
    pub(crate) fn patrol_order(
        &mut self,
        ids: &[Option<ObjectId>],
        waypoints: &[Point],
        patrols: SavedPatrols,
    ) -> GenieResult<()> {
        let sweep = Sweep {
            patrol: true,
            patrols,
            queued: false,
            ignore_pending: true,
        };
        self.sweep_order(ids, waypoints, sweep)
    }

    /// [`World::sweep_order`]'s first step of an attack move: each listed unit's AI, sleeping
    /// or not, resets as for an order.
    pub(super) fn reset_for_attack_move(&mut self, ids: &[Option<ObjectId>]) {
        for &id in ids {
            let place = self.place(id);
            let ai = place
                .and_then(|place| self.players.at_mut(place).ok())
                .and_then(|unit| self.parts.unit_ai.get_mut(unit.id));
            if let Some(ai) = ai {
                ai.reset_for_order(self.streams.get_mut(Stream::Ai));
            }
        }
    }

    /// [`World::sweep_order`]'s timers: each listed unit's AI idles from 0 to its place's patrol
    /// timeout ([`patrol_timeout`]), its patrol zeroed with [`SavedPatrols::Dropped`].
    pub(super) fn start_patrol_timers(&mut self, ids: &[Option<ObjectId>], patrols: SavedPatrols) {
        let dt = self.dt;
        for (index, &id) in ids.iter().enumerate() {
            if let Some(ai) = id.and_then(|id| self.parts.unit_ai.get_mut(id)) {
                ai.idle_timer = Millis::ZERO;
                let timeout = patrol_timeout(index, dt);
                ai.adjusted_idle_timeout = timeout;
                ai.secondary_timer = timeout;
                if patrols == SavedPatrols::Dropped {
                    ai.zero_patrol();
                }
            }
        }
    }

    /// `FUN_14161b970` without a target for PATROL ([`World::patrol_order`]) and `ATTACK_MOVE`: an
    /// attack move first resets each listed unit's AI as for an order (`FUN_141631040`,
    /// [`crate::unit_ai::UnitAi::reset_for_order`]); then as PATROL's, but that a group's move
    /// to the last waypoint holds (`commandMoveTo` with `param_6` 0, `param_8` 1) and each loose
    /// unit's is [`World::sweep_unit`]'s.
    ///
    /// # Errors
    ///
    /// What the groups' and the units' moves do not model.
    pub(crate) fn sweep_order(
        &mut self,
        ids: &[Option<ObjectId>],
        waypoints: &[Point],
        sweep: Sweep,
    ) -> GenieResult<()> {
        let Sweep {
            patrol, patrols, ..
        } = sweep;
        if !patrol {
            self.reset_for_attack_move(ids);
        }
        self.start_patrol_timers(ids, patrols);
        let mut split = self.split(ids, Kept::All, ControlKey::Up)?;
        let mut listed: Vec<GroupId> = Vec::new();
        for units in split.groups.clone() {
            let common = common_group(&units, |id| self.object(id).map(|(_, unit)| unit.group));
            let kept = match common {
                None => None,
                Some(id) => match self.groups.get(id) {
                    None => continue,
                    Some(group) => (group.members.len() == units.len()).then_some(id),
                },
            };
            let id = if let Some(id) = kept {
                self.keep_group(&mut split, id, None)?;
                id
            } else {
                let owner = split.owner;
                let id = self.groups.create(owner);
                for &unit in &units {
                    let _ = self.add_to_group(id, unit)?;
                }
                self.group_statistics(id, FarMembers::Stay)?;
                self.set_initial_position(id)?;
                id
            };
            if listed.len() < LISTED {
                listed.push(id);
            }
            let commands = self.groups.commands;
            let unit_group = self.groups.held_mut(id)?;
            unit_group.differentiator_id = commands;
            let kept = waypoints_from(
                unit_group.motion.position[0].ground(),
                waypoints,
                &self.model.tuning.order,
            );
            let Some((&last, before)) = kept.split_last() else {
                continue;
            };
            for point in before {
                unit_group.add_waypoint(*point, MoveFlags::FORGET_TARGET);
            }
            let leg = CommandMoveTo {
                range: Distance::ZERO,
                // a patrol's leg, else a hold
                flags: MoveFlags::FORGET_TARGET
                    | MoveFlags::DIRECT
                    | if patrol {
                        MoveFlags::LEG
                    } else {
                        MoveFlags::HOLD
                    },
            };
            let _ = self.command_move_to(id, None, last, leg)?;
        }
        for &id in &listed {
            if let Some(group) = self.groups.get_mut(id) {
                set_differentiator_groups(group, &listed);
            }
        }
        self.groups.count_command();
        for index in 0..split.loose.len() {
            self.sweep_unit(&split.loose, index, waypoints, sweep)?;
        }
        Ok(())
    }

    /// `FUN_140fd9f40`'s test of a command on a target: the squared gap between the unit and the
    /// target on the ground (`FUN_14118f8c0`) is above its master's line of sight squared (`COMISS`:
    /// a NaN is not), so the target is out of its sight.
    fn out_of_sight(&self, unit: &Unit, master: &Master, target: ObjectId) -> GenieResult<bool> {
        let Some(world_object) = self.world_object(target) else {
            return Ok(true);
        };
        let tm = self.master(
            world_object.player(),
            world_object.master_id(self.model.slots),
        )?;
        let gap = footprint_gap_squared(
            unit.position(),
            master.radius,
            world_object.position(),
            tm.radius,
        );
        Ok(master.line_of_sight.squared() < gap)
    }

    /// `FUN_140fd9f40` (the patrol's without a target, INTERACT's and MOVE's with the one the world
    /// holds, which a loose unit that sees it does not leave for): the splitter kept group `group`. Each
    /// loose unit of `split` the world holds, of type 70 or more whose master has a formation
    /// category (`+0x8bc`) and not among the split's far units, joins it (`FUN_1410083c0`; flags
    /// bit 17 set) when the tile planner of its terrain restriction, where one exists, finds a
    /// path from its place to the group's (`FUN_140f7c530`, no radii, no range: result 1) and
    /// where it last moved to (slot 0x520) is within its owner's influence distance of the
    /// group's place in three dimensions (`COMISS`/`JC`: a NaN is not); it leaves the loose units,
    /// and the loop's index goes on past the unit that takes its place, which is passed over.
    pub(crate) fn keep_group(
        &mut self,
        split: &mut Split,
        group: GroupId,
        target: Option<ObjectId>,
    ) -> GenieResult<()> {
        let mut index = 0;
        while index < split.loose.len() {
            let id = *split.loose.checked(index)?;
            let Some((player_id, unit)) = self.object(id) else {
                index += 1;
                continue;
            };
            let master = unit.master(self.master_tables(), player_id)?;
            if master.kind < MasterKind::COMBAT
                || !master.creatable_part().formation_category.grouped()
                || split.far.contains(&id)
            {
                index += 1;
                continue;
            }
            if let Some(object_id) = target
                && !self.out_of_sight(unit, master, object_id)?
            {
                index += 1;
                continue;
            }
            let held = self.groups.held(group)?;
            let at = held.motion.position[0];
            let from = unit.position().ground();
            let moved = self
                .parts
                .mover_of(unit.id)
                .map_or_else(|| unit.position(), |mover| mover.moved_at);
            let restriction = self
                .parts
                .mover_of(unit.id)
                .map(|mover| mover.stats.restriction);
            let reaches = can_reach_group(
                &mut self.pathing,
                self.spatial.tiles(),
                &self.model.tuning.tile_path,
                restriction,
                from,
                at.ground(),
            )?;
            if !reaches {
                index += 1;
                continue;
            }
            // slot 0x520: +0x824 as the game holds it (x, height, y)
            let offset = BVector::new(moved.x(), moved.z(), moved.y()) - at;
            let influence = self.players.player(player_id)?.formations.influence.get();
            if offset.x() * offset.x() + offset.height() * offset.height() + offset.y() * offset.y()
                <= influence * influence
            {
                let _ = split.loose.remove(index);
                let _ = self.add_to_group(group, id)?;
                if let Some(group) = self.groups.get_mut(group) {
                    group.flags |= GroupFlags::JOINED;
                }
            }
            index += 1;
        }
        Ok(())
    }

    /// The loose unit `loose[list_index]` of PATROL or `ATTACK_MOVE` (`FUN_14161b970`'s loop over
    /// the splitter's loose units): one the world does not hold takes nothing. An attack move's
    /// unit with a unit AI, `queued` (`+0x9`) or, without `ignore_pending` (`+0xa`), with an order
    /// in its queue that is not ready ([`crate::unit_ai::UnitAi::has_unready_order`]), takes the
    /// order through its queue (slot 0x30: the attack move `0x2dd` at the first waypoint, priority
    /// 100), which is not modelled: the call leaves the record's shown point and ready flag to the
    /// stack ([`ATTACK_MOVE_QUEUED`]). Any other leaves its unit group (`FUN_14102c160` with −1); a
    /// unit that moves (with movement state: type 30 or more) loses its own waypoints (slot 0x450)
    /// and its ignore list (`FUN_141197da0`), waits to move (slot 0x2a0 with 1) and lists the other
    /// loose units (`FUN_14118c790`); an object below type 30 has those slots empty, and its ignore
    /// list (the static part's, which only a walker's short pather reads) is not held. Of the
    /// waypoints its place passes ([`waypoints_from`]) none leaves it there; else each but the last
    /// becomes its own (slot 0x448 at its height) and it moves to the last (slot 0x340: range 0,
    /// forced for a patrol only, walking): of type 70 or 80 through `FUN_141216c50`
    /// ([`World::move_aside`]; type 50 and 60 run `FUN_1410866e0` without its class test, which
    /// only masters of type 70 meet), types 30 and 40 not at all (`FUN_14106d330`). Its unit AI
    /// takes the patrol order (an attack move's `0x2dd`) and the move action (`FUN_140c64b60` with
    /// 1, `FUN_140c64b50`) and no target at the command's last waypoint at the unit's height
    /// (`FUN_140c64be0`), even where the filter left that waypoint out.
    pub(super) fn sweep_unit(
        &mut self,
        loose: &[ObjectId],
        loose_index: usize,
        waypoints: &[Point],
        sweep: Sweep,
    ) -> GenieResult<()> {
        let id = *loose.checked(loose_index)?;
        let Some((player, unit)) = self.object(id) else {
            return Ok(());
        };
        if !sweep.patrol
            && self
                .parts
                .unit_ai
                .get(unit.id)
                .is_some_and(|ai| sweep.queued || (!sweep.ignore_pending && ai.has_unready_order()))
        {
            return Err(ATTACK_MOVE_QUEUED);
        }
        let kind = unit.master(self.master_tables(), player)?.kind;
        self.leave_group_of(id, None)?;
        // the group's leave moves its member's slot at most (`World::leave_group_of`)
        let (_, unit) = self.object(id).ok_or_else(|| object_gone(id))?;
        let at = unit.position();
        if let Some(mover) = self.parts.mover_mut(id) {
            mover.clear_user_waypoints();
            mover.ignore_units.clear();
            mover.waiting = 1;
            for (index, &other) in loose.iter().enumerate() {
                if index != loose_index {
                    ignore_unit(&mut mover.ignore_units, other);
                }
            }
        }
        let (x, y, z) = (at.x(), at.y(), at.z());
        let kept = waypoints_from(Point::new(x, y), waypoints, &self.model.tuning.order);
        let Some((last, before)) = kept.split_last() else {
            return Ok(());
        };
        let (lx, ly) = (last.x(), last.y());
        if let Some(mover) = self.parts.mover_mut(id) {
            for point in before {
                mover.add_user_waypoint(
                    Position::new(point.x(), point.y(), z),
                    &self.model.tuning.movement,
                );
            }
        }
        // a sleeping object is of a type below 30 (only slot 0x180 puts one to sleep)
        if MasterKind::RGE_COMBAT <= kind
            && let Some(Place::Live(player_id, idx)) = self.place(id)
        {
            let walk = AiMove {
                point: Position::new(lx, ly, z),
                range: Distance::ZERO,
                force: if sweep.patrol {
                    OrderForce::Forced
                } else {
                    OrderForce::Unforced
                },
                pace: Pace::Walk,
            };
            self.move_aside(player_id, idx, walk)?;
        }
        let unit = match self.place(id) {
            Some(Place::Live(owner, slot)) => self.players.live_mut(owner, slot)?,
            Some(Place::Sleeping(player_id, list_index)) => {
                self.players.asleep_mut(player_id, list_index)?
            }
            None => return Ok(()),
        };
        let rng = self.streams.get_mut(Stream::Ai);
        if let Some(ai) = self.parts.unit_ai.get_mut(unit.id) {
            let shown = waypoints.last().copied().unwrap_or_default();
            ai.set_order(
                Some(if sweep.patrol {
                    OrderKind::Patrol
                } else {
                    OrderKind::AttackMove
                }),
                rng,
            );
            ai.set_action(Some(AiAction::Move));
            ai.set_target(None, None, shown.with_height(z));
        }
        Ok(())
    }
}

/// `FUN_140fd9f40`'s path test ([`World::keep_group`]): whether the tile planner of a loose unit's
/// terrain restriction, where one exists, reaches the group's place `to` from the unit's `from`
/// (`FUN_140f7c530`, no radii, no range: result 1). A unit without a restriction or a planner
/// passes.
///
/// # Errors
///
/// What the planner's build or search fails on.
fn can_reach_group(
    pathing: &mut Pathing,
    tiles: &Tiles,
    tile_path_tuning: &TilePathTuning,
    restriction: Option<RestrictionId>,
    from: Point,
    to: Point,
) -> GenieResult<bool> {
    let Some(restriction_id) = restriction else {
        return Ok(true);
    };
    let Some(planner) = pathing.planner(restriction_id, tiles)? else {
        return Ok(true);
    };
    let req = Request {
        from,
        to,
        radii: Distance::ZERO,
        range: Distance::ZERO,
        closed: Vec::new(),
        full: false,
    };
    let mut path = Path::default();
    let found = planner.plan::<GenieError>(&req, false, &mut path, None, tile_path_tuning)?;
    Ok(found == Found::Goal)
}
