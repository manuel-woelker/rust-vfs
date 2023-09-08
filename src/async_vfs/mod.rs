/// Asynchronous port of virtual file system abstraction
///
///
/// Just as with the synchronous version, the main interaction with the virtual filesystem is by using virtual paths ([`VfsPath`](path/struct.VfsPath.html)).
///
/// This *async_vfs* module of this crate currently has the following implementations:
///
///  * **[`PhysicalFS`](impls/physical/struct.PhysicalFS.html)** - the actual filesystem of the underlying OS
///  * **[`MemoryFS`](impls/memory/struct.MemoryFS.html)** - an ephemeral in-memory implementation (intended for unit tests)
///  * **[`AltrootFS`](impls/altroot/struct.AltrootFS.html)** - a file system with its root in a particular directory of another filesystem
///  * **[`OverlayFS`](impls/overlay/struct.OverlayFS.html)** - a union file system consisting of a read/writable upper layer and several read-only lower layers
///
/// # Usage Examples
///
/// ```
/// use async_std::io::{ReadExt, WriteExt};
/// use vfs::async_vfs::{VfsPath, PhysicalFS, VfsError};
///
/// # tokio_test::block_on(async {
/// let root: VfsPath = PhysicalFS::new(std::env::current_dir().unwrap()).into();
/// assert!(root.exists().await?);
///
/// let mut content = String::new();
/// root.join("README.md")?.open_file().await?.read_to_string(&mut content).await?;
/// assert!(content.contains("vfs"));
/// # Ok::<(), VfsError>(())
/// # });
/// ```
///
/// ```
/// use async_std::io::{ReadExt, WriteExt};
/// use vfs::async_vfs::{VfsPath, VfsError, MemoryFS};
///
/// # tokio_test::block_on(async {
/// let root: VfsPath = MemoryFS::new().into();
/// let path = root.join("test.txt")?;
/// assert!(!path.exists().await?);
///
/// path.create_file().await?.write_all(b"Hello world").await?;
/// assert!(path.exists().await?);
/// let mut content = String::new();
/// path.open_file().await?.read_to_string(&mut content).await?;
/// assert_eq!(content, "Hello world");
/// # Ok::<(), VfsError>(())
/// # });
/// ```
///

#[cfg(any(test, feature = "export-test-macros"))]
#[macro_use]
pub mod test_macros;

pub mod error;
pub mod filesystem;
pub mod impls;
pub mod path;

pub use error::{VfsError, VfsResult};
pub use filesystem::FileSystem;
pub use impls::altroot::AltrootFS;
pub use impls::memory::MemoryFS;
pub use impls::overlay::OverlayFS;
pub use impls::physical::PhysicalFS;
pub use path::*;
