//! The unit AI's alert mode ([`AlertMode`]).

/// The unit AI's alert mode (`+0x1b8`, `alertModeValue`): whether its last search found enemies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertMode {
    /// 0: the last search found none.
    #[default]
    Calm,
    /// 1: it found enemies.
    Alert,
}

impl AlertMode {
    /// The mode a search leaves that found enemies (`enemies`) or none.
    #[must_use]
    pub const fn of_search(enemies: bool) -> Self {
        if enemies { Self::Alert } else { Self::Calm }
    }

    /// The mode of the save's byte: 0 calm, any other value alert.
    #[must_use]
    pub const fn of_data(value: u8) -> Self {
        Self::of_search(value != 0)
    }
}

impl genie_diff::Diff for AlertMode {
    fn diff(&self, other: &Self, d: &mut genie_diff::Differ) {
        d.equal(self, other);
    }
}

#[cfg(test)]
mod tests {
    use crate::unit_ai::AlertMode;

    /// A search's enemies make the mode alert; the save's 0 is calm, any other byte alert.
    #[test]
    fn alert_mode_is_alert_on_enemies_and_on_any_byte_but_zero() {
        assert_eq!(AlertMode::of_search(true), AlertMode::Alert);
        assert_eq!(AlertMode::of_search(false), AlertMode::Calm);
        assert_eq!(
            [0, 1, 2].map(AlertMode::of_data),
            [AlertMode::Calm, AlertMode::Alert, AlertMode::Alert]
        );
    }
}
