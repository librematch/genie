//! A pasture's livestock: their setup (`FUN_1411c8410`, `SetupPasture`) and their food, which is
//! the pasture's own. The completion's first part (`FUN_1411c6bd0`) of a pasture (masters 1893 and
//! 1897, class 49) whose build points reached its build time puts its owner's livestock on points
//! along its edges, each holding its share of the pasture's food. At the end of each of its
//! updates the pasture holds nothing, and its Make's food still to make is what its livestock
//! holds ([`World::sum_livestock_food`]).

use crate::construction::build::world_side::MakeMode;
use crate::construction::building_update::walls_branch;
use crate::gathering::PastureId;
use crate::map::Distance;
use crate::map::Point;
use crate::map::Position;
use crate::map::Tile;
use crate::math::conversions::cvttss2si;
use crate::math::msvc::sort;
use crate::math::rng::Stream;
use crate::movement::footprint_gap_squared;
use crate::objects::ListIndex;
use crate::objects::ObjectId;
use crate::objects::ObjectKey;
use crate::objects::SpriteFlags;
use crate::objects::WorkerCount;
use crate::objects::masters::MasterId;
use crate::objects::masters::MasterKind;
use crate::objects::unit::UnitFlags;
use crate::players::Amount;
use crate::players::PlayerId;
use crate::technology::Attributes;
use crate::world::World;
use crate::world::layer::MasterRoles;
use crate::world::layer::MasterSlots;
use crate::world::layer::generated::PastureTuning;
use crate::world::objects_ref::ObjectsRef;
use crate::world::slots::CheckedIndexExt;
use crate::world::slots::PlayerSlotsExt;
use crate::{GenieError, GenieResult};

/// The livestock a pasture holds (`DAT_143f51db0`, which the static constructor `FUN_140174900`
/// fills), in the table's order a draw picks from: the Argali and the Ibex (Pasture), the
/// build's `pasture_argali` and `pasture_ibex`.
#[must_use]
pub const fn livestock(masters: MasterSlots) -> [MasterId; 2] {
    [masters.pasture_argali, masters.pasture_ibex]
}

/// The points along the edges of a pasture at `at` whose ground radius on x is `radius_x`, in the
/// routine's order: the radius less `PastureTuning::inset` gives the corners (x−r, y−r), (x+r,
/// y−r), (x+r, y+r) and (x−r, y+r); each edge from its first corner to its second, `(b − a) ·
/// fraction + a` in f32 (SUBSS, MULSS, ADDSS) with `fraction` from 0 by `PastureTuning::step` while
/// at most `PastureTuning::end`: the bottom edge from the first corner to the second, the right one
/// from the third to the second, the top one from the third to the fourth and the left one from the
/// first to the fourth.
#[must_use]
pub fn edge_points(at: Position, radius_x: Distance, pasture_tuning: &PastureTuning) -> Vec<Point> {
    let distance = radius_x - Distance::new(pasture_tuning.inset);
    let (x, y) = (at.x(), at.y());
    let points = [
        Point::new(x - distance.get(), y - distance.get()),
        Point::new(x + distance.get(), y - distance.get()),
        Point::new(x + distance.get(), y + distance.get()),
        Point::new(x - distance.get(), y + distance.get()),
    ];
    let edges = [
        (points[0], points[1]),
        (points[2], points[1]),
        (points[2], points[3]),
        (points[0], points[3]),
    ];
    let mut points = Vec::new();
    for (start, end) in edges {
        let mut fraction = 0.0f32;
        // COMISS END, t with JNC: on while END is at least t
        while fraction <= pasture_tuning.end {
            points.push(Point::new(
                (end.x() - start.x()) * fraction + start.x(),
                (end.y() - start.y()) * fraction + start.y(),
            ));
            fraction += pasture_tuning.step;
        }
    }
    points
}

/// A draw of the Object stream modulo `modulus` (`CDQE` then `DIV`: the draw is at most 0x7fff).
fn draw_below(turns: &mut World, modulus: usize) -> usize {
    turns.streams.get_mut(Stream::Object).rand15() as usize % modulus
}

impl World {
    /// `FUN_1411c8410` on player `player_id`'s pasture `id`: the points along its edges
    /// ([`edge_points`] with its master's ground radius on x), its owner's
    /// [`AttributeSlots::pasture_livestock`](crate::world::layer::AttributeSlots::pasture_livestock)
    /// cut to an integer (CVTTSS2SI) as the count and
    /// [`AttributeSlots::pasture_food`](crate::world::layer::AttributeSlots::pasture_food) over it
    /// (DIVSS) as each one's food. Up to the count, while points are left: a draw of the Object
    /// stream picks the master ([`livestock`], `SetupPasture` line 0x1fa4), a second the point
    /// (0x1fa6). A point off the map (`FUN_140f5ec30`) or on a tile whose objects (its list,
    /// `+0x28`) hold a livestock master of this pasture (`+0x790`) goes, and a draw picks another
    /// (0x1fb2); none left ends it. On the point the owner's slot 0x1c0 makes the master at the
    /// pasture's height ([`World::make_new_object`], no footprint): the point goes, the pasture
    /// lists it ([`crate::objects::layers::Building::livestock`]), its state is 2 (slot 0xa0 with
    /// 2: it is already), no unit targets it (`+0x689` 0,
    /// [`crate::objects::unit::UnitFlags::UNTARGETABLE`]), it is the pasture's (`+0x790`) and holds
    /// its food (`+0x310`); its current sprite with a random first facet draws a facet below its
    /// facets less one (0x1fd2, IDIV). Each attempt counts, a master the owner has not got too. Not
    /// held: its selection flag (`+0x68b` 0, slot 0x6b0) and the drawn facet (`+0xe`,
    /// `FUN_14119d490`: the moving set-sprite derives it from the angle again, `FUN_141158620` ->
    /// `FUN_14114d6c0`, its dying sprite's included).
    ///
    /// # Errors
    ///
    /// Its master or attributes the player has not got, a facet draw on a sprite of one facet
    /// (the game divides by zero), and what the object's making does not model.
    pub(crate) fn setup_pasture(&mut self, player_id: PlayerId, id: PastureId) -> GenieResult<()> {
        let (at, master) = self
            .object(id)
            .map(|(_, unit)| (unit.position(), unit.life.master_id))
            .ok_or(GenieError::unreachable("a pasture its completion names"))?;
        let radius_x = self.master(player_id, master)?.radius.x();
        let mut points = edge_points(at, Distance::new(radius_x), &self.model.tuning.pasture);
        let owner = self.players.player(player_id)?;
        let count =
            cvttss2si(Attributes::get(owner, self.model.slots.attribute.pasture_livestock)?.get());
        let food = Attributes::get(owner, self.model.slots.attribute.pasture_food)? / count as f32;
        let map = self.spatial.map;
        let mut made = 0;
        while made < count && !points.is_empty() {
            let kinds = livestock(self.model.slots.master);
            let kind = *kinds.checked(draw_below(self, kinds.len()))?;
            let mut index = draw_below(self, points.len());
            loop {
                let (x, y) = {
                    let checked = points.checked(index)?;
                    (checked.x(), checked.y())
                };
                let (tx, ty) = (cvttss2si(x), cvttss2si(y));
                let on_map = 0 <= tx && tx < map.width() && 0 <= ty && ty < map.height();
                if on_map && !self.holds_livestock_of(id, Tile::new(tx, ty)) {
                    break;
                }
                let _ = points.remove(index);
                if points.is_empty() {
                    return Ok(());
                }
                index = draw_below(self, points.len());
            }
            let (x, y) = {
                let point = points.checked(index)?;
                (point.x(), point.y())
            };
            if let Some(new) = self.make_new_object(
                player_id,
                kind,
                Position::new(x, y, at.z()),
                MakeMode::Plain,
            )? {
                let _ = points.remove(index);
                if let Some((_, layer)) = self.object_and_layer_mut(id)? {
                    layer.livestock.push(new);
                }
                // `make_new_object` files the object it answers
                let (_, unit) = self
                    .object_mut(new)
                    .ok_or(GenieError::unreachable("the livestock just made"))?;
                unit.flags.set(UnitFlags::UNTARGETABLE, true);
                unit.pasture = Some(id);
                unit.life.amount = food;
                let graphic = unit.sprite.map(|active_sprite| active_sprite.graphic);
                if let Some(graphic) = graphic
                    .filter(|graphic| graphic.flags.contains(SpriteFlags::RANDOM_FIRST_FRAME))
                {
                    let draw = self.streams.get_mut(Stream::Object).rand15() as i32;
                    // the `.dat` can give the livestock such a sprite: the game's `IDIV` by 0
                    // stops it
                    let _ = draw
                        .checked_rem(graphic.facets.counted() - 1)
                        .ok_or_else(|| {
                            GenieError::invalid_input(
                                "a random first facet of a livestock sprite of one facet",
                            )
                            .with("sprite", graphic.id)
                        })?;
                }
            }
            made += 1;
        }
        Ok(())
    }

    /// `FUN_1411c7670`: the livestock of pasture `pasture` that player `player`'s unit at `idx`
    /// tends, with its place, −1 for none (a unit of another player, too): its livestock the world
    /// holds with their workers and their footprints' squared gap to the unit (`FUN_14118f8c0`),
    /// sorted by the gap (`std::sort`, [`crate::math::msvc::sort`]); the first without workers,
    /// else the first whose workers are below the owner's
    /// [`AttributeSlots::pasture_workers`](crate::world::layer::AttributeSlots::pasture_workers).
    ///
    /// # Errors
    ///
    /// A master the players have not got, or an attribute the owner has not got.
    pub(crate) fn pasture_pick(
        &self,
        pasture: PastureId,
        player: PlayerId,
        idx: ListIndex,
    ) -> GenieResult<Option<(ObjectId, Position)>> {
        let Some((pasture_player_id, pasture_unit)) = self.object(pasture) else {
            return Ok(None);
        };
        if pasture_player_id != player {
            return Ok(None);
        }
        let unit = &self.players.live(player, idx)?;
        let reach = unit.master(self.master_tables(), player)?.radius;
        let mut tended = Vec::new();
        for &id in &self.parts.building_of(pasture_unit.id).livestock {
            let Some((player_id, livestock)) = self.object(id) else {
                continue;
            };
            let radius = livestock.master(self.master_tables(), player_id)?.radius;
            let gap = footprint_gap_squared(livestock.position(), radius, unit.position(), reach);
            tended.push((id, livestock.workers, gap, livestock.position()));
        }
        sort(&mut tended, &|left, right| left.2 < right.2)?;
        let room = Attributes::get(
            self.players.player(pasture_player_id)?,
            self.model.slots.attribute.pasture_workers,
        )?;
        let mut pick = None;
        for (id, workers, _, at) in tended {
            if workers == WorkerCount::ZERO {
                return Ok(Some((id, at)));
            }
            if pick.is_none() && workers.as_f32() < room.get() {
                pick = Some((id, at));
            }
        }
        Ok(pick)
    }

    /// `FUN_141189ca0`: player `player`'s unit at `idx` tends `pasture`, none (−1) to tend none.
    /// The building it names (the one the unit tends, `+0x794`, for none; none for an object that
    /// is no building) takes the unit last among its workers unless it lists it already, or for
    /// none drops it (`FUN_1411cafa0`, [`crate::objects::layers::Building::pasture_workers`]); then
    /// the unit tends `pasture` ([`crate::objects::unit::Unit::my_pasture`]).
    ///
    /// # Errors
    ///
    /// A master the players have not got.
    pub(crate) fn tend_pasture(
        &mut self,
        player: PlayerId,
        idx: ListIndex,
        pasture: Option<PastureId>,
    ) -> GenieResult<()> {
        let unit = &self.players.live(player, idx)?;
        let (id, named) = (unit.id, pasture.or(unit.my_pasture));
        let building = match self.object(named) {
            Some((player_id, unit)) => {
                unit.master(self.master_tables(), player_id)?.kind == MasterKind::BUILDING
            }
            None => false,
        };
        if building && let Some((_, layer)) = self.object_and_layer_mut(named)? {
            if pasture.is_none() {
                layer.pasture_workers.retain(|&object_id| object_id != id);
            } else if !layer.pasture_workers.contains(&id) {
                layer.pasture_workers.push(id);
            }
        }
        self.players.live_mut(player, idx)?.my_pasture = pasture;
        Ok(())
    }

    /// Whether the objects on `tile` (its list, `+0x28`) hold a livestock master ([`livestock`])
    /// of the pasture `pasture` (`+0x790`). A ghost on it is not of a pasture.
    fn holds_livestock_of(&self, pasture: PastureId, tile: Tile) -> bool {
        let kinds = livestock(self.model.slots.master);
        self.spatial.tile_list(tile).iter().any(|&object_id| {
            self.object(object_id).is_some_and(|(_, unit)| {
                unit.pasture == Some(pasture) && kinds.contains(&unit.life.master_id)
            })
        })
    }
}

impl ObjectsRef<'_> {
    /// A pasture's slot 0x780 (`FUN_1411c5dd0`): the workers (`+0x644`) of its livestock
    /// ([`crate::objects::layers::Building::livestock`]) the world holds, summed; 0 for an object
    /// the world does not hold.
    #[must_use]
    pub(crate) fn livestock_workers(&self, pasture: impl ObjectKey) -> WorkerCount {
        self.building(pasture)
            .map_or(WorkerCount::ZERO, |building| {
                building
                    .livestock
                    .iter()
                    .filter_map(|&id| self.get(id))
                    .map(|unit| unit.workers)
                    .sum()
            })
    }
}

/// The walk over a pasture's `livestock` (`+0xb78`..`+0xb80`) at its update's end
/// ([`World::sum_livestock_food`], `0x1411ca48f`): from the first, an id the world's table does
/// not find (`FUN_1415417a0`, `food_of` None) leaves the list (`memmove`: the later ones move up
/// one place), and the walk goes on at the next place on both paths (`INC R14D`, `ADD R15, 4`),
/// so the id that moved up into the gone one's place is passed over in this walk, a gone one
/// too. The others' food (their `+0x310`) sums in `f32` from 0.0 in the list's order (`ADDSS`).
/// The list kept and the sum.
pub(crate) fn held_livestock(
    livestock: &[ObjectId],
    food_of: impl Fn(ObjectId) -> Option<Amount>,
) -> (Vec<ObjectId>, Amount) {
    let mut kept = Vec::with_capacity(livestock.len());
    let mut food = Amount::ZERO;
    // the id after a gone one takes its place, which the walk has passed
    let mut passes_next = false;
    for &id in livestock {
        if passes_next {
            passes_next = false;
            kept.push(id);
        } else if let Some(held) = food_of(id) {
            food += held;
            kept.push(id);
        } else {
            passes_next = true;
        }
    }
    (kept, food)
}

impl World {
    /// `TRIBE_Building_Object::update`'s end (`0x1411ca41f`, after the production's part,
    /// [`World::building_production`]) on player `player_id`'s object `id` at `list_index` (or
    /// where the update's later parts moved it, [`World::live_slot`]): outside the walls' branch
    /// ([`walls_branch`], whose combat update ends the update), a building of a pasture's master
    /// ([`MasterRoles::PASTURE_MANGROVE`] and [`MasterRoles::PASTURE`], 1893 and 1897: `MOVSX`
    /// of its id less 0x765, `TEST` with `0xfffffffb`) walks its livestock ([`held_livestock`]:
    /// the gone ones leave), holds nothing itself (`+0x310`, a dword store of 0), and the action
    /// on top of its list, its Make, takes the livestock's food as its food still to make
    /// (`+0x44`, `FUN_140a78140`, [`ActionList::top_make_mut`]). So what its Make made into its
    /// holding in this update is gone, and what is left to make is what its livestock holds. Any
    /// other object keeps its food. It runs whatever the combat update answered.
    ///
    /// # Errors
    ///
    /// A master the player has not got, and an action on top other than a Make.
    ///
    /// [`ActionList::top_make_mut`]: crate::objects::actions::ActionList::top_make_mut
    pub(crate) fn sum_livestock_food(
        &mut self,
        player_id: PlayerId,
        list_index: ListIndex,
        id: ObjectId,
    ) -> GenieResult<()> {
        let Some((player_id, list_index)) = self.live_slot((player_id, list_index, id)) else {
            return Ok(());
        };
        let pasture = self.players.live(player_id, list_index)?;
        let master = pasture.master(self.master_tables(), player_id)?;
        let roles = &self.model.roles;
        if master.kind != MasterKind::BUILDING
            || walls_branch(master, roles)
            || !roles
                .of_master(master.id)
                .intersects(MasterRoles::PASTURE_MANGROVE | MasterRoles::PASTURE)
        {
            return Ok(());
        }
        let livestock = &self.parts.building_of(id).livestock;
        let (kept, food) = held_livestock(livestock, |livestock_id| {
            self.object(livestock_id).map(|(_, unit)| unit.life.amount)
        });
        if kept.len() != livestock.len() {
            self.parts.building_mut(id)?.livestock = kept;
        }
        self.players.live_mut(player_id, list_index)?.life.amount = Amount::ZERO;
        if let Some(layer) = self.parts.actions.get_mut(id)
            && let Some(make) = layer.list.top_make_mut(id)?
        {
            make.remaining = food;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::construction::FoundationId;
    use crate::construction::wait_solidify::WaitSolidify;
    use crate::gathering::make::MakeAction;
    use crate::gathering::make::MakeState;
    use crate::objects::HitPoints;
    use crate::objects::ObjectState;
    use crate::objects::actions::Action as ActionsAction;
    use crate::objects::actions::ActionList;
    use crate::objects::layers::{Action, Building};
    use crate::objects::lifecycle::Lifecycle;
    use crate::objects::masters::ClassId;
    use crate::objects::masters::Task;
    use crate::objects::masters::{Master, Masters};
    use crate::objects::unit::Unit;
    use crate::test_ids::object_id;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::tuning;
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::Player;
    use crate::world::WorldTime;
    use std::sync::Arc;

    /// 429381190's pasture 15342 (master 1897, radius 2) at (136, 72): sixteen points, four an
    /// edge; its livestock stood on the left edge's second and third (134.5, 71.4), (134.5,
    /// 72.3) and the bottom edge's fourth (137.2, 70.5).
    #[test]
    fn pasture_edges_take_four_points_each() {
        let points = edge_points(
            Position::new(136.0, 72.0, 0.0),
            Distance::new(2.0),
            &tuning().pasture,
        );
        assert_eq!(points.len(), 16);
        let fractions = [0.0f32, 0.3, 0.6, 0.3 + 0.3 + 0.3];
        assert_eq!(points[0], Point::new(134.5, 70.5));
        assert_eq!(points[3], Point::new(3.0 * fractions[3] + 134.5, 70.5));
        assert_eq!(points[3], Point::new(137.2, 70.5));
        // the right edge from (137.5, 73.5) down to (137.5, 70.5)
        assert_eq!(points[4], Point::new(137.5, 73.5));
        assert_eq!(points[5], Point::new(137.5, -3.0 * 0.3 + 73.5));
        // the top edge from (137.5, 73.5) to (134.5, 73.5)
        assert_eq!(points[9], Point::new(-3.0 * 0.3 + 137.5, 73.5));
        // the left edge from (134.5, 70.5) up
        assert_eq!(points[13], Point::new(134.5, 71.4));
        assert_eq!(points[14], Point::new(134.5, 72.3));
    }

    /// The food of livestock 601, 603 and 604 by id; 602 is gone.
    fn food_of(id: ObjectId) -> Option<Amount> {
        match id.get() {
            601 => Some(Amount::new(10.0)),
            603 => Some(Amount::new(20.0)),
            604 => Some(Amount::new(30.0)),
            _ => None,
        }
    }

    /// A gone id leaves the list (`memmove`), and the walk goes on at the next place: the id
    /// that moved up into it is kept unread in this walk, a gone one too.
    #[test]
    fn gone_livestock_leaves_and_the_next_one_is_passed_over() {
        let livestock = [601, 602, 603, 604].map(object_id);
        assert_eq!(
            held_livestock(&livestock, food_of),
            ([601, 603, 604].map(object_id).to_vec(), Amount::new(40.0))
        );
        let after_two = [601, 602, 602, 604, 602].map(object_id);
        assert_eq!(
            held_livestock(&after_two, food_of),
            ([601, 602, 604].map(object_id).to_vec(), Amount::new(40.0))
        );
        let held = [601, 603, 604].map(object_id);
        assert_eq!(
            held_livestock(&held, food_of),
            (held.to_vec(), Amount::new(60.0))
        );
    }

    /// The sum is `ADDSS` in the list's order from 0.0: 2^24 swallows each 1 after it, and a
    /// sum of −0.0 alone or of nothing is 0.0.
    #[test]
    fn livestock_food_sums_in_list_order_from_zero() {
        let foods = |id: ObjectId| match id.get() {
            1 => Some(Amount::new(16_777_216.0)),
            2 | 3 => Some(Amount::new(1.0)),
            _ => Some(Amount::new(-0.0)),
        };
        let food = |ids: &[i32]| {
            let livestock: Vec<ObjectId> = ids.iter().copied().map(object_id).collect();
            held_livestock(&livestock, foods).1.get()
        };
        assert_eq!(food(&[1, 2, 3]), 16_777_216.0);
        assert_eq!(food(&[2, 3, 1]), 16_777_218.0);
        assert_eq!(food(&[4]).to_bits(), 0.0f32.to_bits());
        assert_eq!(food(&[]).to_bits(), 0.0f32.to_bits());
    }

    /// A building master `id` of kind `kind` and class `class`.
    pub(crate) fn building_master(id: MasterId, kind: MasterKind, class: ClassId) -> Master {
        Master {
            id,
            kind,
            class,
            ..Master::default()
        }
    }

    /// A Make with `remaining` food still to make.
    pub(crate) fn make(remaining: f32) -> ActionsAction {
        ActionsAction::Make(MakeAction {
            state: MakeState::Making,
            remaining: Amount::new(remaining),
            total: Amount::new(175.0),
            task: Task::default(),
        })
    }

    /// Player 1's building 600 of `master` holding 5 food, its livestock 601 to 604 and
    /// `actions` on its list; the livestock 601, 603 and 604 beside it hold 10, 20 and 30 food,
    /// and 602 is gone.
    pub(crate) fn pasture_world(master: Master, actions: &[ActionsAction]) -> World {
        let unit = |id: i32, master_id: MasterId, amount: f32| Unit {
            id: object_id(id),
            life: Lifecycle {
                state: ObjectState::Alive,
                hp: HitPoints::new(50.0),
                master_id,
                amount: Amount::new(amount),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        let objects = vec![
            unit(600, master.id, 5.0),
            unit(601, ids::PASTURE_ARGALI, 10.0),
            unit(603, ids::PASTURE_ARGALI, 20.0),
            unit(604, ids::PASTURE_ARGALI, 30.0),
        ];
        let owner = Player {
            objects: objects.into(),
            ..Player::default()
        };
        let mut list = ActionList::EMPTY;
        for action in actions.iter().rev() {
            list.put(action.clone());
        }
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![Player::default(), owner],
        )
        .with_actions([(
            object_id(600),
            Action {
                list,
                ..Action::default()
            },
        )])
        .with_buildings([(
            object_id(600),
            Building {
                livestock: [601, 602, 603, 604].map(object_id).to_vec(),
                ..Building::NONE
            },
        )]);
        let livestock = building_master(ids::PASTURE_ARGALI, MasterKind::COMBAT, class::LIVESTOCK);
        world.model.masters = vec![
            Arc::new(Masters::default()),
            Arc::new(Masters::new([master, livestock])),
        ];
        world
    }

    /// The building's livestock, what it holds and its Make's food still to make.
    pub(crate) fn pasture_food(world: &World) -> (Vec<ObjectId>, Amount, Option<Amount>) {
        let id = object_id(600);
        (
            world.parts.building_of(id).livestock.clone(),
            world.players[1].objects[ListIndex::new(0)].life.amount,
            world
                .parts
                .actions_of(id)
                .list
                .make()
                .map(|make_action| make_action.remaining),
        )
    }

    /// A pasture of either master (1893, 1897) holds nothing at its update's end, its Make's
    /// food still to make is its livestock's (603 passed over), and its gone livestock left.
    #[test]
    fn pasture_food_is_its_livestock_food() {
        for id in [ids::PASTURE_MANGROVE, ids::PASTURE] {
            let master = building_master(id, MasterKind::BUILDING, class::FARM);
            let mut world = pasture_world(master, &[make(100.0)]);
            world
                .sum_livestock_food(PlayerId::new(1), ListIndex::new(0), object_id(600))
                .unwrap();
            assert_eq!(
                pasture_food(&world),
                (
                    [601, 603, 604].map(object_id).to_vec(),
                    Amount::ZERO,
                    Some(Amount::new(40.0))
                )
            );
        }
    }

    /// Without an action on top the pasture still holds nothing and lets its gone livestock go.
    #[test]
    fn pasture_without_an_action_on_top_holds_nothing() {
        let master = building_master(ids::PASTURE, MasterKind::BUILDING, class::FARM);
        let mut world = pasture_world(master, &[]);
        world
            .sum_livestock_food(PlayerId::new(1), ListIndex::new(0), object_id(600))
            .unwrap();
        assert_eq!(
            pasture_food(&world),
            ([601, 603, 604].map(object_id).to_vec(), Amount::ZERO, None)
        );
    }

    /// Another action on top, which no pasture of genie's holds, is an error.
    #[test]
    fn pasture_with_another_action_on_top_fails() {
        let master = building_master(ids::PASTURE, MasterKind::BUILDING, class::FARM);
        let wait = ActionsAction::WaitSolidify(WaitSolidify::new(
            Task::default(),
            FoundationId::new(object_id(601)),
        ));
        let mut world = pasture_world(master, &[wait, make(100.0)]);
        assert_eq!(
            world
                .sum_livestock_food(PlayerId::new(1), ListIndex::new(0), object_id(600))
                .map_err(|error| error.reason().to_owned()),
            Err("a farm whose action on top is not its Make".to_owned())
        );
    }

    /// A farm (50), a pasture's master of the walls' class (the walls' branch returns before
    /// the end) and one of another kind than the building's keep their food, Make and list.
    #[test]
    fn farm_walls_branch_and_other_kind_keep_their_food() {
        let untouched = (
            [601, 602, 603, 604].map(object_id).to_vec(),
            Amount::new(5.0),
            Some(Amount::new(100.0)),
        );
        for master in [
            building_master(ids::FARM, MasterKind::BUILDING, class::FARM),
            building_master(ids::PASTURE, MasterKind::BUILDING, class::WALL),
            building_master(ids::PASTURE, MasterKind::COMBAT, class::FARM),
        ] {
            let mut world = pasture_world(master, &[make(100.0)]);
            world
                .sum_livestock_food(PlayerId::new(1), ListIndex::new(0), object_id(600))
                .unwrap();
            assert_eq!(pasture_food(&world), untouched);
        }
    }
}
