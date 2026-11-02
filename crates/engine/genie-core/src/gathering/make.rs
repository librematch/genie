//! A farm's food: `RGE_Action_Make` (action type 21, vtable `RGE_Action_Make::vftable`), the
//! default task of the farms (50, 1187), the fish trap (199) and the pastures (1893, 1897). The
//! RGE factory makes it (`FUN_140a97e00`, case 0x15, constructor `FUN_140a99430`); the TRIBE
//! factory (`FUN_140aca4b0`) has no case of its own for it.
//!
//! The farm holds what it made (`+0x308`, `+0x310`) up to its master's capacity
//! (`attribute_max_amount`, `+0x61c`): the Make adds its master's work rate (plus the farm's
//! powerup) times the task's `work_val1` per second, and the farmers take from it. The food
//! still to make starts at the player's farm food (attribute 36) or fish trap food (88); once it
//! is made and the farmers took the last of it, the farm's slot 0xd8 sets its hit points to 0.

use crate::GenieError;
use crate::objects::HitPoints;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionType;
use crate::objects::masters::ClassId;
use crate::objects::masters::MasterId;
use crate::objects::masters::PlayersMasters;
use crate::objects::masters::Task;
use crate::objects::masters::class;
use crate::objects::masters::master_type::COMBAT;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::players::attribute;
use crate::world::slots::PlayerSlots;
use crate::world::step::Step;
use genie_diff::Diff;

/// The action type (`+0x8`, 0x15).
pub const MAKE: ActionType = ActionType::named(21);

pub use crate::gathering::make_state::MakeState;

/// The masters whose exhaustion the update does not check (`FUN_140a99e30`: the master id, not
/// 0x765 and not 0x769): the pastures.
pub const UNEXHAUSTED: [MasterId; 2] = [MasterId::named(1893), MasterId::named(1897)];

/// The class whose slot 0xd8 (`FUN_141211340`, a combat object's or building's) does nothing
/// (decoded 42); class 58 is spared only while it holds some, which an exhausted farm does not.
const NEVER_EXHAUSTED_CLASS: ClassId = class::RELIC;

/// `RGE_Action_Make`: its state (`+0x18`), the food still to make (`+0x44`, the base's
/// `mTimer`), the food it makes in all (`+0x68`, the save's `work_timer`) and its task (`+0x48`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MakeAction {
    /// `+0x18`.
    pub state: MakeState,
    /// `+0x44`: the food still to make.
    pub remaining: Amount,
    /// `+0x68`: the food made in all; the constructor's 120.0 never shows, as
    /// [`MakeAction::start`] sets it at once.
    pub total: Amount,
    /// `+0x48`: the master's default task.
    pub task: Task,
}

impl MakeAction {
    /// The default task's start (slot 0x7f0, `FUN_14106eb40`): the factory's action
    /// (`FUN_140a99430`), then `set_only_action` (slot 0x808, `FUN_14106e990`), whose push
    /// (`FUN_140a977e0`) runs `first_in_stack` (`FUN_140a99980`): `set_state(6)`, then a task
    /// resource other than the farm's own holding (`life`'s `+0x308`) replaces it, with nothing
    /// held (`FUN_14119d310` with 0).
    #[must_use]
    pub fn start(task: Task, attributes: &[Amount], life: &mut Lifecycle) -> Self {
        // the factory's constructor (FUN_140a99430) sets +0x68 to 120.0, which set_state(6)
        // replaces at once
        let mut make = Self {
            state: MakeState::New,
            remaining: Amount::ZERO,
            total: Amount::new(120.0),
            task,
        };
        make.set_state(MakeState::Making, attributes);
        if task.attribute_type.is_some() && life.attribute_type != task.attribute_type {
            life.attribute_type = task.attribute_type;
            life.amount = Amount::ZERO;
        }
        make
    }

    /// `set_state` (`FUN_140a99d60`): state 6 takes the player's food to make (attribute 36 for
    /// a task of the farm food kind, `task+0x20 == 0x10`, else 88) as both what is left and what
    /// is made in all.
    pub fn set_state(&mut self, state: MakeState, attributes: &[Amount]) {
        self.state = state;
        if state == MakeState::Making {
            let kind = if self.task.attribute_type == Some(attribute::FARM_FOOD) {
                attribute::FARM_FOOD_AMOUNT
            } else {
                attribute::FISH_TRAP_FOOD
            };
            self.remaining = attributes
                .get(kind.index())
                .copied()
                .unwrap_or(Amount::ZERO);
            self.total = self.remaining;
        }
    }

    /// The update (`FUN_140a99e30`) of farm `farm` through its step. State 1 is done (its 1:
    /// the action list deletes it). In state 6, while food is left to make (or none was ever to
    /// be made), the farm's holding (`+0x310`) grows by its master's work rate (`+0x798`) plus
    /// its powerup (slot 0x6e8, `+0xa30`) times the task's `work_val1` and the update's seconds,
    /// within its master's capacity (`+0x61c`; `FUN_14119d320` with 1 and 1), and what it gained
    /// is no longer left to make; a task with a work flag (`+0x34`) adds to its player's
    /// attribute `task.attribute_type` instead (the player's slot 0xf8, `FUN_141616820` with 0
    /// and 1), so nothing is taken off. Once all is made, a farm that holds nothing (not a
    /// pasture) runs its slot 0xd8: a combat object's or building's (`FUN_141211340`) spares
    /// class 42, then `FUN_14118f580` takes its hit points to 0 (the object is in its list, so
    /// not asleep; the special-object list, `+0x30c` bit 0, is not held). The game's `+0xc14`
    /// (`FUN_140c884c0`), which makes 1000 at once, is 0, as the tick oracle holds it. The other
    /// states do nothing.
    ///
    /// # Errors
    ///
    /// A farm whose master the slice lacks, and an attribute its player has not got.
    pub(crate) fn update(
        &mut self,
        c: &mut Step<'_>,
        farm: &mut Unit,
    ) -> Result<HeadCode, GenieError> {
        let master = object_master(
            c.w.parts.own_master.get(farm.id),
            (farm.id, farm.life.master_id),
            c.w.masters.of(c.player),
        )?;
        match self.state {
            // `+0x18`: the state whose update reports the action done (`FUN_140a99e30` returns 1).
            MakeState::Done => return Ok(HeadCode::Deletes),
            MakeState::Making if Amount::ZERO < self.remaining || self.total <= Amount::ZERO => {
                let rate = (master.work_rate + c.w.parts.combat_of(farm.id).work_rate_powerup)
                    * self.task.work_val1.get()
                    * c.dt.seconds();
                let before = farm.life.amount;
                if self.task.work_flag == 0 {
                    // FUN_14119d320: add, then at most the capacity
                    farm.life.amount += Amount::new(rate);
                    let capacity = Amount::of_master(master.capacity);
                    if capacity < farm.life.amount {
                        farm.life.amount = capacity;
                    }
                } else {
                    let attribute = self.task.attribute_type.ok_or(crate::world::NO_ATTRIBUTE)?;
                    c.w.players
                        .player_mut(c.player)?
                        .add_attribute(attribute, Amount::new(rate))?;
                }
                self.remaining -= farm.life.amount - before;
            }
            MakeState::Making
                if !UNEXHAUSTED.contains(&master.id)
                    && farm.life.amount <= Amount::ZERO
                    // The first master type whose slot 0xd8 is `FUN_141211340` (70, `TRIBE_Combat_Object`, and 80);
                    // below it slot 0xd8 is `FUN_14118f580` itself.
                    && (master.kind < COMBAT || master.class != NEVER_EXHAUSTED_CLASS) =>
            {
                farm.life.hp = HitPoints::ZERO;
            }
            _ => {}
        }
        Ok(HeadCode::Keeps)
    }
}

impl Unit {
    /// A farm's Make in its update ([`MakeAction::update`]) and its code: nothing without one.
    pub(crate) fn make_part(&mut self, c: &mut Step<'_>) -> Result<HeadCode, GenieError> {
        let Some(mut make) = self.actions.list.make().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let made = make.update(c, self);
        if let Some(m) = self.actions.list.make_mut() {
            *m = make;
        }
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ListIndex;
    use crate::objects::ObjectState;
    use crate::objects::WorkRate;
    use crate::objects::masters::CarryCapacity;
    use crate::objects::masters::ids;
    use crate::objects::masters::master_type;
    use crate::objects::masters::{Master, Masters};
    use crate::players::PlayerId;
    use crate::players::attribute;
    use crate::players::attribute::AttributeId;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::world::Millis;
    use crate::world::WorldTime;
    use crate::world::{Player, World};

    /// The farm's default task in build 158041's `.dat` (master 50, task 0).
    fn farm_task() -> Task {
        Task {
            action_type: MAKE,
            attribute_type: Some(attribute::FARM_FOOD),
            attribute_type2: Some(attribute::FOOD),
            work_val1: crate::objects::masters::WorkValue::new(1.0),
            ..Task::default()
        }
    }

    /// A farm's master `id` (type 80, class 49, work rate 0.4, capacity 15).
    fn farm_master(id: MasterId) -> Master {
        Master {
            id,
            kind: master_type::BUILDING,
            class: class::FARM,
            work_rate: WorkRate::new(0.4),
            capacity: CarryCapacity::of_data(15),
            ..Master::default()
        }
    }

    fn attributes() -> Vec<Amount> {
        let mut a = vec![Amount::ZERO; 100];
        (
            a[attribute::FARM_FOOD_AMOUNT.index()],
            a[attribute::FISH_TRAP_FOOD.index()],
        ) = (Amount::new(175.0), Amount::new(225.0));
        a
    }

    /// Player 1's farm 600 of `master` holding `amount` of food under `make`, its masters the
    /// farms 50 and 1897 and the class-42 master 7.
    fn farm_world(make: MakeAction, master: MasterId, amount: f32) -> World {
        let mut farm = Unit {
            id: object_id(600),
            life: Lifecycle {
                state: ObjectState::Alive,
                hp: HitPoints::new(480.0),
                master_id: master,
                attribute_type: Some(attribute::FARM_FOOD),
                amount: Amount::new(amount),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        farm.actions.list.set_make(Some(make));
        let owner = Player {
            objects: vec![farm].into(),
            attributes: attributes(),
            ..Player::default()
        };
        let mut t = World::new(
            WorldTime::ZERO,
            crate::world::FrameSeed::of_data(1),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![Player::default(), owner],
        );
        let wall = Master {
            class: NEVER_EXHAUSTED_CLASS,
            ..farm_master(ids::ID_7)
        };
        let masters = Masters::new([farm_master(ids::FARM), farm_master(ids::PASTURE), wall]);
        t.masters = vec![Masters::default(), masters];
        t
    }

    /// The farm's update of 104 ms through its step: its code, its Make, what it holds and its
    /// hit points.
    fn made(t: &mut World) -> (HeadCode, MakeAction, f32, f32) {
        let code = crate::world::tests::in_update(
            t,
            (PlayerId::new(1), ListIndex::new(0)),
            104,
            Unit::make_part,
        )
        .unwrap();
        let farm = &t.players[1].objects[ListIndex::new(0)];
        (
            code,
            *farm.actions.list.make().unwrap(),
            farm.life.amount.get(),
            farm.life.hp.get(),
        )
    }

    fn making(remaining: f32, total: f32, task: Task) -> MakeAction {
        MakeAction {
            state: MakeState::Making,
            remaining: Amount::new(remaining),
            total: Amount::new(total),
            task,
        }
    }

    #[test]
    fn the_start_takes_the_farm_food_and_holds_the_task_s_resource_from_nothing() {
        let mut life = Lifecycle {
            attribute_type: Some(attribute::FOOD),
            amount: Amount::new(15.0),
            ..Lifecycle::default()
        };
        let make = MakeAction::start(farm_task(), &attributes(), &mut life);
        assert_eq!(
            (make.state, make.remaining, make.total),
            (MakeState::Making, Amount::new(175.0), Amount::new(175.0))
        );
        assert_eq!(
            (life.attribute_type, life.amount),
            (Some(attribute::FARM_FOOD), Amount::ZERO)
        );
    }

    #[test]
    fn a_fish_trap_takes_the_fish_trap_food_and_keeps_a_holding_of_its_kind() {
        let task = Task {
            attribute_type: Some(AttributeId::numbered(17)),
            ..farm_task()
        };
        let mut life = Lifecycle {
            attribute_type: Some(AttributeId::numbered(17)),
            amount: Amount::new(15.0),
            ..Lifecycle::default()
        };
        let make = MakeAction::start(task, &attributes(), &mut life);
        assert_eq!(
            (make.remaining, life.attribute_type, life.amount),
            (
                Amount::new(225.0),
                Some(AttributeId::numbered(17)),
                Amount::new(15.0)
            )
        );
    }

    #[test]
    fn the_update_fills_the_farm_within_its_capacity_and_counts_what_it_made() {
        let mut t = farm_world(making(100.0, 175.0, farm_task()), ids::FARM, 14.98);
        let (code, make, amount, _) = made(&mut t);
        let gained = 15.0f32 - 14.98;
        assert_eq!(
            (code, amount, make.remaining),
            (HeadCode::Keeps, 15.0, Amount::new(100.0 - gained))
        );
        // at the capacity nothing more is made
        let (_, make, _, _) = made(&mut t);
        assert_eq!(make.remaining, Amount::new(100.0 - gained));
        // a farmer took 5: the rate's share comes back
        t.players[1].objects[ListIndex::new(0)].life.amount = Amount::new(10.0);
        let (_, make, amount, _) = made(&mut t);
        let rate = 0.4f32 * 1.0 * 0.104;
        assert_eq!(amount, 10.0 + rate);
        assert_eq!(
            make.remaining,
            Amount::new(100.0 - gained - (10.0 + rate - 10.0))
        );
    }

    /// The farm's powerup (slot 0x6e8, `+0xa30`) adds to its master's work rate.
    #[test]
    fn the_farm_s_powerup_adds_to_the_rate() {
        let mut t = farm_world(making(100.0, 175.0, farm_task()), ids::FARM, 0.0);
        t.parts
            .combat_mut(t.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .work_rate_powerup = WorkRate::new(0.1);
        let (_, _, amount, _) = made(&mut t);
        assert_eq!(amount, (0.4f32 + 0.1) * 1.0 * 0.104);
    }

    #[test]
    fn a_farm_with_nothing_left_to_make_and_nothing_held_is_exhausted_unless_a_pasture() {
        let exhausted = |master: MasterId, amount: f32| {
            let mut t = farm_world(making(0.0, 175.0, farm_task()), master, amount);
            made(&mut t).3 == 0.0
        };
        assert!(exhausted(ids::FARM, 0.0));
        assert!(!exhausted(ids::PASTURE, 0.0));
        assert!(!exhausted(ids::ID_7, 0.0));
        assert!(!exhausted(ids::FARM, 3.0));
    }

    #[test]
    fn a_task_with_a_work_flag_makes_for_its_player_and_never_runs_out() {
        let task = Task {
            work_flag: 1,
            ..farm_task()
        };
        let mut t = farm_world(making(5.0, 5.0, task), ids::FARM, 0.0);
        let (_, make, amount, _) = made(&mut t);
        let food = t.players[1].attributes[attribute::FARM_FOOD.index()];
        assert_eq!(food, Amount::new(0.4 * 0.104));
        assert_eq!((amount, make.remaining), (0.0, Amount::new(5.0)));
    }

    #[test]
    fn state_1_is_done() {
        let done = MakeAction {
            state: MakeState::Done,
            ..making(0.0, 0.0, farm_task())
        };
        let mut t = farm_world(done, ids::FARM, 0.0);
        assert_eq!(made(&mut t).0, HeadCode::Deletes);
    }

    /// A farm whose master the slice lacks stops its update, in any state.
    #[test]
    fn a_farm_without_its_master_fails() {
        let mut t = farm_world(making(0.0, 0.0, farm_task()), master_id(99), 0.0);
        let r = crate::world::tests::in_update(
            &mut t,
            (PlayerId::new(1), ListIndex::new(0)),
            104,
            Unit::make_part,
        );
        assert!(r.is_err());
    }
}
