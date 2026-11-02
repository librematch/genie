//! What a stop of the unit AI's current action does to its order ([`StopOrder`]).

/// Slot 0xb0's (`FUN_140c64ed0`) second argument: a stop that keeps the unit's order (1) or
/// ends it with the action (0). A patrol order, an attack-move and an order the action belongs
/// to end either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StopOrder {
    /// 0: the order ends with the action.
    End,
    /// 1: the order stays, unless the action is its own or it is a patrol.
    Keep,
}
