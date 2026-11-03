//! The orders' flag bytes `+0x13` to `+0x15` ([`OrderFlags`]), which INTERACT and MOVE share.

bitflags::bitflags! {
    /// An order's flags: the bytes `+0x13` to `+0x15` of INTERACT's and MOVE's packets.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct OrderFlags: u8 {
        /// `+0x13`: the order is queued.
        const QUEUED = 1;
        /// `+0x14`: a unit with a unit AI takes the order at once.
        const DIRECT = 1 << 1;
        /// `+0x15`: each unit's last human order time is set (`FUN_140e5e780`).
        const HUMAN = 1 << 2;
    }
}

impl OrderFlags {
    /// The flags of a packet's bytes `+0x13`, `+0x14` and `+0x15`, as the order handlers read
    /// them: queued and direct when not 0, human only at 1.
    #[must_use]
    pub fn of_bytes(queued: u8, direct: u8, human: u8) -> Self {
        let mut flags = Self::empty();
        flags.set(Self::QUEUED, queued != 0);
        flags.set(Self::DIRECT, direct != 0);
        flags.set(Self::HUMAN, human == 1);
        flags
    }

    /// The byte of `flag` in a packet: 1 when it is set, else 0.
    #[must_use]
    pub fn byte(self, flag: Self) -> u8 {
        u8::from(self.contains(flag))
    }
}

#[cfg(test)]
mod tests {
    use crate::orders::OrderFlags;

    /// The bytes read as the handlers read them (human only at 1) and write back as 0 or 1.
    #[test]
    fn order_flags_read_and_write_the_packet_bytes() {
        let flags = OrderFlags::of_bytes(2, 0, 2);
        assert_eq!(flags, OrderFlags::QUEUED);
        assert_eq!(flags.byte(OrderFlags::QUEUED), 1);
        assert_eq!(flags.byte(OrderFlags::DIRECT), 0);
        assert_eq!(
            OrderFlags::of_bytes(0, 1, 1),
            OrderFlags::DIRECT | OrderFlags::HUMAN
        );
    }
}
