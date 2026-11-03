//! Callback A's token: what a ray from a polygon corner ignores.

use crate::pathing::{ShapeId, ShapeSide};

/// Callback A's token (`DAT_143f0f360`): what a ray from a polygon corner ignores, as the game's
/// one integer: a side (0 to 3, a touch on that side of the box at the corner), a shape (its
/// id plus 4, a touch at its corner) or none (0xff). Tokens compare by that integer, so shape
/// 251's token is the none token, as in the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct Token(u32);

impl Token {
    /// No side: a corner whose sides leave the ray none to ignore (`FUN_140fa3bc0`'s 0xff).
    pub(super) const NONE: Self = Self(0xff);

    /// The token of a side.
    pub(super) const fn of_side(side: ShapeSide) -> Self {
        Self(side as u32)
    }

    /// The token of a shape: its id + 4, in the game's 32 bits.
    pub(super) const fn of_shape(shape: ShapeId) -> Self {
        Self(shape.get().wrapping_add(4))
    }
}

#[cfg(test)]
mod tests {
    use crate::pathing::short_path::token::Token;
    use crate::pathing::{ShapeId, ShapeSide};

    /// A side's token is its number, a shape's its id + 4: shape 251's is the none token, as
    /// the game compares them.
    #[test]
    fn shape_251_token_is_the_none_token() {
        assert_eq!(Token::of_side(ShapeSide::LowX), Token(3));
        assert_eq!(Token::of_shape(ShapeId::new(0)), Token(4));
        assert_eq!(Token::of_shape(ShapeId::new(251)), Token::NONE);
        assert_eq!(
            Token::of_shape(ShapeId::new(u32::MAX)),
            Token::of_side(ShapeSide::LowX)
        );
    }
}
