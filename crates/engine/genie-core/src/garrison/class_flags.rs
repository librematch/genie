//! A class's garrison flags ([`ClassFlags`], the class-flags table's word).

bitflags::bitflags! {
    /// The flags the class-flags table (`DAT_1439e37b0`, a u32 per class) holds for a class,
    /// which `FUN_1411205e0` ([`crate::garrison::class_has_flags`]) tests. Only the transported
    /// pair has a reader that names it; every bit is kept (`_ = !0`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ClassFlags: u32 {
        /// The flags a unit garrisons a transport with (`FUN_14120eb10`, `FUN_141621220`: bits
        /// 1 and 16).
        const TRANSPORTED = 0x1_0002;
        /// Any other bit.
        const _ = !0;
    }
}

#[cfg(test)]
mod tests {
    use crate::garrison::ClassFlags;

    /// The transported pair is bits 1 and 16; a word keeps its unnamed bits.
    #[test]
    fn the_transported_pair_is_bits_1_and_16() {
        assert_eq!(ClassFlags::TRANSPORTED.bits(), 0x1_0002);
        assert!(ClassFlags::from_bits_retain(0x2).intersects(ClassFlags::TRANSPORTED));
        assert!(
            !ClassFlags::from_bits_retain(0x403).intersects(ClassFlags::from_bits_retain(0x10))
        );
        assert_eq!(ClassFlags::from_bits_retain(0x140).bits(), 0x140);
    }
}
