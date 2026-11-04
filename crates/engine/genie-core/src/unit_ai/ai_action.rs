//! The unit AI's action values as one type: every value its action field (`+0x44`) can hold,
//! each by the order it serves, and the fallible conversion from a raw value.

use crate::unit_ai::OrderKind;
use crate::{GenieError, GenieResult};
use genie_diff::Diff;

/// An action value a unit AI holds (`+0x44`, the last one `+0x68`). Where an order serves it,
/// the value is that order's less 100 ([`AiAction::order`]), and the variant takes the
/// order's name. The value setter's (`FUN_140c64b50`) 44 calls give no other value (these, −1,
/// or a value saved or read before), nor do the unit AI's own stores, nor the work command's
/// notify (`FUN_14106e4e0`); a save's raw value converts fallibly ([`AiAction::try_from`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Diff)]
#[repr(i32)]
pub enum AiAction {
    /// 600: attacking.
    Attack = 0x258,
    /// `0x259`: defending.
    Defend = 0x259,
    /// `0x25a`: building, which the back-to-work record keeps.
    Build = 0x25a,
    /// `0x25b`: healing.
    Heal = 0x25b,
    /// `0x25c`: converting.
    Convert = 0x25c,
    /// `0x25d`: exploring.
    Explore = 0x25d,
    /// `0x261`: gathering.
    Gather = 0x261,
    /// `0x262`: moving.
    Move = 0x262,
    /// `0x264`: following.
    Follow = 0x264,
    /// `0x265`: hunting.
    Hunt = 0x265,
    /// `0x266`: transporting.
    Transport = 0x266,
    /// `0x267`: trading.
    Trade = 0x267,
    /// `0x269`: entering.
    Enter = 0x269,
    /// `0x26a`: repairing.
    Repair = 0x26a,
    /// `0x26b`: making a unit, which a building's make action sets.
    Make = 0x26b,
    /// `0x26c`: researching, which RESEARCH sets on the building.
    Research = 0x26c,
    /// `0x26d`: unloading.
    Unload = 0x26d,
    /// `0x26e`: seeking and destroying.
    SeekAndDestroy = 0x26e,
    /// `0x26f`: exploring and destroying.
    ExploreAndDestroy = 0x26f,
    /// `0x274`: waiting.
    Wait = 0x274,
    /// `0x276`: scouting.
    Scout = 0x276,
    /// `0x277`: acting on a target, which the actions on one set with their order
    /// ([`OrderKind::OnTarget`]).
    OnTarget = 0x277,
    /// `0x278`: shepherding.
    Shepherd = 0x278,
    /// `0x27a`: waiting for a foundation to solidify ([`OrderKind::Foundation`]).
    WaitSolidify = 0x27a,
    /// 700: unloading over a wall.
    UnloadOverWall = 0x2bc,
    /// `0x2bd`: auto scouting.
    AutoScout = 0x2bd,
    /// `0x2be`: luring a boar ([`OrderKind::AutoBoarLuring`]).
    BoarLuring = 0x2be,
    /// `0x2bf`: moving aside for another unit.
    MoveAside = 0x2bf,
    /// `0x2c0`: moving aside for a group.
    GroupAside = 0x2c0,
    /// `0x2c2`: using a hero power.
    HeroPower = 0x2c2,
}

impl AiAction {
    /// Every action value, by value.
    pub const ALL: [Self; 30] = [
        Self::Attack,
        Self::Defend,
        Self::Build,
        Self::Heal,
        Self::Convert,
        Self::Explore,
        Self::Gather,
        Self::Move,
        Self::Follow,
        Self::Hunt,
        Self::Transport,
        Self::Trade,
        Self::Enter,
        Self::Repair,
        Self::Make,
        Self::Research,
        Self::Unload,
        Self::SeekAndDestroy,
        Self::ExploreAndDestroy,
        Self::Wait,
        Self::Scout,
        Self::OnTarget,
        Self::Shepherd,
        Self::WaitSolidify,
        Self::UnloadOverWall,
        Self::AutoScout,
        Self::BoarLuring,
        Self::MoveAside,
        Self::GroupAside,
        Self::HeroPower,
    ];

    /// Its value, as the unit AI's fields and the save hold it.
    #[must_use]
    pub const fn value(self) -> i32 {
        self as i32
    }

    /// The order whose own action it is, the value 100 above it (the notify handlers'
    /// `action + 100 == order`); None where no order holds that value.
    #[must_use]
    pub fn order(self) -> Option<OrderKind> {
        OrderKind::try_from(self.value() + 100).ok()
    }

    /// The action of a raw field that holds −1 for none (`+0x44`, `+0x68`).
    ///
    /// # Errors
    ///
    /// A value no action value names.
    pub fn of_field(value: i32) -> GenieResult<Option<Self>> {
        (value != -1).then(|| Self::try_from(value)).transpose()
    }

    /// The value of a field that holds −1 for none.
    #[must_use]
    pub const fn field(action: Option<Self>) -> i32 {
        match action {
            Some(ai_action) => ai_action.value(),
            None => -1,
        }
    }
}

impl TryFrom<i32> for AiAction {
    type Error = GenieError;

    /// The action of value `value`.
    fn try_from(value: i32) -> GenieResult<Self> {
        Self::ALL
            .into_iter()
            .find(|ai_action| ai_action.value() == value)
            .ok_or_else(|| {
                GenieError::invalid_input("an action value no unit AI's action names")
                    .with("action", value)
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;
    use crate::unit_ai::{AiAction, OrderKind};

    /// Every action value converts back from its value; the values are 30 distinct ones.
    #[test]
    fn every_action_converts_back_from_its_value() {
        for ai_action in AiAction::ALL {
            assert_eq!(AiAction::try_from(ai_action.value()), Ok(ai_action));
        }
        let mut values: Vec<i32> = AiAction::ALL
            .iter()
            .map(|ai_action| ai_action.value())
            .collect();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), 30);
        assert_eq!(
            (AiAction::Attack.value(), AiAction::HeroPower.value()),
            (600, 0x2c2)
        );
    }

    /// A value no action names is an error; −1 is a field's none.
    #[test]
    fn value_no_action_names_is_an_error() {
        for value in [0, 0x25e, 0x263, 0x2c1, 0x2c6, 700 + 100] {
            assert_eq!(
                AiAction::try_from(value).map_err(|error| error.kind()),
                Err(ErrorKind::InvalidInput)
            );
        }
        assert_eq!(AiAction::of_field(-1), Ok(None));
        assert_eq!(AiAction::of_field(0x262), Ok(Some(AiAction::Move)));
        assert_eq!(AiAction::field(None), -1);
        assert_eq!(AiAction::field(Some(AiAction::Hunt)), 0x265);
    }

    /// An action's order is the value 100 above it: the move's, the trade's, the hero power's;
    /// the make, research and the two moves aside have none.
    #[test]
    fn an_action_order_is_100_above_it() {
        assert_eq!(AiAction::Move.order(), Some(OrderKind::Move));
        assert_eq!(AiAction::Trade.order(), Some(OrderKind::Trade));
        assert_eq!(AiAction::HeroPower.order(), Some(OrderKind::HeroPower));
        assert_eq!(AiAction::OnTarget.order(), Some(OrderKind::OnTarget));
        for ai_action in [
            AiAction::Make,
            AiAction::Research,
            AiAction::MoveAside,
            AiAction::GroupAside,
        ] {
            assert_eq!(ai_action.order(), None, "{ai_action:?}");
        }
    }
}
