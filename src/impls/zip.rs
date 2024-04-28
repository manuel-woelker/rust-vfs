use crate::error::VfsErrorKind;
use crate::{FileSystem, SeekAndRead, SeekAndWrite, VfsError, VfsFileType, VfsMetadata, VfsResult};
use ouroboros::self_referencing;
use std::collections::HashSet;
use std::fmt::{Debug, Formatter};
use std::io::{Read, Seek, SeekFrom};
use zip::read::ZipFile;
use zip::ZipArchive;

/// a read-only file system view of a [ZIP archive file](https://en.wikipedia.org/wiki/ZIP_(file_format))
pub struct ZipFS {
    create_fn: Box<dyn Fn() -> Box<dyn SeekAndReadAndSend> + Send + Sync>,
}

impl ZipFS {
    pub fn new<T: SeekAndReadAndSend + 'static, F: (Fn() -> T) + Send + Sync + 'static>(
        f: F,
    ) -> Self {
        ZipFS {
            create_fn: Box::new(move || Box::new(f())),
        }
    }

    fn resolve_path(path: &str) -> String {
        let mut path = path.to_string();
        if path.starts_with("/") {
            path.remove(0);
        }
        path
    }

    fn open_archive(&self) -> VfsResult<ZipArchive<Box<dyn SeekAndReadAndSend>>> {
        let reader = (self.create_fn)();
        let archive = ZipArchive::new(reader)?;
        Ok(archive)
    }
}

impl Debug for ZipFS {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ZipFS")?;
        Ok(())
    }
}

pub trait SeekAndReadAndSend: Seek + Read + Send {}

impl<T> SeekAndReadAndSend for T where T: Seek + Read + Send {}

#[self_referencing]
struct SeekableZipFile {
    archive: ZipArchive<Box<dyn SeekAndReadAndSend>>,

    #[borrows(mut archive)]
    #[not_covariant]
    zip_file: ZipFile<'this>,
}

impl Read for SeekableZipFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.with_zip_file_mut(|zip_file| zip_file.read(buf))
    }
}

impl Seek for SeekableZipFile {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::other(VfsError::from(
            VfsErrorKind::NotSupported,
        )))
    }
}

// Should be safe since the input to the archive is Send
unsafe impl Send for SeekableZipFile {}

impl FileSystem for ZipFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String> + Send>> {
        let mut resolved_path = Self::resolve_path(path);
        if resolved_path != "" {
            resolved_path += "/";
        }
        let mut archive = self.open_archive()?;
        let mut entries = HashSet::<String>::new();
        let size = archive.len();
        for i in 0..size {
            let file = archive.by_index(i)?;
            if let Some(rest) = file.name().strip_prefix(&resolved_path) {
                if rest == "" {
                    continue;
                }
                if let Some((entry, _)) = rest.split_once("/") {
                    if entry == "" {
                        continue;
                    }
                    entries.insert(entry.to_string());
                } else {
                    entries.insert(rest.to_string());
                }
            }
        }
        if entries.is_empty() {
            // Maybe directory does not exist
            if !self.exists(&path)? {
                return Err(VfsError::from(VfsErrorKind::FileNotFound));
            }
        }
        Ok(Box::new(entries.into_iter()))
    }

    fn create_dir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send>> {
        let mut archive = self.open_archive()?;
        let path = Self::resolve_path(path);
        archive.by_name(&path)?;
        let file = SeekableZipFileBuilder {
            archive: archive,
            zip_file_builder: |archive: &mut ZipArchive<Box<dyn SeekAndReadAndSend>>| {
                archive.by_name(&path).unwrap()
            },
        }
        .build();
        Ok(Box::new(file))
    }

    fn create_file(&self, _path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn append_file(&self, _path: &str) -> VfsResult<Box<dyn SeekAndWrite + Send>> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        if path == "" {
            return Ok(VfsMetadata {
                file_type: VfsFileType::Directory,
                len: 0,
                modified: None,
                created: None,
                accessed: None,
            });
        }
        let mut archive = self.open_archive()?;
        let path = Self::resolve_path(path);
        let zipfile = {
            let mut result = archive.by_name(&path);
            if result.is_err() {
                drop(result);
                result = archive.by_name(&(path + "/"));
            }
            result
        }?;
        Ok(VfsMetadata {
            file_type: if zipfile.is_dir() {
                VfsFileType::Directory
            } else {
                VfsFileType::File
            },
            len: zipfile.size(),
            modified: None,
            created: None,
            accessed: None,
        })
    }

    fn exists(&self, path: &str) -> VfsResult<bool> {
        if path == "" {
            return Ok(true);
        }
        let mut archive = self.open_archive()?;
        let path = Self::resolve_path(path);
        let zipfile = archive.by_name(&path);
        if zipfile.is_err() {
            drop(zipfile);
            let zipfile = archive.by_name(&(path + "/"));
            return Ok(zipfile.is_ok());
        }
        Ok(true)
    }

    fn remove_file(&self, _path: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }

    fn remove_dir(&self, _path: &str) -> VfsResult<()> {
        Err(VfsErrorKind::NotSupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::zip::write::SimpleFileOptions;
    use ::zip::ZipWriter;
    use std::fs;
    use std::fs::File;
    use std::io::Cursor;
    use std::sync::Arc;
    use walkdir::WalkDir;

    #[derive(Clone)]
    pub struct LargeData {
        data: Arc<Vec<u8>>,
    }

    impl AsRef<[u8]> for LargeData {
        fn as_ref(&self) -> &[u8] {
            &self.data
        }
    }

    impl LargeData {
        fn open_for_read(&self) -> Cursor<LargeData> {
            Cursor::new(self.clone())
        }
    }

    fn get_test_fs() -> ZipFS {
        let mut buf = Vec::new();
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let base_path = "test/test_directory";
        for entry in WalkDir::new(base_path) {
            let entry = entry.unwrap();
            let path = entry
                .path()
                .strip_prefix(base_path)
                .unwrap()
                .to_str()
                .unwrap()
                .to_string()
                .replace("\\", "/");
            if fs::metadata(entry.path()).unwrap().is_dir() {
                let options =
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Zstd);
                zip.add_directory(path, options).unwrap();
                continue;
            }
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Zstd);
            zip.start_file(path, options).unwrap();
            let mut file = File::open(entry.path()).unwrap();
            std::io::copy(&mut file, &mut zip).unwrap();
        }
        zip.finish().unwrap();
        drop(zip);
        let data = LargeData {
            data: Arc::new(buf),
        };
        ZipFS::new(move || data.open_for_read())
    }

    test_vfs_readonly!({ get_test_fs() });
}
