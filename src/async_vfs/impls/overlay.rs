//! An overlay file system combining two filesystems, an upper layer with read/write access and a lower layer with only read access

use crate::async_vfs::{AsyncFileSystem, AsyncVfsPath, SeekAndRead};
use crate::error::VfsErrorKind;
use crate::{VfsMetadata, VfsResult};

use async_std::io::Write;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use std::collections::HashSet;

/// An overlay file system combining several filesystems into one, an upper layer with read/write access and lower layers with only read access
///
/// Files in upper layers shadow those in lower layers. Directories are the merged view of all layers.
///
/// NOTE: To allow removing files and directories (e.g. via remove_file()) from the lower layer filesystems, this mechanism creates a `.whiteout` folder in the root of the upper level filesystem to mark removed files
///
#[derive(Debug, Clone)]
pub struct AsyncOverlayFS {
    layers: Vec<AsyncVfsPath>,
}

impl AsyncOverlayFS {
    /// Create a new overlay FileSystem from the given layers, only the first layer is written to
    pub fn new(layers: &[AsyncVfsPath]) -> Self {
        if layers.is_empty() {
            panic!("AsyncOverlayFS needs at least one layer")
        }
        AsyncOverlayFS {
            layers: layers.to_vec(),
        }
    }

    fn write_layer(&self) -> &AsyncVfsPath {
        &self.layers[0]
    }

    async fn read_path(&self, path: &str) -> VfsResult<AsyncVfsPath> {
        if path.is_empty() {
            return Ok(self.layers[0].clone());
        }
        if self.whiteout_path(path)?.exists().await? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        for layer in &self.layers {
            let layer_path = layer.join(&path[1..])?;
            if layer_path.exists().await? {
                return Ok(layer_path);
            }
        }
        let read_path = self.write_layer().join(&path[1..])?;
        if !read_path.exists().await? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        Ok(read_path)
    }

    fn write_path(&self, path: &str) -> VfsResult<AsyncVfsPath> {
        if path.is_empty() {
            return Ok(self.layers[0].clone());
        }
        self.write_layer().join(&path[1..])
    }

    fn whiteout_path(&self, path: &str) -> VfsResult<AsyncVfsPath> {
        if path.is_empty() {
            return self.write_layer().join(".whiteout/_wo");
        }
        self.write_layer()
            .join(format!(".whiteout/{}_wo", &path[1..]))
    }

    async fn ensure_has_parent(&self, path: &str) -> VfsResult<()> {
        let separator = path.rfind('/');
        if let Some(index) = separator {
            let parent_path = &path[..index];
            if self.exists(parent_path).await? {
                self.write_path(parent_path)?.create_dir_all().await?;
                return Ok(());
            }
        }
        Err(VfsErrorKind::Other("Parent path does not exist".into()).into())
    }
}

#[async_trait]
impl AsyncFileSystem for AsyncOverlayFS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Stream<Item = String> + Send + Unpin>> {
        let actual_path = if !path.is_empty() { &path[1..] } else { path };
        if !self.read_path(path).await?.exists().await? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        let mut entries = HashSet::<String>::new();
        for layer in &self.layers {
            let layer_path = layer.join(actual_path)?;
            if layer_path.exists().await? {
                let mut path_stream = layer_path.read_dir().await?;
                while let Some(path) = path_stream.next().await {
                    entries.insert(path.filename());
                }
            }
        }
        // remove whiteout entries that have been removed
        let whiteout_path = self.write_layer().join(format!(".whiteout{}", path))?;
        if whiteout_path.exists().await? {
            let mut path_stream = whiteout_path.read_dir().await?;
            while let Some(path) = path_stream.next().await {
                let filename = path.filename();
                if filename.ends_with("_wo") {
                    entries.remove(&filename[..filename.len() - 3]);
                }
            }
        }
        Ok(Box::new(futures::stream::iter(entries)))
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.ensure_has_parent(path).await?;
        self.write_path(path)?.create_dir().await?;
        let whiteout_path = self.whiteout_path(path)?;
        if whiteout_path.exists().await? {
            whiteout_path.remove_file().await?;
        }
        Ok(())
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        self.read_path(path).await?.open_file().await
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        self.ensure_has_parent(path).await?;
        let result = self.write_path(path)?.create_file().await?;
        let whiteout_path = self.whiteout_path(path)?;
        if whiteout_path.exists().await? {
            whiteout_path.remove_file().await?;
        }
        Ok(result)
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        let write_path = self.write_path(path)?;
        if !write_path.exists().await? {
            self.ensure_has_parent(path).await?;
            self.read_path(path).await?.copy_file(&write_path).await?;
        }
        write_path.append_file().await
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.read_path(path).await?.metadata().await
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        if self
            .whiteout_path(path)
            .map_err(|err| err.with_context(|| "whiteout_path"))?
            .exists()
            .await?
        {
            return Ok(false);
        }
        match self.read_path(path).await {
            Ok(p) => p.exists().await,
            Err(_) => Ok(false),
        }
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        // Ensure path exists
        self.read_path(path).await?;
        let write_path = self.write_path(path)?;
        if write_path.exists().await? {
            write_path.remove_file().await?;
        }
        let whiteout_path = self.whiteout_path(path)?;
        whiteout_path.parent().create_dir_all().await?;
        whiteout_path.create_file().await?;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        // Ensure path exists
        self.read_path(path).await?;
        let write_path = self.write_path(path)?;
        if write_path.exists().await? {
            write_path.remove_dir().await?;
        }
        let whiteout_path = self.whiteout_path(path)?;
        whiteout_path.parent().create_dir_all().await?;
        whiteout_path.create_file().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_vfs::AsyncMemoryFS;

    use async_std::io::WriteExt;
    use futures::stream::StreamExt;

    test_async_vfs!({
        let upper_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        let lower_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        AsyncOverlayFS::new(&[upper_root, lower_root])
    });

    fn create_roots() -> (AsyncVfsPath, AsyncVfsPath, AsyncVfsPath) {
        let lower_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        let upper_root: AsyncVfsPath = AsyncMemoryFS::new().into();
        let overlay_root: AsyncVfsPath =
            AsyncOverlayFS::new(&[upper_root.clone(), lower_root.clone()]).into();
        (lower_root, upper_root, overlay_root)
    }

    #[tokio::test]
    async fn read() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        let lower_path = lower_root.join("foo.txt")?;
        let upper_path = upper_root.join("foo.txt")?;
        let overlay_path = overlay_root.join("foo.txt")?;
        lower_path
            .create_file()
            .await?
            .write_all(b"Hello Lower")
            .await?;
        assert_eq!(&overlay_path.read_to_string().await?, "Hello Lower");
        upper_path
            .create_file()
            .await?
            .write_all(b"Hello Upper")
            .await?;
        assert_eq!(&overlay_path.read_to_string().await?, "Hello Upper");
        lower_path.remove_file().await?;
        assert_eq!(&overlay_path.read_to_string().await?, "Hello Upper");
        upper_path.remove_file().await?;
        assert!(
            !overlay_path.exists().await?,
            "File should not exist anymore"
        );
        Ok(())
    }

    #[tokio::test]
    async fn read_dir() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        upper_root.join("foo/upper")?.create_dir_all().await?;
        upper_root.join("foo/common")?.create_dir_all().await?;
        lower_root.join("foo/common")?.create_dir_all().await?;
        lower_root.join("foo/lower")?.create_dir_all().await?;
        let entries: Vec<_> = overlay_root.join("foo")?.read_dir().await?.collect().await;
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/foo/common", "/foo/lower", "/foo/upper"]);
        Ok(())
    }

    #[tokio::test]
    async fn read_dir_root() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        upper_root.join("upper")?.create_dir_all().await?;
        upper_root.join("common")?.create_dir_all().await?;
        lower_root.join("common")?.create_dir_all().await?;
        lower_root.join("lower")?.create_dir_all().await?;
        let entries: Vec<_> = overlay_root.read_dir().await?.collect().await;
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/common", "/lower", "/upper"]);
        Ok(())
    }

    #[tokio::test]
    async fn create_dir() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        assert!(
            overlay_root.join("foo")?.exists().await?,
            "dir should exist"
        );
        overlay_root.join("foo/bar")?.create_dir().await?;
        assert!(
            overlay_root.join("foo/bar")?.exists().await?,
            "dir should exist"
        );
        Ok(())
    }

    #[tokio::test]
    async fn create_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        assert!(
            overlay_root.join("foo")?.exists().await?,
            "dir should exist"
        );
        overlay_root.join("foo/bar")?.create_file().await?;
        assert!(
            overlay_root.join("foo/bar")?.exists().await?,
            "file should exist"
        );
        Ok(())
    }

    #[tokio::test]
    async fn append_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        lower_root
            .join("foo/bar.txt")?
            .create_file()
            .await?
            .write_all(b"Hello Lower\n")
            .await?;
        overlay_root
            .join("foo/bar.txt")?
            .append_file()
            .await?
            .write_all(b"Hello Overlay\n")
            .await?;
        assert_eq!(
            &overlay_root.join("foo/bar.txt")?.read_to_string().await?,
            "Hello Lower\nHello Overlay\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn remove_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        lower_root
            .join("foo/bar.txt")?
            .create_file()
            .await?
            .write_all(b"Hello Lower\n")
            .await?;
        assert!(
            overlay_root.join("foo/bar.txt")?.exists().await?,
            "file should exist"
        );

        overlay_root.join("foo/bar.txt")?.remove_file().await?;
        assert!(
            !overlay_root.join("foo/bar.txt")?.exists().await?,
            "file should not exist anymore"
        );

        overlay_root
            .join("foo/bar.txt")?
            .create_file()
            .await?
            .write_all(b"Hello Overlay\n")
            .await?;
        assert!(
            overlay_root.join("foo/bar.txt")?.exists().await?,
            "file should exist"
        );
        assert_eq!(
            &overlay_root.join("foo/bar.txt")?.read_to_string().await?,
            "Hello Overlay\n"
        );
        Ok(())
    }

    #[tokio::test]
    async fn remove_dir() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        lower_root.join("foo/bar")?.create_dir_all().await?;
        assert!(
            overlay_root.join("foo/bar")?.exists().await?,
            "dir should exist"
        );

        overlay_root.join("foo/bar")?.remove_dir().await?;
        assert!(
            !overlay_root.join("foo/bar")?.exists().await?,
            "dir should not exist anymore"
        );

        overlay_root.join("foo/bar")?.create_dir().await?;
        assert!(
            overlay_root.join("foo/bar")?.exists().await?,
            "dir should exist"
        );
        Ok(())
    }

    #[tokio::test]
    async fn read_dir_removed_entries() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all().await?;
        lower_root.join("foo/bar")?.create_dir_all().await?;
        lower_root.join("foo/bar.txt")?.create_dir_all().await?;

        let entries: Vec<_> = overlay_root.join("foo")?.read_dir().await?.collect().await;
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/foo/bar", "/foo/bar.txt"]);
        overlay_root.join("foo/bar")?.remove_dir().await?;
        overlay_root.join("foo/bar.txt")?.remove_file().await?;

        let entries: Vec<_> = overlay_root.join("foo")?.read_dir().await?.collect().await;
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec![] as Vec<&str>);

        Ok(())
    }
}

#[cfg(test)]
mod tests_physical {
    use super::*;
    use crate::async_vfs::AsyncPhysicalFS;

    test_async_vfs!(futures::executor::block_on(async {
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        let lower_path = dir.join("lower");
        async_std::fs::create_dir_all(&lower_path).await.unwrap();
        let upper_path = dir.join("upper");
        async_std::fs::create_dir_all(&upper_path).await.unwrap();

        let upper_root: AsyncVfsPath = AsyncPhysicalFS::new(upper_path).into();
        let lower_root: AsyncVfsPath = AsyncPhysicalFS::new(lower_path).into();
        AsyncOverlayFS::new(&[upper_root, lower_root])
    }));
}
