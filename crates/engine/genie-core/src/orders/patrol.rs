//! PATROL, the attack move and their sweep, and the group the splitter keeps.

use crate::GenieError;
use crate::groups::GroupFlags;
use crate::groups::GroupId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::math::fmath::BVector;
use crate::math::rng::Stream;
use crate::objects::ObjectId;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::masters::master_type::RGE_COMBAT;
use crate::orders::object_gone;
use crate::orders::{
    ATTACK_MOVE_QUEUED, LISTED, Place, Split, Sweep, ignore_unit, patrol_timeout,
    set_differentiator_groups, waypoints_from,
};
use crate::players::PlayerId;
use crate::unit_ai::ActionKind;
use crate::unit_ai::OrderKind;
use crate::world::Millis;
use crate::world::World;
use crate::world::slots::CheckedIndex;
use crate::world::slots::PlayerSlots;

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
        reset: bool,
    ) -> Result<(), GenieError> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        self.patrol_order(ids.split_at(ids.len().min(cap)).0, waypoints, reset)
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
    ) -> Result<(), GenieError> {
        let Some(ids) = self.command_check(player, ids)? else {
            return Ok(());
        };
        let cap = if self.players.player(player)?.computer {
            500
        } else {
            60
        };
        self.sweep_order(ids.split_at(ids.len().min(cap)).0, waypoints, sweep)
    }

    /// `FUN_14161b970` for PATROL (its patrol flag 1, no target, `param_7` 0, `param_8` 1):
    /// first each listed unit's AI (the gone entries count in the list) takes an idle timer of
    /// 0 and an adjusted idle timeout and secondary timer of [`patrol_timeout`]; with `reset`
    /// (`+0x8`) its saved patrol goes (`zeroPatrolInformation`). The splitter (`FUN_140fd9ae0`, no
    /// Ctrl, [`World::split`]) parts the units. Each of its groups: a new group of the split's
    /// owner (`FUN_14152efc0`) takes them ([`World::add_to_group`]), its statistics and its
    /// place ([`World::group_statistics`], [`World::set_initial_position`]), unless they are a
    /// group the world holds whole, which the splitter keeps ([`World::keep_group`]); one the
    /// world does not hold takes nothing. Each listed group (60 at most, `DAT_143f152b8`) takes
    /// the command count as its differentiator, the waypoints its place passes ([`waypoints_from`])
    /// but the last as its own (`FUN_14100f6b0`, the target forgotten) and a patrol's leg to the
    /// last ([`World::command_move_to`]: range 0, the target forgotten, direct). Then every
    /// listed group lists them all (`FUN_1410395a0`) and the command count goes on. Each loose
    /// unit last ([`World::patrol_unit`]).
    ///
    /// [`World::patrol_unit`]: crate::world::World::patrol_unit
    pub(crate) fn patrol_order(
        &mut self,
        ids: &[Option<ObjectId>],
        waypoints: &[Point],
        reset: bool,
    ) -> Result<(), GenieError> {
        let sweep = Sweep {
            patrol: true,
            reset,
            queued: false,
            ignore_pending: true,
        };
        self.sweep_order(ids, waypoints, sweep)
    }

    /// [`World::sweep_order`]'s first step of an attack move: each listed unit's AI, sleeping
    /// or not, resets as for an order.
    pub(super) fn reset_for_attack_move(&mut self, ids: &[Option<ObjectId>]) {
        for &id in ids {
            let ai = self.players.iter_mut().find_map(|player| {
                let unit = player
                    .objects
                    .by_id_mut(id)
                    .or_else(|| player.sleeping.by_id_mut(id))?;
                unit.ai.as_mut()
            });
            if let Some(ai) = ai {
                ai.reset_for_order(self.streams.get(Stream::Ai));
            }
        }
    }

    /// [`World::sweep_order`]'s timers: each listed unit's AI idles from 0 to its place's patrol
    /// timeout ([`patrol_timeout`]), its patrol zeroed with `reset`.
    pub(super) fn start_patrol_timers(&mut self, ids: &[Option<ObjectId>], reset: bool) {
        let dt = self.dt;
        for (i, &id) in ids.iter().enumerate() {
            if let Some(ai) = self.object_mut(id).and_then(|(_, u)| u.ai.as_mut()) {
                ai.idle_timer = Millis::ZERO;
                let timeout = patrol_timeout(i, dt);
                ai.adjusted_idle_timeout = timeout;
                ai.secondary_timer = timeout;
                if reset {
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
    ) -> Result<(), GenieError> {
        let Sweep { patrol, reset, .. } = sweep;
        if !patrol {
            self.reset_for_attack_move(ids);
        }
        self.start_patrol_timers(ids, reset);
        let mut split = self.split(ids, &|_| true, false)?;
        let mut listed: Vec<GroupId> = Vec::new();
        for units in split.groups.clone() {
            let common =
                crate::groups::common_group(&units, |id| self.object(id).map(|(_, u)| u.group));
            let kept = match common {
                None => None,
                Some(id) => match self.groups.get(id) {
                    None => continue,
                    Some(g) => (g.members.len() == units.len()).then_some(id),
                },
            };
            let id = if let Some(id) = kept {
                self.keep_group(&mut split, id, None)?;
                id
            } else {
                let owner = split.owner;
                let id = self.groups.create(owner).id;
                for &unit in &units {
                    let _ = self.add_to_group(id, unit)?;
                }
                self.group_statistics(id, false)?;
                self.set_initial_position(id)?;
                id
            };
            if listed.len() < LISTED {
                listed.push(id);
            }
            let commands = self.groups.commands;
            let g = self.groups.held_mut(id)?;
            g.differentiator_id = commands;
            let kept = waypoints_from(g.motion.position[0].ground(), waypoints);
            let Some((&last, before)) = kept.split_last() else {
                continue;
            };
            for w in before {
                g.add_waypoint(*w, true);
            }
            let leg = crate::groups::CommandMoveTo {
                range: Distance::ZERO,
                leg: patrol,
                forget_target: true,
                hold: !patrol,
                queued: false,
                direct: true,
                keep_flag: false,
            };
            let _ = self.command_move_to(id, None, last, leg)?;
        }
        for &id in &listed {
            if let Some(g) = self.groups.get_mut(id) {
                set_differentiator_groups(g, &listed);
            }
        }
        self.groups.count_command();
        for i in 0..split.loose.len() {
            self.sweep_unit(&split.loose, i, waypoints, sweep)?;
        }
        Ok(())
    }

    /// `FUN_140fd9f40`'s test of a command on a target: the squared gap between the unit and the
    /// target on the ground (`FUN_14118f8c0`) is above its master's line of sight squared (`COMISS`:
    /// a NaN is not), so the target is out of its sight.
    fn out_of_sight(
        &self,
        unit: &crate::objects::unit::Unit,
        master: &crate::objects::masters::Master,
        target: ObjectId,
    ) -> Result<bool, GenieError> {
        let Some(t) = self.world_object(target) else {
            return Ok(true);
        };
        let tm = self.master(t.player(), t.master_id())?;
        let gap = crate::movement::footprint_gap_squared(
            unit.position(),
            master.radius,
            t.position(),
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
    ) -> Result<(), GenieError> {
        let mut i = 0;
        while i < split.loose.len() {
            let id = *split.loose.checked(i)?;
            let Some((p, u)) = self.object(id) else {
                i += 1;
                continue;
            };
            let master = u.master(&self.masters, p)?;
            if master.kind < COMBAT
                || !master.formation_category.grouped()
                || split.far.contains(&id)
            {
                i += 1;
                continue;
            }
            if let Some(t) = target
                && !self.out_of_sight(u, master, t)?
            {
                i += 1;
                continue;
            }
            let g = self.groups.held(group)?;
            let at = g.motion.position[0];
            let from = u.position().ground();
            let moved = u.body.mover().map_or_else(|| u.position(), |m| m.moved_at);
            let restriction = u.body.mover().map(|m| m.stats.restriction);
            let tiles = self.spatial.tiles();
            if let Some(planner) = restriction.and_then(|r| self.pathing.planner(r, tiles)) {
                let req = crate::pathing::tile_path::Request {
                    from,
                    to: at.ground(),
                    radii: Distance::ZERO,
                    range: Distance::ZERO,
                    closed: Vec::new(),
                    full: false,
                };
                let mut path = crate::pathing::tile_path::Path::default();
                if planner.plan::<GenieError>(&req, false, &mut path, None)?
                    != crate::pathing::tile_path::Found::Goal
                {
                    i += 1;
                    continue;
                }
            }
            // slot 0x520: +0x824 as the game holds it (x, height, y)
            let d = crate::math::fmath::sub(BVector::new(moved.x(), moved.z(), moved.y()), at);
            let influence = self.players.player(p)?.formations.influence.get();
            if d.x() * d.x() + d.height() * d.height() + d.y() * d.y() <= influence * influence {
                let _ = split.loose.remove(i);
                let _ = self.add_to_group(group, id)?;
                if let Some(g) = self.groups.get_mut(group) {
                    g.flags |= GroupFlags::JOINED;
                }
            }
            i += 1;
        }
        Ok(())
    }

    /// The loose unit `loose[i]` of PATROL or `ATTACK_MOVE` (`FUN_14161b970`'s loop over the
    /// splitter's loose units): one the world does not hold takes nothing. An attack move's unit
    /// with a unit AI, `queued` (`+0x9`) or, without `ignore_pending` (`+0xa`), with an order in
    /// its queue that is not ready ([`crate::unit_ai::UnitAi::has_unready_order`]), takes the
    /// order through its queue (slot 0x30: the attack move `0x2dd` at the first waypoint,
    /// priority 100), which is not modelled: the call leaves the record's shown point and ready
    /// flag to the stack ([`ATTACK_MOVE_QUEUED`]). Any other leaves its unit group (`FUN_14102c160` with
    /// −1); a unit that moves (with movement state: type 30 or more) loses its own waypoints
    /// (slot 0x450) and its ignore list (`FUN_141197da0`), waits to move (slot 0x2a0 with 1) and
    /// lists the other loose units (`FUN_14118c790`); an object below type 30 has those slots
    /// empty, and its ignore list (the static part's, which only a walker's short pather reads)
    /// is not held. Of the waypoints its place passes ([`waypoints_from`]) none leaves it there;
    /// else each but the last becomes its own (slot 0x448 at its height) and it moves to the last
    /// (slot 0x340: range 0, forced for a patrol only, walking): of type 70 or 80 through
    /// `FUN_141216c50` ([`World::move_aside`]; type 50 and 60 run `FUN_1410866e0` without its
    /// class test, which only masters of type 70 meet), types 30 and 40 not at all
    /// (`FUN_14106d330`). Its unit AI takes the patrol order (an attack move's `0x2dd`) and the
    /// move action (`FUN_140c64b60` with 1, `FUN_140c64b50`) and no target at the command's last
    /// waypoint at the unit's height (`FUN_140c64be0`), even where the filter left that waypoint
    /// out.
    pub(super) fn sweep_unit(
        &mut self,
        loose: &[ObjectId],
        i: usize,
        waypoints: &[Point],
        sweep: Sweep,
    ) -> Result<(), GenieError> {
        let id = *loose.checked(i)?;
        let Some((p, u)) = self.object(id) else {
            return Ok(());
        };
        if !sweep.patrol
            && u.ai
                .as_ref()
                .is_some_and(|ai| sweep.queued || (!sweep.ignore_pending && ai.has_unready_order()))
        {
            return Err(ATTACK_MOVE_QUEUED);
        }
        let kind = u.master(&self.masters, p)?.kind;
        self.leave_group_of(id, None)?;
        let (_, u) = self.object_mut(id).ok_or_else(|| object_gone(id))?;
        if let Some(m) = u.body.mover_mut() {
            m.clear_user_waypoints();
            m.ignore_units.clear();
            m.waiting = 1;
            for (k, &other) in loose.iter().enumerate() {
                if k != i {
                    ignore_unit(&mut m.ignore_units, other);
                }
            }
        }
        let (x, y, z) = (u.position().x(), u.position().y(), u.position().z());
        let kept = waypoints_from(Point::new(x, y), waypoints);
        let Some((last, before)) = kept.split_last() else {
            return Ok(());
        };
        let (lx, ly) = (last.x(), last.y());
        if let Some(m) = u.body.mover_mut() {
            for w in before {
                m.add_user_waypoint(Position::new(w.x(), w.y(), z));
            }
        }
        // a sleeping object is of a type below 30 (only slot 0x180 puts one to sleep)
        if RGE_COMBAT <= kind
            && let Some(Place::Live(p, idx)) = self.place(id)
        {
            let walk = crate::unit_ai::AiMove {
                point: Position::new(lx, ly, z),
                range: Distance::ZERO,
                force: sweep.patrol,
                walk: true,
            };
            self.move_aside(p, idx, walk)?;
        }
        let unit = match self.place(id) {
            Some(Place::Live(p, i)) => self.players.live_mut(p, i)?,
            Some(Place::Sleeping(p, i)) => self.players.asleep_mut(p, i)?,
            None => return Ok(()),
        };
        let rng = self.streams.get(Stream::Ai);
        if let Some(ai) = unit.ai.as_mut() {
            let shown = waypoints.last().copied().unwrap_or_default();
            ai.set_order(
                Some(if sweep.patrol {
                    OrderKind::Patrol
                } else {
                    OrderKind::AttackMove
                }),
                rng,
            );
            ai.set_action(Some(ActionKind::Move));
            ai.set_target(None, None, shown.with_height(z));
        }
        Ok(())
    }
}
