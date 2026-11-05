//! Whether a combat object shows its master's special sprite.

/// A combat object's `using_special_attack_animation` (`+0xa05`, a saved byte): whether it shows
/// its master's special sprite, which the combat update (`TRIBE_Combat_Object::update`) shows on a
/// master of special attack 2 with a special sprite while its unit AI is alert and idle with no
/// attackers and its action list is empty, and hides otherwise. The sprite itself is the
/// renderer's (`FUN_1413b7bf0`, `FUN_1413ba8d0`, `FUN_1413b9e20` on the object's sprite list).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SpecialSprite {
    /// 0: the master's usual sprites.
    #[default]
    Hidden,
    /// Any other byte: the special sprite shows.
    Shown,
}

impl SpecialSprite {
    /// The state of the save's byte `value`: any byte but 0 shows the sprite (the update tests
    /// the byte against 0).
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        if value == 0 {
            Self::Hidden
        } else {
            Self::Shown
        }
    }
}

impl genie_diff::Diff for SpecialSprite {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::combat::SpecialSprite;

    /// Any byte but 0 shows the sprite.
    #[test]
    fn special_sprite_shows_for_any_byte_but_zero() {
        assert_eq!(SpecialSprite::of_data(0), SpecialSprite::Hidden);
        assert_eq!(SpecialSprite::of_data(1), SpecialSprite::Shown);
        assert_eq!(SpecialSprite::of_data(7), SpecialSprite::Shown);
    }
}
