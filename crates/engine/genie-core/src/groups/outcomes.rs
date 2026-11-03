//! What a group's commands and updates answer: whether its move began ([`GroupMove`]), whether
//! the group stays ([`GroupFate`]), whether its command finished ([`Progress`]) and whether a
//! queued command ends the queue's run ([`QueueRun`]); whether a layout was made
//! ([`Layout`]), a sprite shown ([`SpriteShow`]), the orientation set ([`Orientation`]), the
//! members' arrays grown ([`Resize`]), a unit taken in ([`Join`]), a slot reached
//! ([`SlotTry`]) and a member's own move set ([`MemberMove`]).

/// Whether a group's move began (`commandMoveTo`'s, `commandAttack`'s and `BUnitGroup::command`'s
/// 1 or 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum GroupMove {
    /// 1: the group moves.
    Begun,
    /// 0: it does not (it stands, or the move only joined the command queue).
    NotBegun,
}

/// What `BUnitGroup::update` leaves of the group: 1 keeps it, 0 has the world delete it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum GroupFate {
    /// 1: the group stays.
    Kept,
    /// 0: the world deletes it.
    Deleted,
}

/// Whether a group's current command (its move, its follow, its attack) finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Progress {
    /// The command finished.
    Finished,
    /// It goes on.
    Going,
}

/// Whether a queued command ends the queue's run (`check_queued_orders`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum QueueRun {
    /// The command is the group's now: the run ends.
    Ends,
    /// The run goes on to the next command.
    GoesOn,
}

/// Whether a formation's layout (its slots, a line, a box's places) was made: the builders' 1
/// or 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Layout {
    /// 1: made.
    Laid,
    /// 0: not made (nothing to lay out, or a size the builder refuses).
    NotLaid,
}

/// Whether an object shows the sprite it is asked for (slots 0x650, 0x658, 0x660): 0 when it
/// has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum SpriteShow {
    /// It shows the sprite.
    Shown,
    /// It has no such sprite.
    NoSprite,
}

/// Whether a group's motion took a new orientation (`FUN_141049e10`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Orientation {
    /// The motion faces the new direction.
    Set,
    /// Nothing changed: the two vectors' dot product lies outside the bounds.
    Unchanged,
}

/// Whether the members' arrays grew (`FUN_14102d710`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Resize {
    /// The arrays hold the new capacity.
    Grown,
    /// Nothing changed: the size is 0 or less, or not above the capacity.
    Unchanged,
}

/// Whether `AddUnit` took the unit into the group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Join {
    /// The unit is a member now (or was one).
    Joined,
    /// The group does not take it.
    Refused,
}

/// Whether a member reached its due slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum SlotTry {
    /// It has a way there and stands on it.
    Placed,
    /// It has none.
    NoWay,
}

/// Whether a member's own move was set (`FUN_141039f60`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum MemberMove {
    /// The move is set (or ended).
    Set,
    /// The unit is no member, or the world does not hold it.
    NoMember,
}
