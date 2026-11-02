//! Where a queued order joins its unit AI's queue ([`QueueEnd`]).

/// Where a queued order joins its unit AI's queue (`FUN_140c42640`'s flag, the AI order's
/// `front`): behind the last order, or in front of the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QueueEnd {
    /// After the last order.
    Back,
    /// Before the first order (after the last when the queue is empty).
    Front,
}
