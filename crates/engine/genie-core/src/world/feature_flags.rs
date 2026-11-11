//! The game's feature flags for experimental bug fixes.

/// The game's feature flags for experimental bug fixes (`DAT_143f01830` to `DAT_143f01834`,
/// which `FUN_140ecabe0` sets together). `World::load_world` turns them on for a world whose
/// saved `mUseFeatureFlags` (`+0x857`, read for save versions from 0x200000) is 1, or under the
/// option that forces them in saves and recordings (`DAT_143fc5ca8`), which a playback leaves
/// off. The world's constructor (`FUN_141599e50`) sets them by that option or the one that
/// toggles them (`DAT_143fc5bd8`) before the load.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FeatureFlags {
    /// Off, as in every recording seen.
    #[default]
    Off,
    /// On.
    On,
}

impl FeatureFlags {
    /// The flags of a world whose saved `mUseFeatureFlags` is `saved`: on for 1 alone (the load's
    /// `CMP byte ptr [R15 + 0x857], 0x1` at 0x1415d531d).
    #[must_use]
    pub const fn of_saved(saved: u8) -> Self {
        if saved == 1 { Self::On } else { Self::Off }
    }
}
