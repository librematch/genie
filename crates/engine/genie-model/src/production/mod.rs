//! The production values a master holds: its train location's button and its hotkey.

pub mod button;
pub mod hotkey;
pub use self::button::LocationButton;
pub use self::button::QueueButton;
pub use self::hotkey::HotkeyId;
