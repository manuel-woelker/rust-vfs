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
//!  * **[`AltrootFS`](impls/altroot/struct.AltrootFS.html)** - a file system with its root in a particular directory of another filesystem
//!  * **[`OverlayFS`](impls/overlay/struct.OverlayFS.html)** - a union file system consisting of a read/writable upper layer and several read-only lower layers
//!
//! # Usage Examples
//!
//! ```
//! use vfs::{VfsPath, PhysicalFS, VfsError};
//!
//! # fn main() -> vfs::VfsResult<()> {
//! let root: VfsPath = PhysicalFS::new(std::env::current_dir().unwrap()).into();
//! assert!(root.exists());
//!
//! let mut content = String::new();
//! root.join("README.md")?.open_file()?.read_to_string(&mut content)?;
//! assert!(content.contains("vfs"));
//! # Ok::<(), VfsError>(())
//! # }
//! ```
//!
//! ```
//! use vfs::{VfsPath, VfsError, MemoryFS};
//!
//! # fn main() -> vfs::VfsResult<()> {
//! let root: VfsPath = MemoryFS::new().into();
//! let path = root.join("test.txt")?;
//! assert!(!path.exists());
//!
//! path.create_file()?.write_all(b"Hello world")?;
//! assert!(path.exists());
//! let mut content = String::new();
//! path.open_file()?.read_to_string(&mut content)?;
//! assert_eq!(content, "Hello world");
//! # Ok::<(), VfsError>(())
//! # }
//! ```
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
