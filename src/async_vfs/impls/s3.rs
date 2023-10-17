use std::io::{IoSliceMut, SeekFrom, Write};
use std::pin::Pin;
use std::task::{Context, Poll};
use async_std::io::{ReadExt, Seek, prelude::*};
use async_std::prelude::Stream;
use async_trait::async_trait;
use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::{ByteStream};
use futures::{AsyncRead, AsyncSeek, AsyncWrite, StreamExt, TryStreamExt};
use crate::async_vfs::{AsyncFileSystem, SeekAndRead};
use crate::{VfsFileType, VfsMetadata, VfsResult};

#[derive(Debug)]
pub struct S3FS {
    s3_client: Client,
    bucket: String,
}

impl S3FS {
    pub async fn new(s3_client: Client, bucket: String) -> S3FS {
        let _ = s3_client.create_bucket()
            .bucket(&bucket)
            .send()
            .await;
        S3FS {
            s3_client,
            bucket
        }
    }

}

struct S3File {
    contents: ByteStream,
    bucket: String,
    key: String
}

impl Read for S3File {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        todo!()
    }

    fn poll_read_vectored(self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &mut [IoSliceMut<'_>]) -> Poll<std::io::Result<usize>> {
        todo!()
    }
}

impl AsyncSeek for S3File {

    fn poll_seek(self: Pin<&mut Self>, cx: &mut Context<'_>, pos: SeekFrom) -> Poll<std::io::Result<u64>> {
        todo!()
    }
}

impl AsyncWrite for S3File {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        todo!()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        todo!()
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        todo!()
    }
}

impl Write for S3File {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> std::io::Result<()> {
        todo!()
    }
}

#[async_trait]
impl AsyncFileSystem for S3FS {
    async fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>> {
        let s3_rez = self.s3_client.list_objects_v2()
            .bucket(&self.bucket)
            .prefix(path)
            .send()
            .await;
        let entries = Box::new(
            s3_rez
                .unwrap()
                .contents()
                .unwrap()
                .iter()
                .map(|x| x.key.unwrap().to_string()),
        );
        Ok(Box::new(futures::stream::iter(entries)))
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        let _rez = self.s3_client.put_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        Ok(())
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        let s3_rez = self.s3_client.get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        let body = s3_rez.unwrap().body;
        Ok(Box::new(S3File {
            contents: s3_rez.unwrap().body,
            bucket: self.bucket.clone(),
            key: path.to_string().clone(),
        }))
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        let _s3_rez = self.s3_client.put_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        let s3_rez = self.s3_client.get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        let body = s3_rez.unwrap().body;
        Ok(Box::new(S3File {
            contents: body,
            bucket: self.bucket.clone(),
            key: path.to_string().clone(),
        }))
    }

    async fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write + Send + Unpin>> {
        todo!()
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let s3_rez = self.s3_client.head_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;

        Ok(VfsMetadata {
            file_type: VfsFileType::File,
            len: s3_rez.unwrap().content_length as u64,
        })
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        match self.s3_client.head_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        let _ = self.s3_client.delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        let _ = self.read_dir(path).await?
            .map(|f| self.remove_file(f.as_str()));
        Ok(())
    }

    async fn copy_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        let _ = self.s3_client.copy_object()
            .bucket(&self.bucket)
            .key(_dest)
            .copy_source(_src)
            .send()
            .await;
        Ok(())
    }

    async fn move_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        self.copy_file(_src, _dest).await?;
        self.remove_file(_src).await?;
        Ok(())
    }

    async fn move_dir(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    /// It's important to note that you may be charged for running these tests.
    use async_std::prelude::FutureExt;
    use aws_sdk_s3::Client;
    use crate::async_vfs::AsyncVfsPath;
    use crate::async_vfs::impls::s3::S3FS;

    async fn create_root() -> AsyncVfsPath {
        let sdk_config = aws_config::from_env()
            .profile_name("test_aws_config")
            .load()
            .await;
        AsyncVfsPath::new(S3FS::new(
            Client::new(&sdk_config),
            "test_s3_vfs_bucket".to_string()))
    }

    #[tokio::test]
    async fn create_file() {
        let root = create_root();
        let contents = b"derp";
        root.join("test_file.txt")
           .unwrap()
           .create_file()
           .await
           .unwrap()
           .write_all(contents)
           .await
           .unwrap();
        let read = async_std::fs::read_to_string("test_file.txt")
           .await
           .unwrap();
        assert_eq!(read, contents);
    }
}