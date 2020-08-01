//! Virtual file system abstraction
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)
//!
//! The main interaction with the virtual filesystem is by using virtual paths ([`VfsPath`](struct.VfsPath.html)).
//!
//! This crate currently has the following implementations:
//!
//!  * **[`PhysicalFS`](physical/struct.PhysicalFS.html)** - the actual filesystem of the underlying OS
//!  * **[`MemoryFS`](memory/struct.MemoryFS.html)** - an ephemeral in-memory implementation (intended for unit tests)

#[cfg(test)]
#[macro_use]
pub mod test_macros;

pub mod error;
pub mod filesystem;
pub mod memory;
pub mod path;
pub mod physical;

pub use error::{VfsError, VfsResult};
pub use filesystem::FileSystem;
pub use memory::MemoryFS;
pub use path::*;
pub use physical::PhysicalFS;
