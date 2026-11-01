//! The orders a unit AI takes at once and its attack commands.

use crate::GenieError;
use crate::map::Point;
use crate::map::Position;
use crate::math::rng::Well512;
use crate::objects::ObjectId;
use crate::objects::ObjectState;
use crate::objects::masters::ClassId;
use crate::objects::masters::class;
use crate::unit_ai::ActionKind;
use crate::unit_ai::Issuer;
use crate::unit_ai::OrderKind;
use crate::unit_ai::OrderOutcome;
use crate::unit_ai::OrderTargetType;
use crate::unit_ai::UnitAiType;
use crate::unit_ai::{
    ARTIFACT_AI, CIVILIAN, Notify, QueuedOrder, SIEGE_AI, SIEGE_TOWER_AI, Surroundings, UnitAi,
    UnitSide, civilian_may_attack, lion_may_attack,
};
use crate::world::Millis;
use crate::world::slots::CheckedIndex;

impl UnitAi {
    /// `FUN_140c55c90`'s immediate branch ("Adding as an immediate order."), for a unit in state
    /// 2 or below and an order other than −1. With `keep_from` (inFront 1, the unit's id) a
    /// current order goes in front of the queue first ("Keeping old order, putting it onto the
    /// queue.", [`UnitAi::insert_order`]): the issuer the unit, the current order, priority
    /// (`+0x40`), target and its class, owner −1, the target point, the range `+0x5c`, and the
    /// new order's shown point and ready flag. Then the order goes in front. But for an attack
    /// roundup (`0x2d9`, "so I'm not stopping the object") the target goes (`FUN_140c5e1e0`) and
    /// the action stops (slot 0xb0 with 1, which ends an order of its action and reads the
    /// unit's `place` for it). The notifies end (`FUN_140c5dc10` with 0; outside the full update
    /// without `world`, where a completion it would stop on is not modelled), the queue's first
    /// order runs at once (slot 0x230, [`UnitAi::process_order`] with `world`, the surroundings
    /// and the unit) and, unless it was not ready, leaves the queue (`FUN_140c5e280`).
    pub(super) fn order_now(
        &mut self,
        order: QueuedOrder,
        keep_from: Option<ObjectId>,
        place: Option<(Point, Option<ClassId>)>,
        world: Option<(&Surroundings, bool)>,
        ai: &mut Well512,
        side: &mut dyn UnitSide,
    ) -> Result<(), GenieError> {
        if let Some(unit) = keep_from
            && let Some(current) = self.current_order
        {
            let kept = QueuedOrder {
                issuer: Some(Issuer::Object(unit)),
                order: current,
                priority: self.order_priority,
                target: self.current_target,
                target_owner: None,
                point: self.target_point,
                range: self.desired_distance,
                shown: order.shown,
                ready: order.ready,
                target_type: self.target_class.and_then(OrderTargetType::of_class),
            };
            self.insert_order(kept, true);
        }
        self.insert_order(order, true);
        if order.order != OrderKind::AttackRoundup {
            self.remove_target();
            self.stop_with(true, place, side, ai)?;
        }
        match &world {
            Some((at, _)) => self.end_notifies(Millis::ZERO, at, side, ai)?,
            None => self.clear_notifies(Millis::ZERO)?,
        }
        let front = *self.orders.checked(0)?;
        if self.process_order(&front, world, place, ai, side)? != OrderOutcome::NotReady {
            let _ = self.orders.remove(0);
        }
        Ok(())
    }

    /// `FUN_140c42640`: an order joins the queue, in front of the first when `in_front` and the
    /// queue holds one, else after the last. A full queue doubles its capacity first, its new
    /// records filled with a stop order (`0x2c2`) to nothing (Vec's own growth here); a failed
    /// allocation, the routine's only other way out, cannot happen (`operator new` throws).
    pub(crate) fn insert_order(&mut self, order: QueuedOrder, in_front: bool) {
        if in_front && !self.orders.is_empty() {
            self.orders.insert(0, order);
        } else {
            self.orders.push(order);
        }
    }

    /// `AttackObject` (slot 0xb8, `FUN_140c43720`): `+0x1d5` clears; not `force`d, an AI with an
    /// action fails (false). The target must pass [`UnitAi::attack_check`] with `any`; then the
    /// unit's attack command ([`UnitAi::attack_command`]), on success the action is the attack
    /// (600, the last action keeping the old) and the target its point and class
    /// (`FUN_140c64c10`). The group flag's attack by the unit's group (behind the byte
    /// `DAT_143f01835`, which nothing writes) cannot run.
    pub(super) fn attack_object(
        &mut self,
        target: ObjectId,
        force: bool,
        any: bool,
        at: &Surroundings,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        if !force && self.current_action.is_some() {
            return Ok(false);
        }
        let Some((class, position)) = self.attack_check(target, any, at, side)? else {
            return Ok(false);
        };
        if !self.attack_command(target, force, at, side)? {
            return Ok(false);
        }
        self.set_action(Some(ActionKind::Attack));
        self.target_at(Some(target), Some(class), position);
        Ok(true)
    }

    /// `AttackObject`'s test of `target` (`FUN_140c43720`, and `attackRoundupObject`'s): the world
    /// must hold it and its slot 0xa0 with `any` must let it attack the target: the base's
    /// (`FUN_140c48400`: the boar's, the ranged units', the ships', the artifact's, the siege
    /// tower's and the livestock's) anything, the building's (`FUN_140c18b60`) nothing, the
    /// lion's ([`lion_may_attack`]), the civilian's ([`civilian_may_attack`]), the soldier's, the
    /// tower's and the siege unit's ([`UnitSide::may_attack`]); the hunted animal's
    /// (`FUN_140c18d00`) reads its action list (slot 0x538) and the priest's (`FUN_140c19020`) its
    /// conversions: not modelled. Its master's class and its place then.
    pub(super) fn attack_check(
        &self,
        target: ObjectId,
        any: bool,
        at: &Surroundings,
        side: &mut dyn UnitSide,
    ) -> Result<Option<(ClassId, Position)>, GenieError> {
        if matches!(
            self.ai_type,
            UnitAiType::SOLDIER | UnitAiType::TOWER | SIEGE_AI
        ) {
            return side.may_attack(target, &self.seeker(at));
        }
        let Some(t) = Self::world_object(target, at, side)? else {
            return Ok(None);
        };
        let may = match self.ai_type {
            // FUN_140c48400, the base's: the boar's, the ranged units' (6, 7), the trade
            // ship's, the transport ship's, the fishing ship's, the artifact's, the siege
            // tower's and the livestock's
            UnitAiType::BOAR
            | UnitAiType::RANGED_6
            | UnitAiType::RANGED_7
            | UnitAiType::TRADE
            | UnitAiType::TRANSPORT
            | UnitAiType::FISHING
            | ARTIFACT_AI
            | SIEGE_TOWER_AI
            | UnitAiType::LIVESTOCK => true,
            UnitAiType::LION => any || lion_may_attack(at.attack_priority, t.class),
            CIVILIAN => civilian_may_attack(at.attack_priority, t.class),
            // FUN_140c18b60
            UnitAiType::BUILDING => false,
            _ => {
                return Err(GenieError::unsupported(
                    "AttackObject's slot 0xa0 of a unit AI type not modelled",
                ));
            }
        };
        Ok(may.then_some((t.class, t.position)))
    }

    /// The unit's attack command (slot 0x338, [`UnitSide::attack`]) on `target`, `force`d or not,
    /// as `AttackObject`, `attackRoundupObject` and seek and destroy give it, at once. The
    /// notifies the command posts (the unit's slot 0x328 -> the AI's slot 0x38) go into the
    /// queue, or the buffer while it is full. True when the unit attacks.
    pub(super) fn attack_command(
        &mut self,
        target: ObjectId,
        force: bool,
        at: &Surroundings,
        side: &mut dyn UnitSide,
    ) -> Result<bool, GenieError> {
        let attacked = side.attack(target, force, self)?;
        for (kind, p) in attacked.posted {
            self.post(Notify::own(at.id, kind, p), at.state)?;
        }
        Ok(attacked.attacks)
    }

    /// The civilian's `AttackObject` (slot 0xb8, `FUN_140c17370`) with its force flag 1, as every
    /// caller in the slice passes it (the damage notify and the unit in the way): an object the
    /// world does not hold (`FUN_1415417a0`) gets nothing (false); an animal or a tree (classes 9,
    /// 10 and 15) is hunted (slot 0xc8, [`UnitAi::hunt_object`]); any other goes to the base
    /// `AttackObject` with `any` 0 and the group flag 1 ([`UnitAi::attack_object`]). True when
    /// it hunts or attacks.
    pub(super) fn civilian_attack(
        &mut self,
        target: ObjectId,
        at: &Surroundings,
        side: &mut dyn UnitSide,
        rng: &mut Well512,
    ) -> Result<bool, GenieError> {
        let t = match at.unit(target) {
            Some(t) => *t,
            None => match side.seen_unit(target)? {
                Some(t) => t,
                None => return Ok(false),
            },
        };
        if matches!(t.class, class::PREY | class::PREDATOR | class::TREE) {
            self.hunt_object(target, t.class, t.position, at, side, rng)?;
            return Ok(true);
        }
        self.attack_object(target, true, false, at, side)
    }

    /// The attackers' upkeep of the full update (`0x140c6741b`): with none, a unit under attack
    /// no longer is (`FUN_14119d2d0` with 0); else the first attacker that the world does not
    /// hold, that has no unit AI, is dead (state above 2) or whose unit AI's target is another
    /// leaves the set (one an update).
    pub(super) fn keep_attackers(&mut self, at: &Surroundings, side: &mut dyn UnitSide) {
        if self.attackers.is_empty() {
            if at.under_attack {
                side.set_under_attack(false);
            }
            return;
        }
        // none (−1) is an object the world does not hold
        self.attackers.remove_first(|id| {
            id.and_then(|id| at.unit(id))
                .is_none_or(|u| ObjectState::Alive < u.state || u.target != Some(at.id))
        });
    }
}
