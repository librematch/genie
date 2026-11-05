//! The unit AI's alert mode ([`AlertMode`]) and the object it watches with it on
//! ([`UnitAi::watch_alert`]).

use crate::GenieResult;
use crate::unit_ai::UnitAi;
use crate::unit_ai::UnitAiKind;
use crate::unit_ai::world_view::UnitView;

/// The unit AI's alert mode (`+0x1b8`, `alertModeValue`): whether its last search found enemies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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

impl UnitAi {
    /// With the alert mode on, the alert object (`+0x1bc`) takes the unit AI's slot 0x1d8: the
    /// soldier's (`FUN_140c1c6e0`) is its nearest armed enemy of the last search
    /// ([`UnitView::nearest_armed_enemy`]), any other factory type's the base's −1
    /// (`FUN_140c1c7b0`). The soldier's and the siege unit's idle checks call it.
    ///
    /// # Errors
    ///
    /// An enemy without its master.
    pub(crate) fn watch_alert(&mut self, side: &UnitView<'_, '_>) -> GenieResult<()> {
        if self.alert_mode == AlertMode::Alert {
            self.alert_object = if self.ai_kind == UnitAiKind::Soldier {
                side.nearest_armed_enemy()?
            } else {
                None
            };
        }
        Ok(())
    }
}

impl genie_diff::Diff for AlertMode {
    fn diff(&self, other: &Self, differ: &mut genie_diff::Differ) {
        differ.equal(self, other);
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
