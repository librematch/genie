//! An object's reload time (slot 0x218, `FUN_141086c90`): the time between its attacks, from its
//! master's and its own values, which technologies and effects change.

use crate::objects::masters::MasterKind;
use crate::world::Seconds;

/// Slot 0x218 (`FUN_141086c90`) of an object of master type `kind`: from type 60 on the master's
/// reload time `master_reload` (`speed_of_attack`, which technologies change) with the object's
/// reload stinger `stinger` added (slot 0x758, `+0xb40`) and its reload powerup `powerup` taken
/// off (slot 0x6e0, `+0xa28`), in the game's order; below type 60 the base's 0
/// (`FUN_14106d570`).
#[must_use]
pub fn reload_time(
    kind: MasterKind,
    master_reload: Seconds,
    stinger: Seconds,
    powerup: Seconds,
) -> Seconds {
    if kind < MasterKind::MISSILE {
        return Seconds::ZERO;
    }
    full_reload(master_reload, stinger, powerup)
}

/// The reload time's sum (slot 0x218 from type 60 on, and `FUN_141087040`, which sets a ranged
/// unit's attack timer `+0x91c` to it as it leaves a siege unit): the stinger `stinger` (slot
/// 0x758) plus the master's reload time `master_reload` (`+0x838`) less the powerup `powerup`
/// (slot 0x6e0), in the game's order.
#[must_use]
pub fn full_reload(master_reload: Seconds, stinger: Seconds, powerup: Seconds) -> Seconds {
    stinger + (master_reload - powerup)
}

#[cfg(test)]
mod tests {
    use crate::combat::reload::reload_time;
    use crate::objects::masters::MasterKind;
    use crate::world::Seconds;

    /// From type 60 on the master's time takes the stinger and loses the powerup; a technology
    /// that changes the master's time changes the result; below type 60 it is 0.
    #[test]
    fn reload_time_adds_the_stinger_to_the_master_reload_less_the_powerup() {
        let reload = |kind, master: f32| {
            reload_time(
                kind,
                Seconds::new(master),
                Seconds::new(0.5),
                Seconds::new(0.25),
            )
            .get()
        };
        assert_eq!(reload(MasterKind::COMBAT, 2.0), 2.25);
        assert_eq!(reload(MasterKind::COMBAT, 1.7), 0.5 + (1.7 - 0.25));
        assert_eq!(reload(MasterKind::MISSILE, 2.0), 2.25);
        assert_eq!(reload(MasterKind::ACTION, 2.0), 0.0);
    }
}
