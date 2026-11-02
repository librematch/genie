//! Where a unit takes its resources: the drop sites ([`DropSite`]) per resource.

pub mod drop_site;
pub use self::drop_site::DropSite;
pub use self::drop_site::DropTarget;
