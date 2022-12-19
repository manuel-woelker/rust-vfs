//! An overlay file system combining two filesystems, an upper layer with read/write access and a lower layer with only read access

use crate::error::VfsErrorKind;
use crate::{FileSystem, SeekAndRead, VfsMetadata, VfsPath, VfsResult};
use std::collections::HashSet;
use std::io::Write;

/// An overlay file system combining several filesystems into one, an upper layer with read/write access and lower layers with only read access
///
/// Files in upper layers shadow those in lower layers. Directories are the merged view of all layers.
///
/// NOTE: To allow removing files and directories (e.g. via remove_file()) from the lower layer filesystems, this mechanism creates a `.whiteout` folder in the root of the upper level filesystem to mark removed files
///
#[derive(Debug, Clone)]
pub struct OverlayFS {
    layers: Vec<VfsPath>,
}

impl OverlayFS {
    /// Create a new overlay FileSystem from the given layers, only the first layer is written to
    pub fn new(layers: &[VfsPath]) -> Self {
        if layers.is_empty() {
            panic!("OverlayFS needs at least one layer")
        }
        OverlayFS {
            layers: layers.to_vec(),
        }
    }

    fn write_layer(&self) -> &VfsPath {
        &self.layers[0]
    }

    fn read_path(&self, path: &str) -> VfsResult<VfsPath> {
        if path.is_empty() {
            return Ok(self.layers[0].clone());
        }
        if self.whiteout_path(path)?.exists()? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        for layer in &self.layers {
            let layer_path = layer.join(&path[1..])?;
            if layer_path.exists()? {
                return Ok(layer_path);
            }
        }
        let read_path = self.write_layer().join(&path[1..])?;
        if !read_path.exists()? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        Ok(read_path)
    }

    fn write_path(&self, path: &str) -> VfsResult<VfsPath> {
        if path.is_empty() {
            return Ok(self.layers[0].clone());
        }
        self.write_layer().join(&path[1..])
    }

    fn whiteout_path(&self, path: &str) -> VfsResult<VfsPath> {
        if path.is_empty() {
            return self.write_layer().join(".whiteout/_wo");
        }
        self.write_layer()
            .join(format!(".whiteout/{}_wo", &path[1..]))
    }

    fn ensure_has_parent(&self, path: &str) -> VfsResult<()> {
        let separator = path.rfind('/');
        if let Some(index) = separator {
            let parent_path = &path[..index];
            if self.exists(parent_path)? {
                self.write_path(parent_path)?.create_dir_all()?;
                return Ok(());
            }
        }
        Err(VfsErrorKind::Other("Parent path does not exist".into()).into())
    }
}

impl FileSystem for OverlayFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let actual_path = if !path.is_empty() { &path[1..] } else { path };
        if !self.read_path(path)?.exists()? {
            return Err(VfsErrorKind::FileNotFound.into());
        }
        let mut entries = HashSet::<String>::new();
        for layer in &self.layers {
            let layer_path = layer.join(actual_path)?;
            if layer_path.exists()? {
                for path in layer_path.read_dir()? {
                    entries.insert(path.filename());
                }
            }
        }
        // remove whiteout entries that have been removed
        let whiteout_path = self.write_layer().join(format!(".whiteout{}", path))?;
        if whiteout_path.exists()? {
            for path in whiteout_path.read_dir()? {
                let filename = path.filename();
                if filename.ends_with("_wo") {
                    entries.remove(&filename[..filename.len() - 3]);
                }
            }
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.ensure_has_parent(path)?;
        self.write_path(path)?.create_dir()?;
        let whiteout_path = self.whiteout_path(path)?;
        if whiteout_path.exists()? {
            whiteout_path.remove_file()?;
        }
        Ok(())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        self.read_path(path)?.open_file()
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        self.ensure_has_parent(path)?;
        let result = self.write_path(path)?.create_file()?;
        let whiteout_path = self.whiteout_path(path)?;
        if whiteout_path.exists()? {
            whiteout_path.remove_file()?;
        }
        Ok(result)
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send>> {
        let write_path = self.write_path(path)?;
        if !write_path.exists()? {
            self.ensure_has_parent(path)?;
            self.read_path(path)?.copy_file(&write_path)?;
        }
        write_path.append_file()
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        self.read_path(path)?.metadata()
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        if self
            .whiteout_path(path)
            .map_err(|err| err.with_context(|| "whiteout_path"))?
            .exists()?
        {
            return Ok(false);
        }
        self.read_path(path)
            .map(|path| path.exists())
            .unwrap_or(Ok(false))
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        // Ensure path exists
        self.read_path(path)?;
        let write_path = self.write_path(path)?;
        if write_path.exists()? {
            write_path.remove_file()?;
        }
        let whiteout_path = self.whiteout_path(path)?;
        whiteout_path.parent().create_dir_all()?;
        whiteout_path.create_file()?;
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        // Ensure path exists
        self.read_path(path)?;
        let write_path = self.write_path(path)?;
        if write_path.exists()? {
            write_path.remove_dir()?;
        }
        let whiteout_path = self.whiteout_path(path)?;
        whiteout_path.parent().create_dir_all()?;
        whiteout_path.create_file()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryFS;
    test_vfs!({
        let upper_root: VfsPath = MemoryFS::new().into();
        let lower_root: VfsPath = MemoryFS::new().into();
        OverlayFS::new(&[upper_root, lower_root])
    });

    fn create_roots() -> (VfsPath, VfsPath, VfsPath) {
        let lower_root: VfsPath = MemoryFS::new().into();
        let upper_root: VfsPath = MemoryFS::new().into();
        let overlay_root: VfsPath =
            OverlayFS::new(&[upper_root.clone(), lower_root.clone()]).into();
        (lower_root, upper_root, overlay_root)
    }

    #[test]
    fn read() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        let lower_path = lower_root.join("foo.txt")?;
        let upper_path = upper_root.join("foo.txt")?;
        let overlay_path = overlay_root.join("foo.txt")?;
        lower_path.create_file()?.write_all(b"Hello Lower")?;
        assert_eq!(&overlay_path.read_to_string()?, "Hello Lower");
        upper_path.create_file()?.write_all(b"Hello Upper")?;
        assert_eq!(&overlay_path.read_to_string()?, "Hello Upper");
        lower_path.remove_file()?;
        assert_eq!(&overlay_path.read_to_string()?, "Hello Upper");
        upper_path.remove_file()?;
        assert!(!overlay_path.exists()?, "File should not exist anymore");
        Ok(())
    }

    #[test]
    fn read_dir() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        upper_root.join("foo/upper")?.create_dir_all()?;
        upper_root.join("foo/common")?.create_dir_all()?;
        lower_root.join("foo/common")?.create_dir_all()?;
        lower_root.join("foo/lower")?.create_dir_all()?;
        let entries: Vec<_> = overlay_root.join("foo")?.read_dir()?.collect();
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/foo/common", "/foo/lower", "/foo/upper"]);
        Ok(())
    }

    #[test]
    fn read_dir_root() -> VfsResult<()> {
        let (lower_root, upper_root, overlay_root) = create_roots();
        upper_root.join("upper")?.create_dir_all()?;
        upper_root.join("common")?.create_dir_all()?;
        lower_root.join("common")?.create_dir_all()?;
        lower_root.join("lower")?.create_dir_all()?;
        let entries: Vec<_> = overlay_root.read_dir()?.collect();
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/common", "/lower", "/upper"]);
        Ok(())
    }

    #[test]
    fn create_dir() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        assert!(overlay_root.join("foo")?.exists()?, "dir should exist");
        overlay_root.join("foo/bar")?.create_dir()?;
        assert!(overlay_root.join("foo/bar")?.exists()?, "dir should exist");
        Ok(())
    }

    #[test]
    fn create_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        assert!(overlay_root.join("foo")?.exists()?, "dir should exist");
        overlay_root.join("foo/bar")?.create_file()?;
        assert!(overlay_root.join("foo/bar")?.exists()?, "file should exist");
        Ok(())
    }

    #[test]
    fn append_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        lower_root
            .join("foo/bar.txt")?
            .create_file()?
            .write_all(b"Hello Lower\n")?;
        overlay_root
            .join("foo/bar.txt")?
            .append_file()?
            .write_all(b"Hello Overlay\n")?;
        assert_eq!(
            &overlay_root.join("foo/bar.txt")?.read_to_string()?,
            "Hello Lower\nHello Overlay\n"
        );
        Ok(())
    }

    #[test]
    fn remove_file() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        lower_root
            .join("foo/bar.txt")?
            .create_file()?
            .write_all(b"Hello Lower\n")?;
        assert!(
            overlay_root.join("foo/bar.txt")?.exists()?,
            "file should exist"
        );

        overlay_root.join("foo/bar.txt")?.remove_file()?;
        assert!(
            !overlay_root.join("foo/bar.txt")?.exists()?,
            "file should not exist anymore"
        );

        overlay_root
            .join("foo/bar.txt")?
            .create_file()?
            .write_all(b"Hello Overlay\n")?;
        assert!(
            overlay_root.join("foo/bar.txt")?.exists()?,
            "file should exist"
        );
        assert_eq!(
            &overlay_root.join("foo/bar.txt")?.read_to_string()?,
            "Hello Overlay\n"
        );
        Ok(())
    }

    #[test]
    fn remove_dir() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        lower_root.join("foo/bar")?.create_dir_all()?;
        assert!(overlay_root.join("foo/bar")?.exists()?, "dir should exist");

        overlay_root.join("foo/bar")?.remove_dir()?;
        assert!(
            !overlay_root.join("foo/bar")?.exists()?,
            "dir should not exist anymore"
        );

        overlay_root.join("foo/bar")?.create_dir()?;
        assert!(overlay_root.join("foo/bar")?.exists()?, "dir should exist");
        Ok(())
    }

    #[test]
    fn read_dir_removed_entries() -> VfsResult<()> {
        let (lower_root, _upper_root, overlay_root) = create_roots();
        lower_root.join("foo")?.create_dir_all()?;
        lower_root.join("foo/bar")?.create_dir_all()?;
        lower_root.join("foo/bar.txt")?.create_dir_all()?;

        let entries: Vec<_> = overlay_root.join("foo")?.read_dir()?.collect();
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["/foo/bar", "/foo/bar.txt"]);
        overlay_root.join("foo/bar")?.remove_dir()?;
        overlay_root.join("foo/bar.txt")?.remove_file()?;

        let entries: Vec<_> = overlay_root.join("foo")?.read_dir()?.collect();
        let mut paths: Vec<_> = entries.iter().map(|path| path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec![] as Vec<&str>);

        Ok(())
    }
}

#[cfg(test)]
mod tests_physical {
    use super::*;
    use crate::PhysicalFS;
    test_vfs!({
        let temp_dir = std::env::temp_dir();
        let dir = temp_dir.join(uuid::Uuid::new_v4().to_string());
        let lower_path = dir.join("lower");
        std::fs::create_dir_all(&lower_path).unwrap();
        let upper_path = dir.join("upper");
        std::fs::create_dir_all(&upper_path).unwrap();

        let upper_root: VfsPath = PhysicalFS::new(upper_path).into();
        let lower_root: VfsPath = PhysicalFS::new(lower_path).into();
        OverlayFS::new(&[upper_root, lower_root])
    });
}
