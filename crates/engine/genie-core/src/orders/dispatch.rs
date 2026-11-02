//! How an order reaches its units ([`Dispatch`]).

/// How an order reaches its units: each takes it at once, or one with a unit AI has its unit AI
/// queue it behind its other orders (slot 0x30 into slot 0x28, not immediate, behind).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Dispatch {
    /// The unit takes the order at once (a garrison's slot 0x7e8).
    #[default]
    Now,
    /// A unit with a unit AI has it queue the order; one without takes it at once.
    Queued,
}
