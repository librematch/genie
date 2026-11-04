//! `FindShortPath`'s rounds (`FUN_140f85580`'s loop): the walks out of the start's hull and into
//! the goal's, then the A* over the hulls' points, again after the area grew.

use crate::GenieResult;
use crate::map::Distance;
use crate::pathing::ShortFound;
use crate::pathing::hull_path::area::AreaGrowth;
use crate::pathing::hull_path::convex::AtStart;
use crate::pathing::hull_path::find::Hulls;
use crate::pathing::hull_path::node_lists::{NewNode, UnrefusedNode};
use crate::pathing::hull_path::walk_in::WalkIn;
use crate::pathing::hull_path::walks::WalkOut;
use crate::pathing::hull_path::{
    EntranceIndex, GROWTHS, HullIndex, HullPather, HullQuery, ITERATION_CAP, MOST, NUDGE,
    NodeIndex, NodeList, PATHS, Point, WalkResult, Waypoints,
};
use crate::world::slots::CheckedIndex;
use std::mem::take;
use std::slice::from_mut;

/// What `FindShortPath`'s rounds carry from one to the next and to the end.
pub(super) struct Rounds {
    /// The nearest a node came to the goal (at first the straight walk).
    nearest_d: Distance,
    /// Whether the search reached the goal or the target.
    pub(super) found: bool,
    /// The goal hull's entrance the found node takes (the game's -1 for none).
    pub(super) entrance: Option<EntranceIndex>,
    /// The nodes expanded, across the rounds.
    iterations: i32,
    /// How often the area grew.
    growths: i32,
    /// Whether the walk within the start's hull (a goal in the same hull) ended partway (its
    /// result 1), its path the one returned.
    pub(super) within: bool,
    /// Whether the walk into the goal's hull ended in a box (its result 5).
    pub(super) entered_box: bool,
    /// The node found, or the one nearest the goal.
    pub(super) best_node: Option<NodeIndex>,
    /// The hull holding the start (`checkPointInsideHull`), if any.
    start_hull: Option<HullIndex>,
    /// The hull holding the goal, if any.
    goal_hull: Option<HullIndex>,
}

impl Rounds {
    /// The rounds before the first: the straight walk as the nearest.
    pub(super) const fn new(straight: Distance) -> Self {
        Self {
            nearest_d: straight,
            found: false,
            entrance: None,
            iterations: 0,
            growths: 0,
            within: false,
            entered_box: false,
            best_node: None,
            start_hull: None,
            goal_hull: None,
        }
    }

    /// A node `d` from the goal: nearer than the nearest so far, it is the best node.
    fn nearer(&mut self, node: NodeIndex, d: Distance) {
        if d < self.nearest_d {
            self.nearest_d = d;
            self.best_node = Some(node);
        }
    }
}

/// What a round's expanded node leads to.
enum Expanded {
    /// The next node.
    Next,
    /// The next round (the area grew, or the nodes ran out for this round).
    NextRound,
    /// The end of the search.
    End,
}

impl HullPather<'_> {
    /// The rounds, while the nodes and the growths last: each starts the open list anew
    /// ([`HullPather::round_start`]), then expands its best node until the search ends or the
    /// area grows. A result where a round's walk within the start's hull ends the search.
    pub(super) fn search(
        &mut self,
        q: &HullQuery,
        hulls: Hulls,
        rounds: &mut Rounds,
        path: &mut Waypoints,
    ) -> GenieResult<Option<ShortFound>> {
        while rounds.iterations < ITERATION_CAP && rounds.growths < GROWTHS {
            if let Some(result) = self.round_start(q, hulls, rounds, path)? {
                return Ok(Some(result));
            }
            loop {
                match self.expand_best(q, rounds)? {
                    Expanded::Next => {}
                    Expanded::NextRound => break,
                    Expanded::End => return Ok(None),
                }
            }
        }
        Ok(None)
    }

    /// A round's start: the open list closed, the hulls built (in the first round of a search
    /// that keeps none), the start's node; a start in a hull walks out of it
    /// ([`HullPather::leave_start_hull`]), a goal in one is walked into
    /// ([`HullPather::enter_goal_hull`]). A result where the walk out ends the search.
    pub(super) fn round_start(
        &mut self,
        q: &HullQuery,
        hulls: Hulls,
        rounds: &mut Rounds,
        path: &mut Waypoints,
    ) -> GenieResult<Option<ShortFound>> {
        self.close_all(NodeList::SEARCH)?;
        if hulls == Hulls::New && rounds.growths == 0 {
            self.build_hulls()?;
        }
        let root = self.add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: true,
            parent: None,
            position: q.from,
            goal: Some(q.to),
            hull: -1,
            direction: -1,
        })?;
        rounds.start_hull = self.hull_of(q.from);
        rounds.goal_hull = self.hull_of(q.to);
        rounds.within = false;
        if let Some(start) = rounds.start_hull
            && let Some(result) = self.leave_start_hull(q, root, start, rounds, path)?
        {
            return Ok(Some(result));
        }
        rounds.entered_box = false;
        if let Some(goal) = rounds.goal_hull {
            rounds.entered_box = self.enter_goal_hull(q, goal)? == WalkResult::WaysFromBox;
        }
        Ok(None)
    }

    /// A start inside hull `sh`: with the goal in the same hull, the walk to it first (`walk_out`
    /// toward the goal into `path`): there (1, or 2 for a target whose id is below 1, the goal
    /// query's among them), none (3, or 3 for a walk that never left the boxes), or partway
    /// (`within`). Then the ways out ([`HullPather::exit_paths`], none is 3), their nodes on the
    /// start's node, which closes.
    pub(super) fn leave_start_hull(
        &mut self,
        q: &HullQuery,
        root: NodeIndex,
        sh: HullIndex,
        rounds: &mut Rounds,
        path: &mut Waypoints,
    ) -> GenieResult<Option<ShortFound>> {
        if Some(sh) == rounds.goal_hull {
            path.points.clear();
            let mut count = 0;
            let walk = self.walk_out(WalkOut {
                hull: sh,
                start: q.from,
                goal: Some(q.to),
                target: q.target,
                range: q.range,
                nudge: false,
                paths: from_mut(path),
                most: 1,
                count: &mut count,
            })?;
            if walk.result == WalkResult::Reached {
                let partial = q.target.is_some() && q.target_id.is_none_or(|t| t.get() < 1);
                return Ok(Some(if partial {
                    ShortFound::Partial
                } else {
                    ShortFound::Reached
                }));
            }
            if walk.result == WalkResult::Stuck || !walk.left_boxes {
                return Ok(Some(ShortFound::Blocked));
            }
            rounds.within = walk.result == WalkResult::Nearest;
        }
        let count = self.exit_paths(sh, q.from, q.range)?;
        if count < 1 {
            return Ok(Some(ShortFound::Blocked));
        }
        self.close(NodeList::SEARCH, root)?;
        self.add_exits(root, count, q.to, sh)?;
        Ok(None)
    }

    /// The start hull's ways out (`walk_out` toward no goal into `exits`, at most
    /// [`PATHS`]): how many. The walk's result is not read, its paths are.
    fn exit_paths(&mut self, hull: HullIndex, from: Point, range: Distance) -> GenieResult<usize> {
        let mut exits = take(&mut self.exits);
        for e in &mut exits {
            e.points.clear();
        }
        let mut count = 0;
        let walk = self.walk_out(WalkOut {
            hull,
            start: from,
            goal: None,
            target: None,
            range,
            nudge: true,
            paths: &mut exits,
            most: PATHS,
            count: &mut count,
        });
        self.exits = exits;
        let _ = walk?;
        Ok(count)
    }

    /// The first `count` ways out as nodes on the start's node `root`: each way's points in turn,
    /// its last open (none refused: closed, or without a hull point); a way of one point adds
    /// none.
    fn add_exits(
        &mut self,
        root: NodeIndex,
        count: usize,
        to: Point,
        start_hull: HullIndex,
    ) -> GenieResult<()> {
        for k in 0..count {
            let points = self.exits.checked(k)?.points.clone();
            let n = points.len();
            if n <= 1 {
                continue;
            }
            let mut node = root;
            for &p in points.iter().take(n - 1).skip(1) {
                node = self.add_unrefused(UnrefusedNode {
                    list: NodeList::SEARCH,
                    open: false,
                    parent: Some(node),
                    position: p,
                    goal: Some(to),
                    hull: start_hull.slot(),
                    direction: -1,
                })?;
            }
            let last = *points.checked(n - 1)?;
            let _ = self.add_unrefused(UnrefusedNode {
                list: NodeList::SEARCH,
                open: true,
                parent: Some(node),
                position: last,
                goal: Some(to),
                hull: start_hull.slot(),
                direction: -1,
            })?;
        }
        Ok(())
    }

    /// A goal inside the hull `hull`: the ways in (`walk_in` backwards from the goal into
    /// `entrances`, at most [`PATHS`]), their ends joined to the hull's points
    /// ([`HullPather::join_entrances`]). The walk's result (5 when it ended in a box).
    fn enter_goal_hull(&mut self, q: &HullQuery, hull: HullIndex) -> GenieResult<WalkResult> {
        let mut entrances = take(&mut self.entrances);
        for e in &mut entrances {
            e.points.clear();
        }
        let n = self.hulls.checked(hull.index())?.points.len();
        self.hulls.checked_mut(hull.index())?.entrances = vec![None; n];
        let mut count = 0;
        let mut sides = [0usize; PATHS];
        let walk = self.walk_in(WalkIn {
            hull,
            goal: q.to,
            start: q.from,
            target: q.target,
            paths: &mut entrances,
            sides_out: &mut sides,
            most: PATHS,
            count: &mut count,
        });
        self.entrances = entrances;
        let result = walk?.result;
        self.join_entrances(hull, count, &sides)?;
        Ok(result)
    }

    /// One node of the search, the open list's best: beyond the nodes' cap the next round; an
    /// open list with a node off the map closes the best; a best beyond the area grows it (the
    /// next round) or ends the search. A best within range ends it found; else its walk to the
    /// goal crosses the nearest hull ([`HullPather::around_hull`]) or none
    /// ([`HullPather::no_crossing`]).
    fn expand_best(&mut self, q: &HullQuery, rounds: &mut Rounds) -> GenieResult<Expanded> {
        if ITERATION_CAP <= rounds.iterations {
            return Ok(Expanded::NextRound);
        }
        rounds.iterations += 1;
        let Some(best) = self.nodes.checked(NodeList::SEARCH.head())?.next else {
            return Ok(Expanded::End);
        };
        if self.open_off_map(best)? {
            self.close(NodeList::SEARCH, best)?;
            return Ok(Expanded::Next);
        }
        if !self.in_area(self.nodes.checked(best)?.at) {
            if self.grow_area()? == AreaGrowth::TooLarge {
                return Ok(Expanded::End);
            }
            rounds.growths += 1;
            return Ok(Expanded::NextRound);
        }
        self.close(NodeList::SEARCH, best)?;
        let at = self.nodes.checked(best)?.at;
        if self.reached(q, at) {
            rounds.found = q.target.is_none() || q.target_id.is_some_and(|t| !t.is_recyclable());
            rounds.best_node = Some(best);
            return Ok(Expanded::End);
        }
        rounds.nearer(best, self.nodes.checked(best)?.to_goal);
        let vertex = self.nodes.checked(best)?.vertex;
        let Some((hit, p, edge)) = self.nearest_crossing(vertex, at, q.to) else {
            return self.no_crossing(q, best, rounds);
        };
        if rounds.start_hull.is_none() && rounds.goal_hull.is_none() && self.blocked(at, p)? {
            return Ok(Expanded::Next);
        }
        if hit.slot() == self.nodes.checked(best)?.hull && 0 <= vertex {
            return Ok(Expanded::Next);
        }
        let node = self.add_unrefused(UnrefusedNode {
            list: NodeList::SEARCH,
            open: false,
            parent: Some(best),
            position: p,
            goal: Some(q.to),
            hull: hit.slot(),
            direction: -1,
        })?;
        rounds.nearer(node, self.nodes.checked(node)?.to_goal);
        self.around_hull(node, hit, edge, q.to)?;
        Ok(Expanded::Next)
    }

    /// Whether a node of the open list from `best` on lies off the map.
    fn open_off_map(&self, best: NodeIndex) -> GenieResult<bool> {
        let mut n = Some(best);
        while let Some(i) = n {
            let node = self.nodes.checked(i)?;
            if !self.on_map(node.at) {
                return Ok(true);
            }
            n = node.next;
        }
        Ok(false)
    }

    /// Whether `at` is within range of the target's box, or without one of the goal: the radius
    /// and the range, squared, [`NUDGE`] more (`COMISS`/`JNC` at 0x140f86cff: a NaN is not).
    fn reached(&self, q: &HullQuery, at: Point) -> bool {
        let (to, r) = (q.to, self.radius);
        q.target.as_ref().map_or_else(
            || {
                let d =
                    (at.y() - to.y()) * (at.y() - to.y()) + (at.x() - to.x()) * (at.x() - to.x());
                d <= (q.range + r).squared().get() + NUDGE
            },
            |t| self.target_gap(t, at) <= q.range + Distance::new(NUDGE),
        )
    }

    /// The hull the walk from `at` to `to` crosses first: its index, the crossing and the edge
    /// crossed. A node off the hulls' points (`vertex` below 0) tests from outside.
    fn nearest_crossing(
        &self,
        vertex: i32,
        at: Point,
        to: Point,
    ) -> Option<(HullIndex, Point, usize)> {
        let hull_path_tuning = &self.by_id.tuning().hull_path;
        let mut crossing: Option<(HullIndex, Point, usize)> = None;
        let mut crossing_d = MOST;
        let start = if vertex < 0 {
            AtStart::Counts
        } else {
            AtStart::Ignored
        };
        for (i, hull) in self.hulls.iter().enumerate() {
            if let Some((p, edge)) = hull.intersection(start, at, to, hull_path_tuning) {
                let d = (p.y() - at.y()) * (p.y() - at.y()) + (p.x() - at.x()) * (p.x() - at.x());
                if d < crossing_d {
                    crossing = Some((HullIndex::new(i), p, edge));
                    crossing_d = d;
                }
            }
        }
        crossing
    }

    /// A best node whose walk to the goal crosses no hull: an unblocked walk ends the search at
    /// the goal's node; a node on a point of the goal's hull with an entrance ends it there.
    fn no_crossing(
        &mut self,
        q: &HullQuery,
        best: NodeIndex,
        rounds: &mut Rounds,
    ) -> GenieResult<Expanded> {
        let (at, vertex) = (
            self.nodes.checked(best)?.at,
            self.nodes.checked(best)?.vertex,
        );
        if !self.blocked(at, q.to)? {
            rounds.best_node = self.add_node(NewNode {
                list: NodeList::SEARCH,
                open: false,
                parent: Some(best),
                position: q.to,
                goal: Some(q.to),
                hull: -1,
                vertex: -1,
                direction: -1,
                by_place: false,
            })?;
            rounds.found = true;
            return Ok(Expanded::End);
        }
        if let Some(goal) = rounds.goal_hull
            && goal.slot() == self.nodes.checked(best)?.hull
            && 0 <= vertex
        {
            let hull = self.hulls.checked(goal.index())?;
            if let Some(&Some(e)) = hull.entrances.get(vertex as usize)
                && (vertex as usize) < hull.points.len()
            {
                rounds.found = true;
                rounds.best_node = Some(best);
                rounds.entrance = Some(e);
                return Ok(Expanded::End);
            }
        }
        Ok(Expanded::Next)
    }

    /// The crossed hull `hit`'s points as nodes from the crossing's node, both ways around from
    /// the crossed `edge` (back from its start, on from its end), half of them each; a hull of
    /// an odd count adds its last point on the cheaper way (the forward way on a tie).
    pub(super) fn around_hull(
        &mut self,
        node: NodeIndex,
        hit: HullIndex,
        edge: usize,
        to: Point,
    ) -> GenieResult<()> {
        let np = self.hulls.checked(hit.index())?.points.len();
        let mut back = if edge >= 1 { edge } else { np } as i32 - 1;
        let mut fwd = edge;
        let (mut a, mut b) = (Some(node), Some(node));
        for _ in 0..np / 2 {
            if let Some(x) = a {
                let p = *self
                    .hulls
                    .checked(hit.index())?
                    .points
                    .checked(back as usize)?;
                a = self.add_node(NewNode {
                    list: NodeList::SEARCH,
                    open: true,
                    parent: Some(x),
                    position: p,
                    goal: Some(to),
                    hull: hit.slot(),
                    vertex: back,
                    direction: -1,
                    by_place: false,
                })?;
            }
            if let Some(x) = b {
                let p = *self.hulls.checked(hit.index())?.points.checked(fwd)?;
                b = self.add_node(NewNode {
                    list: NodeList::SEARCH,
                    open: true,
                    parent: Some(x),
                    position: p,
                    goal: Some(to),
                    hull: hit.slot(),
                    vertex: fwd as i32,
                    direction: -1,
                    by_place: false,
                })?;
            }
            back -= 1;
            if back < 0 {
                back = np as i32 - 1;
            }
            fwd = if fwd + 1 < np { fwd + 1 } else { 0 };
        }
        if np % 2 == 1 {
            self.odd_point(hit, (a, back), (b, fwd), to)?;
        }
        Ok(())
    }

    /// A hull of an odd count's last point, `back` on the back way's node `a` or `fwd` on the
    /// forward way's `b`: on `a` when it is cheaper than `b` or `b` is gone, else on `b`; none
    /// when both ways are gone.
    fn odd_point(
        &mut self,
        hit: HullIndex,
        (a, back): (Option<NodeIndex>, i32),
        (b, fwd): (Option<NodeIndex>, usize),
        to: Point,
    ) -> GenieResult<()> {
        let (qa, qb) = (
            *self
                .hulls
                .checked(hit.index())?
                .points
                .checked(back as usize)?,
            *self.hulls.checked(hit.index())?.points.checked(fwd)?,
        );
        let on_a = match (a, b) {
            (Some(x), Some(y)) => {
                (self.nodes.checked(x)?.cost < self.nodes.checked(y)?.cost).then_some(x)
            }
            (Some(x), None) => Some(x),
            (None, _) => None,
        };
        if let Some(x) = on_a {
            let _ = self.add_node(NewNode {
                list: NodeList::SEARCH,
                open: true,
                parent: Some(x),
                position: qa,
                goal: Some(to),
                hull: hit.slot(),
                vertex: back,
                direction: -1,
                by_place: false,
            })?;
        } else if let Some(y) = b {
            let _ = self.add_node(NewNode {
                list: NodeList::SEARCH,
                open: true,
                parent: Some(y),
                position: qb,
                goal: Some(to),
                hull: hit.slot(),
                vertex: fwd as i32,
                direction: -1,
                by_place: false,
            })?;
        }
        Ok(())
    }
}
