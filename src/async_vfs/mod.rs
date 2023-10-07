//! Asynchronous port of virtual file system abstraction
//!
//!
//! Just as with the synchronous version, the main interaction with the virtual filesystem is by using virtual paths ([`AsyncVfsPath`](path/struct.AsyncVfsPath.html)).
//!
//! This module currently has the following asynchronous file system implementations:
//!
//!  * **[`AsyncPhysicalFS`](impls/physical/struct.AsyncPhysicalFS.html)** - the actual filesystem of the underlying OS
//!  * **[`AsyncMemoryFS`](impls/memory/struct.AsyncMemoryFS.html)** - an ephemeral in-memory implementation (intended for unit tests)
//!  * **[`AsyncAltrootFS`](impls/altroot/struct.AsyncAltrootFS.html)** - a file system with its root in a particular directory of another filesystem
//!  * **[`AsyncOverlayFS`](impls/overlay/struct.AsyncOverlayFS.html)** - a union file system consisting of a read/writable upper layer and several read-only lower layers
//!
//! # Usage Examples
//!
//! ```
//! use async_std::io::{ReadExt, WriteExt};
//! use vfs::async_vfs::{AsyncVfsPath, AsyncPhysicalFS};
//! use vfs::VfsError;
//!
//! # tokio_test::block_on(async {
//! let root: AsyncVfsPath = AsyncPhysicalFS::new(std::env::current_dir().unwrap()).into();
//! assert!(root.exists().await?);
//!
//! let mut content = String::new();
//! root.join("README.md")?.open_file().await?.read_to_string(&mut content).await?;
//! assert!(content.contains("vfs"));
//! # Ok::<(), VfsError>(())
//! # });
//! ```
//!
//! ```
//! use async_std::io::{ReadExt, WriteExt};
//! use vfs::async_vfs::{AsyncVfsPath, AsyncMemoryFS};
//! use vfs::VfsError;
//!
//! # tokio_test::block_on(async {
//! let root: AsyncVfsPath = AsyncMemoryFS::new().into();
//! let path = root.join("test.txt")?;
//! assert!(!path.exists().await?);
//!
//! path.create_file().await?.write_all(b"Hello world").await?;
//! assert!(path.exists().await?);
//! let mut content = String::new();
//! path.open_file().await?.read_to_string(&mut content).await?;
//! assert_eq!(content, "Hello world");
//! # Ok::<(), VfsError>(())
//! # });
//! ```
//!

#[cfg(any(test, feature = "export-test-macros"))]
#[macro_use]
pub mod test_macros;

pub mod filesystem;
pub mod impls;
pub mod path;

pub use filesystem::AsyncFileSystem;
pub use impls::altroot::AsyncAltrootFS;
pub use impls::memory::AsyncMemoryFS;
pub use impls::overlay::AsyncOverlayFS;
pub use impls::physical::AsyncPhysicalFS;
pub use path::*;
