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
//! A farm whose completion gave its owner the farm bonus takes the bonus off its Make's food
//! still to make in its next update ([`Unit::take_farm_bonus`]). A pasture holds nothing at its
//! update's end, and its Make's food still to make is then what its livestock holds
//! ([`World::sum_livestock_food`]).
//!
//! [`World::sum_livestock_food`]: crate::world::World::sum_livestock_food

use crate::objects::HitPoints;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::actions::Action as ActionsAction;
use crate::objects::actions::ActionList;
use crate::objects::lifecycle::Lifecycle;
use crate::objects::masters::ActionKind;
use crate::objects::masters::MasterKind;
use crate::objects::masters::PlayersMastersExt;
use crate::objects::masters::Task;
use crate::objects::teardown::HeadCode;
use crate::objects::unit::Unit;
use crate::objects::unit::object_master;
use crate::players::Amount;
use crate::technology::Attributes;
use crate::world::slots::PlayerSlotsExt;
use crate::world::step::Step;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// The action type (`+0x8`, 0x15).
pub const MAKE: ActionKind = ActionKind::named(21);

pub use crate::gathering::make_state::MakeState;
use crate::objects::layers::Action;
use crate::world::NO_ATTRIBUTE;
use crate::world::layer::AttributeRoles;
use crate::world::layer::ClassRoles;
use crate::world::layer::MasterRoles;
use crate::world::layer::Roles;
use crate::world::layer::Slots;

/// `RGE_Action_Make`: its state (`+0x18`), the food still to make (`+0x44`, the base's
/// `mTimer`), the food it makes in all (`+0x68`, the save's `work_timer`) and its task (`+0x48`).
#[derive(Clone, Copy, Debug, PartialEq, Diff)]
pub struct MakeAction {
    /// `+0x18`.
    pub(crate) state: MakeState,
    /// `+0x44`: the food still to make.
    pub(crate) remaining: Amount,
    /// `+0x68`: the food made in all; the constructor's 120.0 never shows, as
    /// [`MakeAction::start`] sets it at once.
    pub(crate) total: Amount,
    /// `+0x48`: the master's default task.
    pub(crate) task: Task,
}

impl MakeAction {
    /// The default task's start (slot 0x7f0, `FUN_14106eb40`): the factory's action
    /// (`FUN_140a99430`), then `set_only_action` (slot 0x808, `FUN_14106e990`), whose push
    /// (`FUN_140a977e0`) runs `first_in_stack` (`FUN_140a99980`): `set_state(6)`, then a task
    /// resource other than the farm's own holding (`life`'s `+0x308`) replaces it, with nothing
    /// held (`FUN_14119d310` with 0).
    #[must_use]
    pub fn start(
        task: Task,
        attributes: &[Amount],
        life: &mut Lifecycle,
        roles: &Roles,
        slots: Slots,
    ) -> Self {
        // the factory's constructor (FUN_140a99430) sets +0x68 to 120.0, which set_state(6)
        // replaces at once
        let mut make = Self {
            state: MakeState::New,
            remaining: Amount::ZERO,
            total: Amount::new(120.0),
            task,
        };
        make.set_state(MakeState::Making, attributes, roles, slots);
        if task.attribute_kind.is_some() && life.attribute_kind != task.attribute_kind {
            life.attribute_kind = task.attribute_kind;
            life.amount = Amount::ZERO;
        }
        make
    }

    /// `set_state` (`FUN_140a99d60`): state 6 takes the player's food to make (attribute 36 for
    /// a task of the farm food kind, `task+0x20 == 0x10`, else 88) as both what is left and what
    /// is made in all.
    pub fn set_state(
        &mut self,
        state: MakeState,
        attributes: &[Amount],
        roles: &Roles,
        slots: Slots,
    ) {
        self.state = state;
        if state == MakeState::Making {
            let kind = if roles
                .of_some_attribute(self.task.attribute_kind)
                .contains(AttributeRoles::FARM_FOOD)
            {
                slots.attribute.farm_food_amount
            } else {
                slots.attribute.fish_trap_food
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
    /// attribute `task.attribute_kind` instead (the player's slot 0xf8, `FUN_141616820` with 0
    /// and 1: [`crate::world::World::add_player_attribute`]), so nothing is taken off. Once all
    /// is made, a farm that holds nothing (not a pasture) runs its slot 0xd8: a combat object's
    /// or building's (`FUN_141211340`) spares class 42, then `FUN_14118f580` takes its hit
    /// points to 0 (the object is in its list, so not asleep; the special-object list, `+0x30c`
    /// bit 0, is not held). The game's `+0xc14` (`FUN_140c884c0`), which makes 1000 at once, is
    /// 0, as the tick oracle holds it. The other states do nothing.
    ///
    /// # Errors
    ///
    /// A farm whose master the slice lacks, a task with a work flag and no attribute, and the
    /// add's.
    pub(crate) fn update(
        &mut self,
        step: &mut Step<'_>,
        farm: &mut Unit,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let master = object_master(
            step.world.parts.own_master.get(farm.id),
            (farm.id, farm.life.master_id),
            step.world.model.masters.of(step.player),
        )?;
        match self.state {
            // `+0x18`: the state whose update reports the action done (`FUN_140a99e30` returns 1).
            MakeState::Done => return Ok(HeadCode::Deletes),
            MakeState::Making if Amount::ZERO < self.remaining || self.total <= Amount::ZERO => {
                let rate = (master.action_part().work_rate
                    + step.world.parts.combat_of(farm.id).work_rate_powerup)
                    * self.task.work_val1.get()
                    * step.dt.seconds();
                let before = farm.life.amount;
                if self.task.work_flag == 0 {
                    // FUN_14119d320: add, then at most the capacity
                    farm.life.amount += Amount::new(rate);
                    let capacity = Amount::of_master(master.capacity);
                    if capacity < farm.life.amount {
                        farm.life.amount = capacity;
                    }
                } else {
                    let attribute = self.task.attribute_kind.ok_or(NO_ATTRIBUTE)?;
                    step.world
                        .add_player_attribute(step.player, attribute, Amount::new(rate))?;
                }
                self.remaining -= farm.life.amount - before;
            }
            MakeState::Making
                if !roles.of_master(master.id).contains(MasterRoles::UNEXHAUSTED)
                    && farm.life.amount <= Amount::ZERO
                    // The first master type whose slot 0xd8 is `FUN_141211340` (70, `TRIBE_Combat_Object`, and 80);
                    // below it slot 0xd8 is `FUN_14118f580` itself.
                    && (master.kind < MasterKind::COMBAT || !roles.of_class(master.class).contains(ClassRoles::NEVER_EXHAUSTED)) =>
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
    pub(crate) fn make_part(
        &mut self,
        actions: &mut Action,
        step: &mut Step<'_>,
        roles: &Roles,
    ) -> GenieResult<HeadCode> {
        let Some(mut make) = actions.list.make().copied() else {
            return Ok(HeadCode::Keeps);
        };
        let made = make.update(step, self, roles);
        if let Some(make_action) = actions.list.make_mut() {
            *make_action = make;
        }
        made
    }

    /// `TRIBE_Building_Object::update`'s farm bonus (`0x1411c9000`, after the fifth of its hit
    /// points, before the walls' branch): a building in state 2 that owes the bonus its
    /// completion gave (`+0xcac`, [`Building::owes_bonus`]) and holds an action on top
    /// (`FUN_140a986f0`: the first of its list `+0x880`, at the list's `+0x10`,
    /// [`ActionList::head`]) takes its owner's attribute 237 (`+0x768`, the build layer's
    /// `folwark_collection_amount`) off that action's `+0x44` (`SUBSS`, then `FUN_140a78140`'s
    /// store) and owes nothing more. A farm's action on top is its Make, whose `+0x44` is its
    /// food still to make ([`MakeAction::remaining`]): the farm masters' (50, 199, 1187, 1893,
    /// 1897) default task, which starts in the farm's first update in state 2, after its
    /// building's steps, so the farm takes the bonus in its next update. Their other task, the
    /// Farm (121, `TRIBE_Action_Farm`), no list of genie's holds. Without an action on top it
    /// owes the bonus on.
    ///
    /// [`Building::owes_bonus`]: crate::objects::layers::Building::owes_bonus
    ///
    /// # Errors
    ///
    /// An action on top other than a Make, which no farm of genie's holds
    /// ([`ActionList::top_make_mut`]), and an owner without attribute 237.
    pub(crate) fn take_farm_bonus(
        &self,
        actions: &mut Action,
        step: &mut Step<'_>,
    ) -> GenieResult<()> {
        if self.life.state != ObjectState::Alive
            || !step.world.parts.building_of(self.id).owes_bonus
        {
            return Ok(());
        }
        let Some(make) = actions.list.top_make_mut(self.id)? else {
            return Ok(());
        };
        let bonus = Attributes::get(
            step.world.players.player(step.player)?,
            step.world.model.slots.attribute.folwark_collection_amount,
        )?;
        make.remaining -= bonus;
        step.world.parts.building_mut(self.id)?.owes_bonus = false;
        Ok(())
    }
}

impl ActionList {
    /// A farm's action on top (`FUN_140a986f0`, [`ActionList::head`]) as its Make, whose `+0x44`
    /// (its food still to make, [`MakeAction::remaining`]) the building's update writes
    /// (`FUN_140a78140`): the Make of a farm master's default task, which starts in the farm's
    /// first update in state 2. None for an empty list.
    ///
    /// # Errors
    ///
    /// Another action on top of farm `farm`'s list: a farm's list holds its default task's Make
    /// only (its other task, the Farm 121, `TRIBE_Action_Farm`, no list of genie's holds).
    pub(crate) fn top_make_mut(&mut self, farm: ObjectId) -> GenieResult<Option<&mut MakeAction>> {
        match self.head_mut() {
            None => Ok(None),
            Some(ActionsAction::Make(make)) => Ok(Some(make)),
            Some(top) => Err(
                GenieError::unreachable("a farm whose action on top is not its Make")
                    .with("object", farm)
                    .with("action", top.kind()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::FoundationId;
    use crate::construction::wait_solidify::WaitSolidify;
    use crate::objects::ListIndex;
    use crate::objects::WorkRate;
    use crate::objects::layers::Building;
    use crate::objects::masters::ActionPart;
    use crate::objects::masters::CarryCapacity;
    use crate::objects::masters::MasterId;
    use crate::objects::masters::MasterKind;
    #[cfg(test)]
    use crate::objects::masters::WorkValue;
    use crate::objects::masters::{Master, Masters};
    use crate::players::EscrowShare;
    use crate::players::PlayerId;
    use crate::players::PlayerKind;
    use crate::players::attribute::AttributeId;
    use crate::test_ids::master_id;
    use crate::test_ids::object_id;
    use crate::testing::attribute;
    use crate::testing::class;
    use crate::testing::ids;
    use crate::testing::roles;
    use crate::testing::slots;
    #[cfg(test)]
    use crate::world::FrameSeed;
    use crate::world::Millis;
    use crate::world::WorldTime;
    #[cfg(test)]
    use crate::world::tests::in_update;
    use crate::world::{Player, World};
    use std::slice::from_ref;
    use std::sync::Arc;

    /// The farm's default task in build 158041's `.dat` (master 50, task 0).
    fn farm_task() -> Task {
        Task {
            action_kind: MAKE,
            attribute_kind: Some(attribute::FARM_FOOD),
            attribute_kind2: Some(attribute::FOOD),
            work_val1: WorkValue::new(1.0),
            ..Task::default()
        }
    }

    /// A farm's master `id` (type 80, class 49, work rate 0.4, capacity 15).
    fn farm_master(id: MasterId) -> Master {
        Master {
            id,
            kind: MasterKind::BUILDING,
            class: class::FARM,
            capacity: CarryCapacity::of_data(15),
            action: Some(ActionPart {
                work_rate: WorkRate::new(0.4),
                ..ActionPart::default()
            }),
            ..Master::default()
        }
    }

    fn attributes() -> Vec<Amount> {
        let mut amounts = vec![Amount::ZERO; 100];
        (
            amounts[attribute::FARM_FOOD_AMOUNT.index()],
            amounts[attribute::FISH_TRAP_FOOD.index()],
        ) = (Amount::new(175.0), Amount::new(225.0));
        amounts
    }

    /// Player 1's farm 600 of `master` holding `amount` of food under `make`, its masters the
    /// farms 50 and 1897 and the class-42 master 7.
    fn farm_world(make: MakeAction, master: MasterId, amount: f32) -> World {
        let farm = Unit {
            id: object_id(600),
            life: Lifecycle {
                state: ObjectState::Alive,
                hp: HitPoints::new(480.0),
                master_id: master,
                attribute_kind: Some(attribute::FARM_FOOD),
                amount: Amount::new(amount),
                ..Lifecycle::default()
            },
            ..Unit::default()
        };
        let mut layer = Action::default();
        layer.list.set_make(Some(make));
        let owner = Player {
            objects: vec![farm].into(),
            attributes: attributes(),
            ..Player::default()
        };
        let mut world = World::new(
            WorldTime::ZERO,
            FrameSeed::of_data(1),
            PlayerId::new(1),
            vec![Millis::ZERO, Millis::ZERO],
            vec![Player::default(), owner],
        )
        .with_actions([(object_id(600), layer)]);
        let wall = Master {
            class: class::RELIC,
            ..farm_master(ids::ID_7)
        };
        let masters = Masters::new([farm_master(ids::FARM), farm_master(ids::PASTURE), wall]);
        world.model.masters = vec![Arc::new(Masters::default()), Arc::new(masters)];
        world
    }

    /// The farm's update of 104 ms through its step: its code, its Make, what it holds and its
    /// hit points.
    fn made(world: &mut World) -> (HeadCode, MakeAction, f32, f32) {
        let code = in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            104,
            |unit, action, step| Unit::make_part(unit, action, step, &roles()),
        )
        .unwrap();
        let farm = &world.players[1].objects[ListIndex::new(0)];
        (
            code,
            *world.parts.actions_of(farm.id).list.make().unwrap(),
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
    fn the_start_takes_the_farm_food_and_holds_the_task_resource_from_nothing() {
        let mut life = Lifecycle {
            attribute_kind: Some(attribute::FOOD),
            amount: Amount::new(15.0),
            ..Lifecycle::default()
        };
        let make = MakeAction::start(farm_task(), &attributes(), &mut life, &roles(), slots());
        assert_eq!(
            (make.state, make.remaining, make.total),
            (MakeState::Making, Amount::new(175.0), Amount::new(175.0))
        );
        assert_eq!(
            (life.attribute_kind, life.amount),
            (Some(attribute::FARM_FOOD), Amount::ZERO)
        );
    }

    #[test]
    fn fish_trap_takes_the_fish_trap_food_and_keeps_holding_of_its_kind() {
        let task = Task {
            attribute_kind: Some(AttributeId::named(17)),
            ..farm_task()
        };
        let mut life = Lifecycle {
            attribute_kind: Some(AttributeId::named(17)),
            amount: Amount::new(15.0),
            ..Lifecycle::default()
        };
        let make = MakeAction::start(task, &attributes(), &mut life, &roles(), slots());
        assert_eq!(
            (make.remaining, life.attribute_kind, life.amount),
            (
                Amount::new(225.0),
                Some(AttributeId::named(17)),
                Amount::new(15.0)
            )
        );
    }

    #[test]
    fn the_update_fills_the_farm_within_its_capacity_and_counts_what_it_made() {
        let mut world = farm_world(making(100.0, 175.0, farm_task()), ids::FARM, 14.98);
        let (code, make, amount, _) = made(&mut world);
        let gained = 15.0f32 - 14.98;
        assert_eq!(
            (code, amount, make.remaining),
            (HeadCode::Keeps, 15.0, Amount::new(100.0 - gained))
        );
        // at the capacity nothing more is made
        let (_, make, _, _) = made(&mut world);
        assert_eq!(make.remaining, Amount::new(100.0 - gained));
        // a farmer took 5: the rate's share comes back
        world.players[1].objects[ListIndex::new(0)].life.amount = Amount::new(10.0);
        let (_, make, amount, _) = made(&mut world);
        let rate = 0.4f32 * 1.0 * 0.104;
        assert_eq!(amount, 10.0 + rate);
        assert_eq!(
            make.remaining,
            Amount::new(100.0 - gained - (10.0 + rate - 10.0))
        );
    }

    /// The farm's powerup (slot 0x6e8, `+0xa30`) adds to its master's work rate.
    #[test]
    fn the_farm_powerup_adds_to_the_rate() {
        let mut world = farm_world(making(100.0, 175.0, farm_task()), ids::FARM, 0.0);
        world
            .parts
            .combat_mut(world.players[1].objects[ListIndex::new(0)].id)
            .unwrap()
            .work_rate_powerup = WorkRate::new(0.1);
        let (_, _, amount, _) = made(&mut world);
        assert_eq!(amount, (0.4f32 + 0.1) * 1.0 * 0.104);
    }

    #[test]
    fn farm_with_nothing_left_to_make_and_nothing_held_is_exhausted_unless_pasture() {
        let exhausted = |master: MasterId, amount: f32| {
            let mut world = farm_world(making(0.0, 175.0, farm_task()), master, amount);
            made(&mut world).3 == 0.0
        };
        assert!(exhausted(ids::FARM, 0.0));
        assert!(!exhausted(ids::PASTURE, 0.0));
        assert!(!exhausted(ids::ID_7, 0.0));
        assert!(!exhausted(ids::FARM, 3.0));
    }

    #[test]
    fn task_with_work_flag_makes_for_its_player_and_never_runs_out() {
        let task = Task {
            work_flag: 1,
            ..farm_task()
        };
        let mut world = farm_world(making(5.0, 5.0, task), ids::FARM, 0.0);
        let (_, make, amount, _) = made(&mut world);
        let food = world.players[1].attributes[attribute::FARM_FOOD.index()];
        assert_eq!(food, Amount::new(0.4 * 0.104));
        assert_eq!((amount, make.remaining), (0.0, Amount::new(5.0)));
    }

    /// A work flag's add is its player's slot 0xf8 with 0 and 1: a computer player's food sets
    /// its share aside.
    #[test]
    fn task_with_work_flag_food_takes_the_computer_escrow() {
        let task = Task {
            work_flag: 1,
            attribute_kind: Some(attribute::FOOD),
            ..farm_task()
        };
        let mut world = farm_world(making(5.0, 5.0, task), ids::FARM, 0.0);
        world.players[1].kind = PlayerKind::Computer;
        world.players[1].escrow_percentage = [0.5, 0.0, 0.0, 0.0].map(EscrowShare::of_data);
        let _ = made(&mut world);
        let player = &world.players[1];
        let gained = Amount::new(0.4 * 0.104);
        assert_eq!(
            (player.attributes[0], player.escrow[0]),
            (gained, EscrowShare::of_data(0.5).of(gained))
        );
    }

    #[test]
    fn state_1_is_done() {
        let done = MakeAction {
            state: MakeState::Done,
            ..making(0.0, 0.0, farm_task())
        };
        let mut world = farm_world(done, ids::FARM, 0.0);
        assert_eq!(made(&mut world).0, HeadCode::Deletes);
    }

    /// A farm whose master the slice lacks stops its update, in any state.
    #[test]
    fn farm_without_its_master_fails() {
        let mut world = farm_world(making(0.0, 0.0, farm_task()), master_id(99), 0.0);
        let head_code = in_update(
            &mut world,
            (PlayerId::new(1), ListIndex::new(0)),
            104,
            |unit, action, step| Unit::make_part(unit, action, step, &roles()),
        );
        assert!(head_code.is_err());
    }

    /// [`farm_world`]'s farm in `state` with `actions` on its list, owing the bonus its
    /// completion gave (`+0xcac`) when `owes`, its owner's attribute 237 at 45.
    fn bonus_world(actions: &[ActionsAction], state: ObjectState, owes: bool) -> World {
        let mut world = farm_world(making(175.0, 175.0, farm_task()), ids::FARM, 0.0);
        let farm = &mut world.players[1].objects[ListIndex::new(0)];
        farm.life.state = state;
        let id = farm.id;
        let list = &mut world.parts.actions_mut(id).unwrap().list;
        *list = ActionList::EMPTY;
        for action in actions.iter().rev() {
            list.put(action.clone());
        }
        let attributes = &mut world.players[1].attributes;
        attributes.resize(240, Amount::ZERO);
        attributes[slots().attribute.folwark_collection_amount.index()] = Amount::new(45.0);
        world.with_buildings([(
            id,
            Building {
                owes_bonus: owes,
                ..Building::NONE
            },
        )])
    }

    /// The farm's bonus ([`Unit::take_farm_bonus`]) in an update of [`bonus_world`]'s farm: its
    /// result, its Make's food still to make and whether it still owes the bonus.
    fn bonus_taken(world: &mut World) -> (GenieResult<()>, Option<f32>, bool) {
        let taken = in_update(
            world,
            (PlayerId::new(1), ListIndex::new(0)),
            104,
            |unit, action, step| Ok(unit.take_farm_bonus(action, step)),
        )
        .unwrap();
        let id = world.players[1].objects[ListIndex::new(0)].id;
        (
            taken,
            world
                .parts
                .actions_of(id)
                .list
                .make()
                .map(|make| make.remaining.get()),
            world.parts.building_of(id).owes_bonus,
        )
    }

    /// A farm in state 2 that owes its bonus (`TRIBE_Building_Object::update`, `+0xcac`) takes
    /// its owner's attribute 237 off its Make's food still to make (`+0x44`, `SUBSS`), once.
    #[test]
    fn farm_owing_its_bonus_takes_it_off_its_make_once() {
        let make = ActionsAction::Make(making(175.0, 175.0, farm_task()));
        let mut world = bonus_world(&[make], ObjectState::Alive, true);
        assert_eq!(bonus_taken(&mut world), (Ok(()), Some(175.0 - 45.0), false));
        assert_eq!(bonus_taken(&mut world), (Ok(()), Some(175.0 - 45.0), false));
    }

    /// A farm owes its bonus on without an action on top (its Make not started yet) or outside
    /// state 2; one that owes none keeps its Make as it is.
    #[test]
    fn farm_owes_its_bonus_on_without_an_action_on_top_or_outside_state_2() {
        let make = ActionsAction::Make(making(175.0, 175.0, farm_task()));
        let mut empty = bonus_world(&[], ObjectState::Alive, true);
        assert_eq!(bonus_taken(&mut empty), (Ok(()), None, true));
        for state in [ObjectState::Foundation, ObjectState::Dying] {
            let mut world = bonus_world(from_ref(&make), state, true);
            assert_eq!(bonus_taken(&mut world), (Ok(()), Some(175.0), true));
        }
        let mut paid = bonus_world(&[make], ObjectState::Alive, false);
        assert_eq!(bonus_taken(&mut paid), (Ok(()), Some(175.0), false));
    }

    /// The bonus goes to the action on top (`FUN_140a986f0`), not to a Make below it: another
    /// action there, which no farm of genie's holds, is an error, and the Make keeps its food.
    #[test]
    fn farm_owing_its_bonus_with_another_action_on_top_fails() {
        let wait = ActionsAction::WaitSolidify(WaitSolidify::new(
            Task::default(),
            FoundationId::new(object_id(601)),
        ));
        let make = ActionsAction::Make(making(175.0, 175.0, farm_task()));
        let mut world = bonus_world(&[wait, make], ObjectState::Alive, true);
        let (taken, remaining, owes) = bonus_taken(&mut world);
        assert_eq!(
            taken.map_err(|error| error.reason().to_owned()),
            Err("a farm whose action on top is not its Make".to_owned())
        );
        assert_eq!((remaining, owes), (Some(175.0), true));
    }
}
