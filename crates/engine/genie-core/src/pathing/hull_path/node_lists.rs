//! The node lists: a node added, closed, costed and linked; the manager's line tests; the path
//! built back from its end.

use crate::GenieResult;
use crate::map::Distance;
use crate::map::spatial::TileRecords;
use crate::map::spatial::{Hit, RecordId};
use crate::objects::ObjectId;
use crate::pathing::hull_path::{
    EPSILON, HullPather, LINE_HITS, Node, NodeIndex, NodeList, Point, Waypoints, walk_length,
};
use crate::world::slots::CheckedIndex;

/// A node `MShortPather::addNode` may refuse ([`HullPather::add_node`]'s arguments).
#[derive(Clone, Copy, Debug)]
pub(super) struct NewNode {
    /// The list.
    pub(super) list: NodeList,
    /// Whether it goes into the open list (else straight into the closed one).
    pub(super) open: bool,
    /// The node it comes from.
    pub(super) parent: Option<NodeIndex>,
    /// Its place.
    pub(super) position: Point,
    /// The goal its distance is to (none: 0).
    pub(super) goal: Option<Point>,
    /// Its hull (−1 none).
    pub(super) hull: i32,
    /// Its hull vertex (−1 none).
    pub(super) vertex: i32,
    /// The direction it walks (−1 none).
    pub(super) direction: i32,
    /// Whether an equal node is one within [`EPSILON`] of the place, not one of the same hull
    /// point.
    pub(super) by_place: bool,
}

/// A node `addNode` cannot refuse ([`HullPather::add_unrefused`]'s arguments).
#[derive(Clone, Copy, Debug)]
pub(super) struct UnrefusedNode {
    /// The list.
    pub(super) list: NodeList,
    /// Whether it goes into the open list (else straight into the closed one).
    pub(super) open: bool,
    /// The node it comes from.
    pub(super) parent: Option<NodeIndex>,
    /// Its place.
    pub(super) position: Point,
    /// The goal its distance is to (none: 0).
    pub(super) goal: Option<Point>,
    /// Its hull (−1 none).
    pub(super) hull: i32,
    /// The direction it walks (−1 none).
    pub(super) direction: i32,
}

impl HullPather<'_> {
    // The node lists.

    /// `MShortPather::emptyLists` (`FUN_140f8c300`): list `node_list`'s nodes go.
    pub(super) fn empty(&mut self, node_list: NodeList) -> GenieResult<()> {
        self.nodes.checked_mut(node_list.head())?.next = None;
        self.closed.of_mut(node_list).clear();
        Ok(())
    }

    /// `MShortPather::closeAllOpenNodes` (`FUN_140f8c130`): list `node_list`'s open nodes close,
    /// and every closed node forgets its hull point.
    pub(super) fn close_all(&mut self, node_list: NodeList) -> GenieResult<()> {
        let mut checked = self.nodes.checked(node_list.head())?.next;
        while let Some(i) = checked {
            self.closed.of_mut(node_list).push(i);
            checked = self.nodes.checked(i)?.next;
        }
        self.nodes.checked_mut(node_list.head())?.next = None;
        for &node_index in self.closed.of(node_list) {
            self.nodes.checked_mut(node_index)?.hull = -1;
            self.nodes.checked_mut(node_index)?.vertex = -1;
        }
        Ok(())
    }

    /// `MShortPather::moveToClosedList` (`FUN_140f8ce00`).
    pub(super) fn close(&mut self, node_list: NodeList, closed: NodeIndex) -> GenieResult<()> {
        let (prev, next) = (
            self.nodes.checked(closed)?.prev,
            self.nodes.checked(closed)?.next,
        );
        if let Some(p) = prev {
            self.nodes.checked_mut(p)?.next = next;
        }
        if let Some(node_index) = next {
            self.nodes.checked_mut(node_index)?.prev = prev;
        }
        self.closed.of_mut(node_list).push(closed);
        Ok(())
    }

    /// `MShortPather::addNode` (`FUN_140f8aa10`): a node at `at` from `parent` (its cost the
    /// parent's plus the step; its distance to `goal`, 0 without), on list `node_list`: into the
    /// open list in order of cost plus distance, or straight into the closed list. An open node
    /// replaces an open one of the same hull point (`by_place`: within [`EPSILON`] of the same
    /// place) that costs more, which closes; it is not made when such a node costs no more, nor
    /// when a closed node does (`by_place`: of the closed list but its last).
    pub(super) fn add_node(&mut self, new: NewNode) -> GenieResult<Option<NodeIndex>> {
        let NewNode {
            list: node_list,
            open,
            parent,
            position: at,
            goal,
            hull,
            vertex,
            direction,
            by_place,
        } = new;
        let (cost, to_goal) = self.node_costs(parent, at, goal)?;
        let mut after: Option<NodeIndex> = None;
        if open && (by_place || (0 <= hull && 0 <= vertex)) {
            let same = |n: &Node| {
                if by_place {
                    (at.x() - n.at.x()).abs() < EPSILON && (at.y() - n.at.y()).abs() < EPSILON
                } else {
                    n.hull == hull && n.vertex == vertex
                }
            };
            let mut checked = self.nodes.checked(node_list.head())?.next;
            let mut replaced = false;
            while let Some(i) = checked {
                let node = *self.nodes.checked(i)?;
                if same(&node) {
                    if node.cost <= cost {
                        return Ok(None);
                    }
                    // the worse node closes, and the closed list is not searched
                    self.close(node_list, i)?;
                    replaced = true;
                    break;
                }
                if after.is_none() && to_goal + cost < node.cost + node.to_goal {
                    after = node.prev;
                }
                checked = node.next;
            }
            // the place test leaves the closed list's last node out
            let all = self.closed.of(node_list);
            let closed = if by_place {
                all.head(all.len().saturating_sub(1))?
            } else {
                all.as_slice()
            };
            if !replaced
                && closed.iter().any(|&node_index| {
                    self.nodes
                        .get(node_index)
                        .is_some_and(|node| same(node) && node.cost <= cost)
                })
            {
                return Ok(None);
            }
        }
        Ok(Some(self.link_node(
            node_list,
            open,
            after,
            Node {
                at,
                parent,
                next: None,
                prev: None,
                to_goal,
                cost,
                hull,
                vertex,
                direction,
            },
        )?))
    }

    /// `addNode` for a node nothing can refuse: one straight into the closed list, or an open one
    /// without a hull point and not by place, for which `addNode` searches no list. The game's
    /// callers test its answer for null, which then catches only a failed allocation (the roots
    /// of `FindShortPath` and `pathWithinHull`, `FindShortPath`'s crossing node).
    pub(super) fn add_unrefused(&mut self, new: UnrefusedNode) -> GenieResult<NodeIndex> {
        let UnrefusedNode {
            list: node_list,
            open,
            parent,
            position: at,
            goal,
            hull,
            direction,
        } = new;
        let (cost, to_goal) = self.node_costs(parent, at, goal)?;
        self.link_node(
            node_list,
            open,
            None,
            Node {
                at,
                parent,
                next: None,
                prev: None,
                to_goal,
                cost,
                hull,
                vertex: -1,
                direction,
            },
        )
    }

    /// A node's cost (the parent's plus the step, 0 without a parent) and its distance to `goal`
    /// (0 without one), as `addNode` reckons them.
    pub(super) fn node_costs(
        &self,
        parent: Option<NodeIndex>,
        at: Point,
        goal: Option<Point>,
    ) -> GenieResult<(Distance, Distance)> {
        let cost = match parent {
            Some(node_index) => {
                let node = *self.nodes.checked(node_index)?;
                walk_length(node.at, at) + node.cost
            }
            None => Distance::ZERO,
        };
        let to_goal = goal.map_or(Distance::ZERO, |point| walk_length(at, point));
        Ok((cost, to_goal))
    }

    /// `addNode`'s end: the node into the closed list, or into the open list after `after` or,
    /// without, after the open nodes whose cost plus distance is smaller.
    pub(super) fn link_node(
        &mut self,
        node_list: NodeList,
        open: bool,
        after: Option<NodeIndex>,
        node: Node,
    ) -> GenieResult<NodeIndex> {
        let (cost, to_goal) = (node.cost, node.to_goal);
        let pushed = self.nodes.push(node);
        if !open {
            self.closed.of_mut(node_list).push(pushed);
            return Ok(pushed);
        }
        let mut s = after.unwrap_or_else(|| node_list.head());
        if after.is_none() {
            while let Some(n) = self.nodes.checked(s)?.next {
                if self.nodes.checked(n)?.cost + self.nodes.checked(n)?.to_goal < to_goal + cost {
                    s = n;
                } else {
                    break;
                }
            }
        }
        let next = self.nodes.checked(s)?.next;
        self.nodes.checked_mut(pushed)?.next = next;
        self.nodes.checked_mut(s)?.next = Some(pushed);
        self.nodes.checked_mut(pushed)?.prev = Some(s);
        if let Some(node_index) = next {
            self.nodes.checked_mut(node_index)?.prev = Some(pushed);
        }
        Ok(pushed)
    }

    // The world's tests.

    /// The manager's line test (`FUN_141052b90`, then `FUN_141052350` with flags 0x40) with the
    /// pather's buffer, ignore list and include list: Spatial's pather query, which leaves every
    /// moving record out, or an included one that moves ([`HullPather::included_movers`]).
    pub(super) fn blocked(&mut self, from: Point, to: Point) -> GenieResult<bool> {
        if self
            .world
            .pather_blocked(from, to, self.buffer, self.mask, &self.ignore, self.by_id)?
        {
            return Ok(true);
        }
        Ok(!self.included_movers(from, to)?.is_empty())
    }

    /// The records the walk `a` -> `to` meets, at most [`LINE_HITS`] (`FUN_141052b90`, then
    /// `FUN_141051900` with flags 0x40), with the pather's buffer, ignore list and include list:
    /// Spatial's pather query and the included objects that move
    /// ([`HullPather::included_movers`]). The callers read how many and which, not their order.
    pub(super) fn hits(&mut self, a: Point, to: Point) -> GenieResult<Vec<Hit>> {
        let mut hits =
            self.world
                .pather_hits(a, to, self.buffer, self.mask, &self.ignore, self.by_id)?;
        for id in self.included_movers(a, to)? {
            if let Some(placed) = self.world.get(id) {
                hits.push(Hit {
                    id: RecordId::from(id),
                    bounds: placed.bounds(),
                });
            }
        }
        hits.truncate(LINE_HITS);
        Ok(hits)
    }

    /// The included objects (`FindShortPath`'s arguments 13 and 14) that move, but the ignored
    /// ones, whose records the walk `a` -> `to` meets: the line tests leave a moving record (its
    /// `+3` bit 0x10) out only when the include list does not hold it (`FUN_141051e00`, before
    /// the ignore list). Spatial's pather query leaves every moving record out; its line query
    /// (flags 0x10) tests the same records alike, the buffer the grow, and leaves none out, at
    /// most [`LINE_HITS`] a call: what it lists goes into its ignore list until a call lists
    /// fewer.
    pub(super) fn included_movers(&self, a: Point, to: Point) -> GenieResult<Vec<ObjectId>> {
        let movers: Vec<ObjectId> = self
            .include
            .iter()
            .copied()
            .filter(|id| !self.ignore.contains(&RecordId::from(*id)) && self.by_id.moving(*id))
            .collect();
        let mut met = Vec::new();
        if movers.is_empty() {
            return Ok(met);
        }
        let grow = self.buffer.unwrap_or(Distance::ZERO);
        let mut skip = self.ignore.clone();
        loop {
            let hits = self
                .world
                .line_hits(a, to, grow, TileRecords::Passed, &skip)?;
            let full = LINE_HITS <= hits.len();
            for id in hits {
                if let Some(object_id) = id.object()
                    && movers.contains(&object_id)
                    && !met.contains(&object_id)
                {
                    met.push(object_id);
                }
                if !skip.contains(&id) {
                    skip.push(id);
                }
            }
            if !full || met.len() == movers.len() {
                return Ok(met);
            }
        }
    }

    /// `MShortPather::buildPath` (`FUN_140f8bcd0`): from `end` back, a node that sees its
    /// grandparent skips its parent; then the nodes from the first into `path`.
    pub(super) fn build_path(&mut self, path: &mut Waypoints, end: NodeIndex) -> GenieResult<()> {
        let mut cur = Some(end);
        while let Some(c) = cur {
            let Some(p) = self.nodes.checked(c)?.parent else {
                break;
            };
            let Some(gp) = self.nodes.checked(p)?.parent else {
                break;
            };
            if self.blocked(self.nodes.checked(c)?.at, self.nodes.checked(gp)?.at)? {
                cur = Some(p);
            } else {
                self.nodes.checked_mut(c)?.parent = Some(gp);
            }
        }
        let mut next = Some(end);
        while let Some(node_index) = next {
            path.insert(0, self.nodes.checked(node_index)?.at);
            next = self.nodes.checked(node_index)?.parent;
        }
        Ok(())
    }
}
