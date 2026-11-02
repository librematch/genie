//! The world's values that game data holds: its clock's units (`Seconds`, `WholeSeconds`), the
//! game's options (`Age`) and settings.

pub mod clock;
pub mod options;
pub mod settings;
pub use self::clock::Millis;
pub use self::clock::Seconds;
pub use self::clock::WholeSeconds;
pub use self::clock::WorldTime;
pub use self::options::Age;
pub use self::options::ContentMode;
pub use self::options::GameMode;
pub use self::options::ResourceLevel;
pub use self::options::SubModeFlags;
pub use self::options::TreatyLength;
pub use self::options::Visibility;
