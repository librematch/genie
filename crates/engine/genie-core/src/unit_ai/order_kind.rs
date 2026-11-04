//! The unit AI's order values as one type: the cases of its order handler (`FUN_140c5c170`,
//! slot 0x230) and the three orders set without a case, each by its name, and the fallible
//! conversion from a raw value.

use crate::unit_ai::{AiAction, ai_kinds};
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// An order a unit AI holds: a case of its order handler (`FUN_140c5c170`, slot 0x230), named
/// by the line the case logs (`ai_kinds`, generated) or, where it logs none, by what it does,
/// and three orders no case names: the shepherd's, which its action sets directly, and the trade's
/// and the hero power's, which the work command's notify sets (`FUN_14106e4e0`). The order
/// setter's (`FUN_140c64b60`) 38 calls give no other value (these, −1, or an order saved or read
/// before), and the handler's tail stores only its cases', so a save's or a script's raw value
/// converts fallibly ([`OrderKind::try_from`]). The game also compares the current order with
/// `0x262`, `0x269`, `0x2cf`, `0x2d0`, `0x323` and `0x324` (the full update, the leash,
/// `AddUnit`), which nothing stores there: those tests never hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Diff)]
#[repr(i32)]
pub enum OrderKind {
    /// 700: attack the target (slot 0xb8, `AttackObject`).
    Attack = ai_kinds::ORDER_ATTACK,
    /// `0x2bd`: defend a target or a point ("I have a defend order…", after a check that returns
    /// first, so the generated names miss it).
    Defend = 0x2bd,
    /// `0x2be`: build the foundation (slot 0xf0).
    Build = ai_kinds::ORDER_BUILD,
    /// `0x2bf`: heal the target (slot 0xe0).
    Heal = ai_kinds::ORDER_HEAL,
    /// `0x2c0`: convert the target (slot 0xd8).
    Convert = ai_kinds::ORDER_CONVERT,
    /// `0x2c1`: explore to the point (slot 0x100).
    Explore = ai_kinds::ORDER_EXPLORE,
    /// `0x2c2`: stop ("I have a stop order.", after "SO Call 6."), which the order queue's new
    /// records hold (`FUN_140c42640`).
    Stop = 0x2c2,
    /// `0x2c3`: run away (slot 0x160; "I have a runAway order…").
    RunAway = 0x2c3,
    /// `0x2c5`: gather from the target (slot 0xd0).
    Gather = ai_kinds::ORDER_GATHER,
    /// `0x2c6`: move to the point (slot 0x130).
    Move = ai_kinds::ORDER_MOVEMENT,
    /// `0x2c7`: patrol ("I don't know what to do with patrol orders, skipping it."): the case
    /// takes none, the patrol's restore sets it.
    Patrol = 0x2c7,
    /// `0x2c8`: follow the target within the order's range (slot 0x168).
    Follow = 0x2c8,
    /// `0x2c9`: hunt the target (slot 0xc8).
    Hunt = ai_kinds::ORDER_HUNT,
    /// `0x2ca`: transport to the point (slot 0x128).
    Transport = ai_kinds::ORDER_TRANSPORT,
    /// `0x2cb`: trade, with the action value `0x267`, which the work command's notify sets for a
    /// trade action (type `0x6f`, `FUN_14106e4e0`); no case of the handler names it.
    Trade = 0x2cb,
    /// `0x2cd`: enter the target (slot 0x110).
    Enter = ai_kinds::ORDER_ENTER,
    /// `0x2ce`: repair the target (slot 0xe8).
    Repair = ai_kinds::ORDER_REPAIR,
    /// `0x2d1`: unload at the point (slot 0x118).
    Unload = ai_kinds::ORDER_UNLOAD,
    /// `0x2d2`: seek and destroy (slot 0x180).
    SeekAndDestroy = ai_kinds::ORDER_SEEK_AND_DESTROY,
    /// `0x2d3`: explore and destroy (slot 0x188).
    ExploreAndDestroy = ai_kinds::ORDER_EXPLORE_AND_DESTROY,
    /// `0x2d4`: gather as a group (slot 0x130 to the order's point).
    GroupGather = ai_kinds::ORDER_GROUP_GATHER,
    /// `0x2d8`: wait (the action value `0x274`).
    Wait = ai_kinds::ORDER_WAIT,
    /// `0x2d9`: attack roundup (slot 0xc0).
    AttackRoundup = ai_kinds::ORDER_ATTACK_ROUNDUP,
    /// `0x2da`: scout to the point (slot 0x100, then the action value `0x276`).
    Scout = ai_kinds::ORDER_SCOUT,
    /// `0x2db`: the order an INTERACT queues on its target (the case logs no line).
    OnTarget = 0x2db,
    /// `0x325`: an order on a target as [`OrderKind::OnTarget`], whose case it shares.
    OnTarget2 = 0x325,
    /// `0x2dd`: attack move (the case logs no line).
    AttackMove = 0x2dd,
    /// `0x2de`: the foundation a villager sent off its footprint takes up again (slot 0x108;
    /// the case logs no line).
    Foundation = 0x2de,
    /// 800: unload over a wall (slot 0x120).
    UnloadOverWall = ai_kinds::ORDER_UNLOAD_OVER_WALL,
    /// `0x321`: auto scout (slot 0x100, then the action value `0x2bd`).
    AutoScout = ai_kinds::ORDER_AUTO_SCOUT,
    /// `0x322`: auto boar luring (slot 0xc8, then the action value `0x2be`).
    AutoBoarLuring = ai_kinds::ORDER_AUTO_BOAR_LURING,
    /// `0x2dc`: the shepherd's, with the action value `0x278`, which the Shepherd action's
    /// constructors (`FUN_140adb610`, `FUN_140adb7a0`) and an order on a target of action type
    /// 0x95 (`FUN_14121c890`) set; no case of the handler names it.
    Shepherd = 0x2dc,
    /// `0x326`: the hero power, with the action value `0x2c2`, which the work command's notify
    /// sets for an `ActionHeroPower` (type `0x7e`, `FUN_14106e4e0`); no case of the handler names
    /// it.
    HeroPower = 0x326,
}

impl OrderKind {
    /// Every order: the handler's cases in its switch's order, then the ones it names no case for.
    pub const ALL: [Self; 33] = [
        Self::Attack,
        Self::Defend,
        Self::Build,
        Self::Heal,
        Self::Convert,
        Self::Explore,
        Self::Stop,
        Self::RunAway,
        Self::Gather,
        Self::Move,
        Self::Patrol,
        Self::Follow,
        Self::Hunt,
        Self::Transport,
        Self::Enter,
        Self::Repair,
        Self::Unload,
        Self::SeekAndDestroy,
        Self::ExploreAndDestroy,
        Self::GroupGather,
        Self::Wait,
        Self::AttackRoundup,
        Self::Scout,
        Self::OnTarget,
        Self::OnTarget2,
        Self::AttackMove,
        Self::Foundation,
        Self::UnloadOverWall,
        Self::AutoScout,
        Self::AutoBoarLuring,
        Self::Shepherd,
        Self::Trade,
        Self::HeroPower,
    ];

    /// Its value, as the unit AI's fields and the save hold it.
    #[must_use]
    pub const fn value(self) -> i32 {
        self as i32
    }

    /// Whether `action` is the order's own action value, the order's less 100 (the notify
    /// handlers' `action + 100 == order`; no action, −1, is no order's).
    #[must_use]
    pub fn owns(self, action: Option<AiAction>) -> bool {
        action.and_then(AiAction::order) == Some(self)
    }

    /// The order of a raw field that holds −1 for none (`+0x3c`, `+0x64`).
    ///
    /// # Errors
    ///
    /// A value no case of the order handler names.
    pub fn of_field(value: i32) -> GenieResult<Option<Self>> {
        (value != -1).then(|| Self::try_from(value)).transpose()
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(order: Option<Self>) -> i32 {
        match order {
            Some(order_kind) => order_kind.value(),
            None => -1,
        }
    }
}

impl TryFrom<i32> for OrderKind {
    type Error = GenieError;

    /// The order of value `value`.
    fn try_from(value: i32) -> GenieResult<Self> {
        Self::ALL
            .into_iter()
            .find(|order_kind| order_kind.value() == value)
            .ok_or_else(|| {
                GenieError::invalid_input(
                    "an order value no case of the unit AI's order handler names",
                )
                .with("order", value)
            })
    }
}

/// The queue's new records hold a stop order (`FUN_140c42640`'s fill).
impl Default for OrderKind {
    fn default() -> Self {
        Self::Stop
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::unit_ai::{AiAction, OrderKind};

    /// Every order converts back from its value; the values are 33 distinct ones.
    #[test]
    fn every_order_converts_back_from_its_value() {
        for order_kind in OrderKind::ALL {
            assert_eq!(OrderKind::try_from(order_kind.value()), Ok(order_kind));
        }
        let mut values: Vec<i32> = OrderKind::ALL
            .iter()
            .map(|order_kind| order_kind.value())
            .collect();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), 33);
        assert_eq!(
            (OrderKind::Attack.value(), OrderKind::OnTarget2.value()),
            (700, 0x325)
        );
        assert_eq!(
            (OrderKind::Trade.value(), OrderKind::HeroPower.value()),
            (0x2cb, 0x326)
        );
    }

    /// A value no case names is an error; −1 is a field's none.
    #[test]
    fn value_no_case_names_is_an_error() {
        for value in [0, 0x2c4, 0x262, 0x269, 0x2cf, 0x2d0, 0x323] {
            assert_eq!(
                OrderKind::try_from(value).map_err(|error| error.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        assert_eq!(OrderKind::of_field(-1), Ok(None));
        assert_eq!(OrderKind::of_field(0x2c6), Ok(Some(OrderKind::Move)));
        assert_eq!(OrderKind::field(None), -1);
        assert_eq!(OrderKind::field(Some(OrderKind::Stop)), 0x2c2);
    }

    /// An order's own action value is its value less 100: the move's `0x262`, the follow's
    /// `0x264`; no action is no order's.
    #[test]
    fn an_order_owns_the_action_100_below_it() {
        assert!(OrderKind::Move.owns(Some(AiAction::Move)));
        assert!(OrderKind::Follow.owns(Some(AiAction::Follow)));
        assert!(!OrderKind::Move.owns(Some(AiAction::Follow)));
        assert!(!OrderKind::Attack.owns(None));
        assert!(OrderKind::Defend.owns(Some(AiAction::Defend)));
    }
}
