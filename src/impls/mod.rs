//! Virtual filesystem implementations

pub mod altroot;
#[cfg(feature = "embedded-fs")]
pub mod embedded;
pub mod memory;
pub mod overlay;
pub mod physical;
