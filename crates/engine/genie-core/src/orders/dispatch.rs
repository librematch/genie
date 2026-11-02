//! How an order reaches its units ([`Dispatch`]).

/// How an order reaches its units: at once, or queued in their unit AIs behind their other
/// orders (slot 0x28's `immediate`, the AI order's: 0 queues).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dispatch {
    /// At once: a garrison's unit takes its task (slot 0x7e8), a unit AI its order (slot 0x28
    /// with `immediate` 1).
    Now,
    /// Into the unit AI's queue (slot 0x28 with `immediate` 0, slot 0x30 into it); a unit
    /// without a unit AI takes a garrison's task at once.
    Queued,
}
