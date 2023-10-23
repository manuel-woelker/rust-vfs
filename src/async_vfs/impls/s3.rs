use crate::{
    async_vfs::{AsyncFileSystem, SeekAndRead},
    error::VfsErrorKind,
    VfsError, VfsFileType, VfsMetadata, VfsResult,
};
use async_std::{io::prelude::*, prelude::Stream};
use async_trait::async_trait;
use aws_config::SdkConfig;
use aws_sdk_s3::{
    error::SdkError, operation::get_object::GetObjectOutput, primitives::ByteStream, Client,
};
use futures::{
    executor::block_on, AsyncRead, AsyncSeek, AsyncWrite, FutureExt, StreamExt, TryStreamExt,
};
use std::{
    fmt::Display,
    io::SeekFrom,
    ops::Deref,
    pin::{pin, Pin},
    sync::Arc,
    task::{Context, Poll},
};

#[derive(Debug)]
pub struct S3FSImpl {
    client: Client,
    bucket_name: String,
}

#[derive(Clone, Debug)]
pub struct S3FS(Arc<S3FSImpl>);

impl S3FS {
    // TODO: Change constructor signature so that caller does not need to import AWS SDK
    pub async fn new(config: &SdkConfig, bucket_name: &str) -> VfsResult<Self> {
        let client = Client::new(config);
        client.create_bucket().bucket(bucket_name).send().await?;
        Ok(Self(Arc::new(S3FSImpl {
            client,
            bucket_name: bucket_name.to_owned(),
        })))
    }
}

impl Deref for S3FS {
    type Target = S3FSImpl;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

struct S3FileReader {
    object: GetObjectOutput,
    buffer: Vec<u8>,
    position: u64,
}

impl S3FileReader {
    fn new(object: GetObjectOutput) -> Self {
        Self {
            object,
            buffer: Vec::new(),
            position: 0,
        }
    }

    fn content_length(&self) -> u64 {
        self.object.content_length as _
    }

    async fn fill_buffer(&mut self, upper_bound: u64) -> std::io::Result<()> {
        let desired_buffer_size = upper_bound.min(self.content_length());

        // TODO: Implement a way to avoid infinite loops
        while (self.buffer.len() as u64) < desired_buffer_size {
            if let Some(bytes) = self.object.body.try_next().await? {
                self.buffer.extend(bytes);
            }
        }

        Ok(())
    }

    async fn async_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes_left = self.content_length() - self.position;
        if bytes_left == 0 {
            return Ok(0);
        }

        let bytes_read = bytes_left.min(buf.len() as u64);
        let end_position = self.position + bytes_read;
        let buffered_remaining = (self.buffer.len() as u64) - self.position;
        if bytes_read > buffered_remaining {
            self.fill_buffer(end_position).await?;
        }

        buf[..bytes_read as usize].copy_from_slice(
            &self.buffer[self.position as usize..(self.position + bytes_read) as usize],
        );
        self.position += bytes_read;

        Ok(0)
    }
}

impl AsyncSeek for S3FileReader {
    fn poll_seek(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _pos: SeekFrom,
    ) -> Poll<std::io::Result<u64>> {
        todo!()
    }
}

impl AsyncRead for S3FileReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let mut fut = pin!(this.async_read(buf));
        match fut.poll_unpin(cx) {
            Poll::Ready(res) => Poll::Ready(res),
            Poll::Pending => Poll::Pending,
        }
    }
}

struct S3FileWriter {
    fs: S3FS,
    key: String,
    buffer: Vec<u8>,
}

impl S3FileWriter {
    fn new(fs: &S3FS, key: &str) -> Self {
        Self {
            fs: fs.clone(),
            key: key.to_owned(),
            buffer: Vec::new(),
        }
    }

    async fn async_flush(&self) -> std::io::Result<()> {
        let body = ByteStream::from(self.buffer.clone());
        self.fs
            .client
            .put_object()
            .bucket(&self.fs.bucket_name)
            .key(&self.key)
            .body(body)
            .send()
            .await
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err))?;

        Ok(())
    }
}

impl AsyncWrite for S3FileWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let mut fut = pin!(this.write(buf));

        match fut.poll_unpin(cx) {
            Poll::Ready(res) => Poll::Ready(res),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let mut fut = pin!(this.async_flush());

        match fut.poll_unpin(cx) {
            Poll::Ready(res) => Poll::Ready(res),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for S3FileWriter {
    fn drop(&mut self) {
        let _ = block_on(self.async_flush());
    }
}

fn make_s3_error(cause: impl Display) -> VfsError {
    VfsErrorKind::Other(format!("S3 error: {cause}")).into()
}

impl<E> From<SdkError<E>> for VfsError {
    fn from(value: SdkError<E>) -> Self {
        make_s3_error(value.to_string())
    }
}

#[async_trait]
impl AsyncFileSystem for S3FS {
    async fn read_dir(
        &self,
        path: &str,
    ) -> VfsResult<Box<dyn Unpin + Stream<Item = String> + Send>> {
        let s3_rez = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket_name)
            .prefix(path)
            .send()
            .await?;

        let entries = s3_rez
            .contents()
            .ok_or(make_s3_error("Cannot read list content"))?;
        let mut result = Vec::new();

        for entry in entries {
            result.push(
                entry
                    .key
                    .as_ref()
                    .ok_or(make_s3_error("Cannot read entry"))?
                    .to_owned(),
            );
        }

        Ok(Box::new(futures::stream::iter(result)))
    }

    async fn create_dir(&self, path: &str) -> VfsResult<()> {
        self.client
            .put_object()
            .bucket(&self.bucket_name)
            .key(path)
            .send()
            .await?;
        Ok(())
    }

    async fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead + Send + Unpin>> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(path)
            .send()
            .await?;

        Ok(Box::new(S3FileReader::new(object)))
    }

    async fn create_file(&self, path: &str) -> VfsResult<Box<dyn AsyncWrite + Send + Unpin>> {
        Ok(Box::new(S3FileWriter::new(self, path)))
    }

    async fn append_file(&self, _path: &str) -> VfsResult<Box<dyn AsyncWrite + Send + Unpin>> {
        todo!()
    }

    async fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let s3_rez = self
            .client
            .head_object()
            .bucket(&self.bucket_name)
            .key(path)
            .send()
            .await?;

        Ok(VfsMetadata {
            file_type: VfsFileType::File,
            len: s3_rez.content_length as u64,
        })
    }

    async fn exists(&self, path: &str) -> VfsResult<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket_name)
            .key(path)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn remove_file(&self, path: &str) -> VfsResult<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(path)
            .send()
            .await?;
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> VfsResult<()> {
        let mut path_stream = self.read_dir(path).await?;
        while let Some(file_path) = path_stream.next().await {
            self.remove_file(&file_path).await?;
        }

        Ok(())
    }

    async fn copy_file(&self, _src: &str, _dest: &str) -> VfsResult<()> {
        self.client
            .copy_object()
            .bucket(&self.bucket_name)
            .key(_dest)
            .copy_source(_src)
            .send()
            .await?;
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
    use super::*;
    use crate::async_vfs::AsyncVfsPath;

    async fn create_root() -> AsyncVfsPath {
        let sdk_config = aws_config::from_env()
            .profile_name("test_aws_config")
            .load()
            .await;
        AsyncVfsPath::new(S3FS::new(&sdk_config, "test_s3_vfs_bucket").await.unwrap())
    }

    #[tokio::test]
    async fn create_file() {
        let root = create_root().await;
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
        assert_eq!(read.as_bytes(), contents);
    }
}
