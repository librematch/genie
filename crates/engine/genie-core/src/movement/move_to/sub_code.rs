//! What an owner's sub-action list answers its owner ([`SubCode`]).

use crate::objects::UpdateCode;

/// What the update of an owner's sub-action list (`FUN_140a992b0` over its quiet `MoveTo`, or
/// over none) answers the owner: a quiet `MoveTo` gives 0, 1, 3 or 5. It never gives 4, which
/// only a loud one gives for a target it let go (`0x140aa2a70`; no owner makes a loud
/// `MoveTo` to an object, and the import refuses a saved one), nor 2, which only an Attack
/// gives. The owners' arms for 4 and 2 in the game never run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubCode {
    /// 0: the walk goes on.
    UnderWay,
    /// 1: it arrived, or the list holds no `MoveTo`.
    Done,
    /// 3: it failed.
    Failed,
    /// 5: its target left the world or died, and it let the target go.
    TargetGone,
}

impl From<SubCode> for UpdateCode {
    fn from(code: SubCode) -> Self {
        match code {
            SubCode::UnderWay => Self::UnderWay,
            SubCode::Done => Self::Done,
            SubCode::Failed => Self::Failed,
            SubCode::TargetGone => Self::Code5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codes are the list's bytes 0, 1, 3 and 5.
    #[test]
    fn sub_codes_are_the_list_bytes() {
        let bytes = [
            SubCode::UnderWay,
            SubCode::Done,
            SubCode::Failed,
            SubCode::TargetGone,
        ]
        .map(|c| UpdateCode::from(c).get());
        assert_eq!(bytes, [0, 1, 3, 5]);
    }
}
