//! Virtual file system abstraction
//!
//! The virtual file system abstraction generalizes over file systems and allow using
//! different VirtualFileSystem implementations (i.e. an in memory implementation for unit tests)
//!
//! The main interaction with the virtual filesystem is by using virtual paths ([`VfsPath`](path/struct.VfsPath.html)).
//!
//! This crate currently has the following implementations:
//!
//!  * **[`PhysicalFS`](impls/physical/struct.PhysicalFS.html)** - the actual filesystem of the underlying OS
//!  * **[`MemoryFS`](impls/memory/struct.MemoryFS.html)** - an ephemeral in-memory implementation (intended for unit tests)
//!  * **[`AltrootFS`](impls/altroot/struct.Altroot.html)** - a file system with its root in a particular directory of another filesystem

#[cfg(test)]
#[macro_use]
pub mod test_macros;

pub mod error;
pub mod filesystem;
pub mod impls;
pub mod path;

pub use error::{VfsError, VfsResult};
pub use filesystem::FileSystem;
pub use impls::memory::MemoryFS;
pub use impls::physical::PhysicalFS;
pub use path::*;
