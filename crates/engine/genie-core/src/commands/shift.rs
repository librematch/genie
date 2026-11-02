//! A command's shift flag ([`Shift`]).

use crate::orders::Dispatch;
use crate::production::Batch;

/// `UNIT_ORDER`'s `+0x1d`: the UI's shift key with the order. Its two readers are shift's two
/// uses: the garrison (`FUN_14161e370` case 5) queues its enter orders behind the units' others
/// ([`Shift::dispatch`]), a cancel (case 4) takes back five of a kind ([`Shift::batch`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Shift {
    /// 0: the plain order.
    #[default]
    Released,
    /// Any other byte: shift held.
    Held,
}

impl Shift {
    /// The packet's byte: 0 is released, any other value held (the decoder's test).
    #[must_use]
    pub const fn of_byte(byte: u8) -> Self {
        if byte == 0 {
            Self::Released
        } else {
            Self::Held
        }
    }

    /// Its byte in a packet: 0 or 1.
    #[must_use]
    pub const fn byte(self) -> u8 {
        match self {
            Self::Released => 0,
            Self::Held => 1,
        }
    }

    /// How a garrison gives its units their enter orders: through their unit AIs' queues with
    /// shift held.
    #[must_use]
    pub const fn dispatch(self) -> Dispatch {
        match self {
            Self::Released => Dispatch::Now,
            Self::Held => Dispatch::Queued,
        }
    }

    /// How many units of a kind a cancel takes back: five with shift held.
    #[must_use]
    pub const fn batch(self) -> Batch {
        match self {
            Self::Released => Batch::One,
            Self::Held => Batch::Five,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::Shift;
    use crate::orders::Dispatch;
    use crate::production::Batch;

    /// Any byte but 0 is shift held, which writes back as 1; held queues and takes five.
    #[test]
    fn any_byte_but_0_is_shift_held() {
        assert_eq!(Shift::of_byte(0), Shift::Released);
        assert_eq!(Shift::of_byte(1), Shift::Held);
        assert_eq!(Shift::of_byte(7).byte(), 1);
        assert_eq!(Shift::Released.byte(), 0);
        assert_eq!(Shift::Held.dispatch(), Dispatch::Queued);
        assert_eq!(Shift::Released.dispatch(), Dispatch::Now);
        assert_eq!(Shift::Held.batch(), Batch::Five);
        assert_eq!(Shift::Released.batch(), Batch::One);
    }
}
