//!
//! The idea is, that a program can use normal file access methods, but works on
//! files on another machine. In combination with the other VFSs, this allows to
//! write programs, which can be used to modify local files, but also to modify
//! files on a remote system only by changing the underlying VFS.
//!
//! Architecture:
//! The user API is composed of two data structures: HttpsFS and HttpsFSServer
//!
//! From a user perspective HttpsFS has almost the same behavior as the other
//! VFSs, such as physicalFS, altrootFS and so on. The only difference is, that
//! the constructor takes an URI as argument, which points to an HttpsFSServer.
//! Furthermore, the constructor shall also take some arguments to specify 
//! methods to authenticate the client (HttpsFS) against the HttpsFSServer.
//!
//! The constructor of the HttpsFSServer has more arguments. The first argument
//! should be to TCP port, on which the starts to listen. The next two 
//! arguments are the TLS credentials: A private key for the encryption of the
//! network connection and a certificate, which allows the client to verify the
//! servers identity. As last parameter, it takes another VFS which the server
//! exposes over the https connection. The server also needs to take some
//! arguments for the authentication process.
//!
//! For an example see example directory.
//!
//! TODO:
//! - Implement authentication process
//! - Implement a [CGI](https://en.wikipedia.org/wiki/Common_Gateway_Interface)
//!   version of the HttpsFSServer.
//!     * This would allow a user to use any webserver provided by its
//!       favorite web-hoster as an infrastructure. The advantage is, that the
//!       web-hoster can overtake the certificate management, which is often 
//!       perceived as a liability.
//! - Write a HttpsFS version, which can be compiled to WebAssembly
//! - Consider to provide an non-blocking version of HttpsFS
//! - Change result of FileSystem::exists() from bool to VfsResult<bool> 
//! - Do version check after connecting to a HttpsFSServer
//! - Do not expose reqwest::Certificate and rustls::Certificate via the API
//! - Look for some unwrap(), which can be removed.
//! - Can we add Deserialize and Serialize to VfsResult/VfsMetadata.
//! - Due to the serialization and deserialization all VfsErrors are converted to
//!   VfsError::Other 
//!
//! Potential issues:
//! - The FileSystem trait works with the traits Read and Write, which assumes
//!   an unbuffered access to the files. Intuitively i resist to implement an
//!   unbuffered file access, since https has quiet a lot of overhead and a
//!   10 byte read would be totally silly. But it seams, that that in the most
//!   examples wrap a Read in a BufRead, which solves this issue.

use crate::{FileSystem, VfsResult, VfsMetadata, SeekAndRead, VfsError, VfsFileType};
use std::fmt::{Debug, Formatter};
use std::io::{Write, Read, Seek, Error, ErrorKind};
use std::sync;
use std::pin::Pin;
use core::task::{Poll, Context};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Response, Request, StatusCode, Server};
use tokio::net::{TcpListener, TcpStream};
use async_stream::stream;
use futures_util::{stream::Stream};
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use rustls::internal::pemfile;
use reqwest::blocking::Client;
use serde::{Serialize, Deserialize};

/// A file system exposed over https
pub struct HttpsFS {
    addr: String,
    client: std::sync::Arc<reqwest::blocking::Client>,
}


/// Helper structure for building HttpsFS structs
pub struct HttpsFSBuilder {
    port: u16,
    domain: String,
    root_certs: Vec::<reqwest::Certificate>,
}


/// A https server providing a interface for HttpsFS
pub struct HttpsFSServer<T: FileSystem> {
    port: u16,
    certs: Vec::<rustls::Certificate>,
    private_key: rustls::PrivateKey,
    file_system: std::sync::Arc<T>,
}


struct WritableFile {
    client: std::sync::Arc<reqwest::blocking::Client>,
    addr: String,
    file_name : String,
    position : u64,
}

struct ReadableFile {
    client: std::sync::Arc<reqwest::blocking::Client>,
    addr: String,
    file_name : String,
    position : u64,
}

#[derive(Debug, Deserialize, Serialize)]
enum Command {
    Exists(CommandExists),
    Metadata(CommandMetadata),
    CreateFile(CommandCreateFile),
    RemoveFile(CommandRemoveFile),
    Write(CommandWrite),
    Read(CommandRead),
    CreateDir(CommandCreateDir),
    ReadDir(CommandReadDir),
    RemoveDir(CommandRemoveDir),
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandExists {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandMetadata {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandCreateFile {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandRemoveFile {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandWrite {
    path: String,
    pos: u64,
    len: u64,
    /// Base64 encoded data
    data: String,
}


#[derive(Debug, Deserialize, Serialize)]
struct CommandRead {
    path: String,
    pos: u64,
    len: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandCreateDir {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandReadDir {
    path: String,
}


#[derive(Debug, Deserialize, Serialize)]
struct CommandRemoveDir {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
enum CommandResponse {
    Exists(bool),
    Metadata(Result<CmdMetadata, String>),
    CreateFile(CommandResponseCreateFile),
    RemoveFile(Result<(), String>),
    Write(Result<usize, String>),
    Read(Result<(usize, String), String>),
    CreateDir(CommandResponseCreateDir),
    ReadDir(CommandResponseReadDir),
    RemoveDir(Result<(), String>),
}

#[derive(Debug, Deserialize, Serialize)]
enum CommandResponseCreateFile {
    Success,
    Failed,
}

#[derive(Debug, Deserialize, Serialize)]
enum CommandResponseCreateDir {
    Success,
    Failed,
}

#[derive(Debug, Deserialize, Serialize)]
struct CommandResponseReadDir {
    result: Result<Vec<String>, String>,
}

// TODO: Should we add Deserialize and Serialize to VfsResult/VfsMetadata
#[derive(Debug, Deserialize, Serialize)]
struct CmdMetadata {
    file_type: CmdFileType,
    len: u64,
}

#[derive(Debug, Deserialize, Serialize)]
enum CmdFileType {
    File,
    Directory,
}


impl From<VfsMetadata> for CmdMetadata {
    fn from(vfs_meta: VfsMetadata) -> Self {
        CmdMetadata {
            file_type: CmdFileType::from(vfs_meta.file_type),
            len: vfs_meta.len,
        }
    }
}

impl From<CmdMetadata> for VfsMetadata {
    fn from(cmd_meta: CmdMetadata) -> Self {
        VfsMetadata {
            file_type: VfsFileType::from(cmd_meta.file_type),
            len: cmd_meta.len,
        }
    }
}

impl From<VfsFileType> for CmdFileType {
    fn from(vfs_file_type: VfsFileType) -> Self {
        match vfs_file_type {
            VfsFileType::File => CmdFileType::File,
            VfsFileType::Directory => CmdFileType::Directory,
        }
    }
}

impl From<CmdFileType> for VfsFileType {
    fn from(cmd_file_type: CmdFileType) -> Self {
        match cmd_file_type {
            CmdFileType::File => VfsFileType::File,
            CmdFileType::Directory => VfsFileType::Directory,
        }
    }
}

fn meta_res_convert_vfs_cmd(result: VfsResult<VfsMetadata>) -> Result<CmdMetadata, String> {
    match result {
        Err(e) => Err(format!("{:?}", e)),
        Ok(meta) => Ok(CmdMetadata::from(meta)),
    }
}

fn meta_res_convert_cmd_vfs(result: Result<CmdMetadata, String>) -> VfsResult<VfsMetadata> {
    match result {
        Err(e) => Err(VfsError::Other {message: e}),
        Ok(meta) => Ok(VfsMetadata::from(meta)),
    }
}

impl From<Result<Box<(dyn std::io::Write + 'static)>, VfsError>> for CommandResponseCreateFile {
    fn from(result: Result<Box<(dyn std::io::Write + 'static)>, VfsError>) -> Self {
        match result {
            Ok(_) => CommandResponseCreateFile::Success,
            Err(_) => CommandResponseCreateFile::Failed,
        }
    }
}

impl From<Result<(), VfsError>> for CommandResponseCreateDir {
    fn from(result: Result<(), VfsError>) -> Self {
        match result {
            Ok(_) => CommandResponseCreateDir::Success,
            Err(_) => CommandResponseCreateDir::Failed,
        }
    }
}

impl From<VfsResult<Box<dyn Iterator<Item = String>>>> for CommandResponseReadDir {
    fn from(result: VfsResult<Box<dyn Iterator<Item = String>>>) -> Self {
        match result {
            Err(e) => CommandResponseReadDir {
                result : Err(format!("{:?}", e)),
            },
            Ok(it) => CommandResponseReadDir {
                result : Ok(it.collect()),
            },
        }
    }
}




impl Debug for HttpsFS {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Over Https Exposed File System.")
    }
}


impl HttpsFS {
    /// Create a new https filesystem
    pub fn new(domain: &str) -> VfsResult<Self> {
        HttpsFS::builder(domain).build()
    }

    pub fn builder(domain: &str) -> HttpsFSBuilder {
        HttpsFSBuilder::new(domain)
    }

    pub fn load_certificate(filename: &str) -> VfsResult<reqwest::Certificate> {
        let mut buf = Vec::new();
        std::fs::File::open(filename)?
                      .read_to_end(&mut buf)?;
        let cert = reqwest::Certificate::from_pem(&buf)?;
        Ok(cert)
    }
}

impl HttpsFSBuilder {
    pub fn new(domain: &str) -> Self {
        HttpsFSBuilder {
            port: 443,
            domain: String::from(domain),
            root_certs: Vec::new(),
        }
    }

    pub fn set_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn set_domain(mut self, domain: &str) -> Self {
        self.domain = String::from(domain);
        self
    }

    pub fn add_root_certificate(mut self, cert: reqwest::Certificate) -> Self {
        self.root_certs.push(cert);
        self
    }

    pub fn build(self) -> VfsResult<HttpsFS> {
        let mut client = Client::builder()
                            .https_only(true);
        for cert in self.root_certs {
            client = client.add_root_certificate(cert);
        }

        let client = client.build()?;
        Ok(HttpsFS {
            client : std::sync::Arc::new(client),
            addr: format!("https://{}:{}/", self.domain, self.port),
        })
    }
}


impl From<reqwest::Error> for VfsError {
    fn from(e: reqwest::Error) -> Self {
        VfsError::Other { message: format!("{}", e) }
    }
}

impl From<hyper::Error> for VfsError {
    fn from(e: hyper::Error) -> Self {
        VfsError::Other { message: format!("{}", e) }
    }
}

impl From<serde_json::Error> for VfsError {
    fn from(e: serde_json::Error) -> Self {
        VfsError::Other { message: format!("{}", e) }
    }
}

impl<T:FileSystem> HttpsFSServer<T> {
    pub fn new(port: u16, certs: Vec::<rustls::Certificate>, private_key: rustls::PrivateKey, file_system: T) -> Self {
        // Initially i tried to store a hyper::server::Server object in HttpsFSServer. 
        // I failed, since this type is a very complicated generic and i could
        // not figure out, how to write down the type. 
        // The type definition is:
        //
        // impl<I, IO, IE, S, E, B> Server<I, S, E>
        //   where
        //     I: Accept<Conn = IO, Error = IE>,
        //     IE: Into<Box<dyn StdError + Send + Sync>>,
        //     IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
        //     S: MakeServiceRef<IO, Body, ResBody = B>,
        //     S::Error: Into<Box<dyn StdError + Send + Sync>>,
        //     B: HttpBody + Send + Sync + 'static,
        //     B::Error: Into<Box<dyn StdError + Send + Sync>>,
        //     E: ConnStreamExec<<S::Service as HttpService<Body>>::Future, B>,
        //     E: NewSvcExec<IO, S::Future, S::Service, E, GracefulWatcher>,
        //
        // This makes this struct almost impossible to use in situation, where one can not 
        // rely on rust type inference system. Currently i consider this as bad API design.
        HttpsFSServer { 
            port,
            certs,
            private_key,
            file_system : std::sync::Arc::new(file_system),
        }
    }

    /// Start the server
    #[tokio::main]
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("127.0.0.1:{}", self.port);
        let fs = self.file_system.clone();

        let mut cfg = rustls::ServerConfig::new(rustls::NoClientAuth::new());
        cfg.set_single_cert(self.certs.clone(), self.private_key.clone()).map_err(|e| Error::new(ErrorKind::Other, format!("{}", e)))?;
        cfg.set_protocols(&[b"http/2".to_vec(), b"http/1.1".to_vec()]);
        let tls_conf = sync::Arc::new(cfg);
        
        let tcp = TcpListener::bind(&addr).await?;
        let tls_acceptor = TlsAcceptor::from(tls_conf);

        let incoming_tls_stream = stream! {
            loop {
                let (socket, _) = tcp.accept().await?;
                let stream = tls_acceptor.accept(socket);
                let res = stream.await;
                if let Err(e) = res {
                    println!("TLS Error: {:?}", e);
                    continue;
                }
                yield res;
            }
        };

        // The next let statement is rather complicated:
        // It is a variant of the [Factory method pattern](https://en.wikipedia.org/wiki/Factory_method_pattern)
        // implemented by two closures. In this case, i named the first closure 'factory' and the
        // second closure 'product' (see comments). This is needed, since 'hyper' serves each
        // connection with a different instance of a service. Since we don't know, how many
        // connections have to be served in the future, we give 'hyper' this factory and than it
        // can create services on demand.  But our factory is not producing the service immediately.
        // If we call our factory, it only creates an instruction book and the needed materials, so
        // that we can build the service by ourself later. That means, we get a
        // [future](https://docs.rs/futures/0.3.12/futures/) from our factory, which can be
        // executed later to create our service. Even the service method is a future.
        //
        // The tricky part is, that a closure can be moved out of the current contest. 
        // Therefore, we can not borrow any values from the current context, since the values
        // of the current context might have a shorter lifetime than our 'factory'. In this
        // example, since we wait until the server finishes its execution in the same 
        // contest ("server.await?;"). I'm not sure, whether the lifetime analysis of the rust
        // does not under stand that or whether a 'static lifetime is required by some types
        // provided by hyper.
        // The result of this is, that we cannot have an object which implements FileSystem
        // in the HttpsFSServer and than borrow it the factory and than to the service.
        //
        // 'hyper' also forces us, to use types, which have implemented the 'Send' trait. Therefor
        // we can not use a single-threaded reference count (std::rc:Rc) but have to use a
        // thread save variant (std::sync::Arc) instead. WARNING: Be aware, that the reference
        // counter is thread save, but the data to which is points is not protected. But at the
        // moment we use a single threaded version of hyper (at least i didn't found any hint,
        // that this is multi-threaded).
        let service_factory = make_service_fn(
            // factory closure
            move |_| {
                let fs = fs.clone();
                async move { // return a future (instruction book to create or)
                    Ok::<_, Error>(service_fn(
                        // product closure
                        move |request| {
                            let fs = fs.clone();
                            HttpsFSServer::https_fs_service(fs, request)
                        }
                    ))
                }
            });

        let server = Server::builder(HyperAcceptor {
            acceptor: Box::pin(incoming_tls_stream),
        })
        .serve(service_factory);

        println!("Starting to serve on https://{}.", addr);

        server.await?;

        Ok(())
    }

    async fn https_fs_service(file_system: std::sync::Arc<T>, req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        let mut response = Response::new(Body::empty());
        match (req.method(), req.uri().path()) {
            (&Method::POST, "/") => {
                let body = hyper::body::to_bytes(req.into_body()).await?;
                let req:Result<Command, serde_json::Error> = serde_json::from_slice(&body);
                println!("Server request: {:?}", req);
                
                match req {
                    // TODO: Add more logging for debug
                    Err(_) => *response.status_mut() = StatusCode::BAD_REQUEST,
                    Ok(value) => {
                        let res = HttpsFSServer::<T>::handle_command(&value, &*file_system);
                        let res = serde_json::to_string(&res);
                        println!("Server response: {:?}", res);
                        match res {
                            // TODO: Add more logging for debug
                            Err(_) => *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR,
                            Ok(value) => *response.body_mut() = Body::from(value),
                        }
                    }
                }
            }
            _ => {
                *response.status_mut() = StatusCode::NOT_FOUND;
            }
        };
        Ok(response)
    }

    fn handle_command(command: &Command, file_system: &dyn FileSystem) -> CommandResponse {
        match command {
            Command::Exists(param) => CommandResponse::Exists(file_system.exists(&param.path)),
            Command::Metadata(param) => CommandResponse::Metadata(meta_res_convert_vfs_cmd(file_system.metadata(&param.path))),
            Command::CreateFile(param) => CommandResponse::CreateFile(CommandResponseCreateFile::from(file_system.create_file(&param.path))),
            Command::RemoveFile(param) => CommandResponse::RemoveFile({
                let result = file_system.remove_file(&param.path);
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("{:?}", e)),
                }
            }),
            Command::Write(param) => CommandResponse::Write(HttpsFSServer::<T>::write(&param, file_system)),
            Command::Read(param) => CommandResponse::Read(HttpsFSServer::<T>::read(&param, file_system)),
            Command::CreateDir(param) => CommandResponse::CreateDir(CommandResponseCreateDir::from(file_system.create_dir(&param.path))),
            Command::ReadDir(param) => CommandResponse::ReadDir(CommandResponseReadDir::from(file_system.read_dir(&param.path))),
            Command::RemoveDir(param) => CommandResponse::RemoveDir({
                let result = file_system.remove_dir(&param.path);
                match result {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("{:?}", e)),
                }
            }),
        }
    }

    fn write(cmd: &CommandWrite, file_system: &dyn FileSystem) -> Result<usize, String> {
        if !file_system.exists(&cmd.path) {
            println!("WARN: Tried to write to non existing file.");
            return Err(String::from("File does not exists!"));
        }
        let meta = file_system.metadata(&cmd.path);
        if let Err(e) = meta {
            return Err(format!("{:?}", e));
        }
        let meta = meta.unwrap();
        if meta.len != cmd.pos {
            println!("WARN: We only support appending to files.");
            return Err(String::from("We only support appending to files!"));
        }
        let file = file_system.append_file(&cmd.path);
        if let Err(e) = file {
            return Err(format!("{:?}", e));
        }
        let mut file = file.unwrap();
        let data = base64::decode(&cmd.data);
        if let Err(e) = data {
            return Err(format!("Faild to decode data: {:?}", e));
        }
        let data = data.unwrap();
        let res = file.write(&data);

        match res {
            Err(e) => return Err(format!("{:?}", e)),
            Ok(size) => Ok(size),
        }
    }
    
    fn read(cmd: &CommandRead, file_system: &dyn FileSystem) -> Result<(usize, String), String> {
        let file = file_system.open_file(&cmd.path);
        if let Err(e) = file {
            return Err(format!("{:?}", e));
        }
        let mut file = file.unwrap();

        let mut data: Vec<u8> = vec![0; cmd.len as usize];

        let seek_res = file.seek(std::io::SeekFrom::Start(cmd.pos));
        if let Err(e) = seek_res {
            return Err(format!("{:?}", e));
        }

        let result = file.read(data.as_mut_slice());
        if let Err(e) = result {
            return Err(format!("{:?}", e));
        }
        let len = result.unwrap();
        let data = base64::encode(& mut data.as_mut_slice()[..len]);

        Ok((len, data))
    }
}

/// Load public certificate from file
pub fn load_certs(filename: &str) -> std::io::Result<Vec<rustls::Certificate>> {
    // Open certificate file
    let cert_file = std::fs::File::open(filename).map_err(|e| Error::new(ErrorKind::Other, format!("faild to open {}: {}", filename, e)))?;
    let mut cert_reader = std::io::BufReader::new(cert_file);
    pemfile::certs(&mut cert_reader).map_err(|_|  Error::new(ErrorKind::Other, "faild to load certificate"))
}

/// Load private key from file
pub fn load_private_key(filename: &str) -> std::io::Result<rustls::PrivateKey> {
    // Open keyfile
    let key_file = std::fs::File::open(filename).map_err(|e| Error::new(ErrorKind::Other, format!("faild to open {}: {}", filename, e)))?;
    let mut key_reader = std::io::BufReader::new(key_file);

    // Load and return a single private key
    let keys = pemfile::pkcs8_private_keys(&mut key_reader).map_err(|_| Error::new(ErrorKind::Other, "failed to load private pkcs8 key"))?;
    if keys.len() == 1 {
        return Ok(keys[0].clone());
    }

    let keys = pemfile::rsa_private_keys(&mut key_reader).map_err(|_|  Error::new(ErrorKind::Other, "failed to load private rsa key"))?;
    if keys.len() != 1 {
        println!("len: {}", keys.len());
        return Err(Error::new(ErrorKind::Other, "expected a single private key"));
    }
    Ok(keys[0].clone())
}


struct HyperAcceptor {
    acceptor: Pin<Box<dyn Stream<Item = Result<TlsStream<TcpStream>, Error>>>>,
}


impl hyper::server::accept::Accept for HyperAcceptor {
    type Conn = TlsStream<TcpStream>;
    type Error = Error;

    fn poll_accept(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        Pin::new(&mut self.acceptor).poll_next(cx)
    }
}

impl Write for WritableFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let req = Command::Write(CommandWrite {
            path: self.file_name.clone(),
            pos: self.position,
            len: buf.len() as u64,
            data: base64::encode(buf),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send();
        if let Err(e) = result {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));
        }
        let result = result.unwrap();
        let result = result.text();
        if let Err(e) = result {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));
        }
        let result = result.unwrap();
        let result:CommandResponse = serde_json::from_str(&result)?;
        match result {
            CommandResponse::Write(result) => {
                match result {
                    Ok(size) => {
                        self.position += size as u64;
                        Ok(size)
                    },
                    Err(e) => {
                        Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))
                    }
                }
            },
            _ => Err(std::io::Error::new(std::io::ErrorKind::Other, String::from("Result doesn't match the request!" )))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        todo!("flush()");
    }
}

impl Read for ReadableFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let req = Command::Read(CommandRead {
            path: self.file_name.clone(),
            pos: self.position,
            len: buf.len() as u64,
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send();
        if let Err(e) = result {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));
        }
        let result = result.unwrap();
        let result = result.text();
        if let Err(e) = result {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));
        }
        let result = result.unwrap();
        let result:CommandResponse = serde_json::from_str(&result)?;
        match result {
            CommandResponse::Read(result) => {
                match result {
                    Ok((size, data)) => {
                        self.position += size as u64;
                        let decoded_data = base64::decode(data);
                        let mut result = Err(std::io::Error::new(std::io::ErrorKind::Other, String::from("Faild to decode data" )));
                        if let Ok(data) = decoded_data {
                           buf[..size].copy_from_slice(&data.as_slice()[..size]);
                           result = Ok(size);
                        }
                        result
                    },
                    Err(e) => {
                        Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)))
                    }
                }
            },
            _ => Err(std::io::Error::new(std::io::ErrorKind::Other, String::from("Result doesn't match the request!" )))
        }
    }
}

impl Seek for ReadableFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        match pos {
            std::io::SeekFrom::Start(offset) => self.position = offset,
            std::io::SeekFrom::Current(offset) => self.position = (self.position as i64 + offset) as u64,
            std::io::SeekFrom::End(offset) => {
                let fs = HttpsFS {
                    addr: self.addr.clone(),
                    client: self.client.clone(),
                };
                let meta = fs.metadata(&self.file_name);
                if let Err(e) = meta {
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{:?}", e)));
                }
                let meta = meta.unwrap();
                self.position = (meta.len as i64 + offset) as u64
            }
        }
        Ok(self.position)
    }
}


impl FileSystem for HttpsFS {
    fn read_dir(&self, path: &str) -> VfsResult<Box<dyn Iterator<Item = String>>> {
        let req = Command::ReadDir(CommandReadDir{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        let result = match result {
            CommandResponse::ReadDir(value) => value,
            _ => { return Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}); },
        };
        match result.result {
            Err(e) => Err(VfsError::Other { message: format!("{}", e) }),
            Ok(value) => Ok(Box::new(value.into_iter())),
        }
    }

    fn create_dir(&self, path: &str) -> VfsResult<()> {
        let req = Command::CreateDir(CommandCreateDir{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        let result = match result {
            CommandResponse::CreateDir(value) => value,
            _ => { return Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}); },
        };

        match result {
            CommandResponseCreateDir::Failed => Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}),
            CommandResponseCreateDir::Success => Ok(()),
        }
    }
    
    fn open_file(&self, path: &str) -> VfsResult<Box<dyn SeekAndRead>> {
        if !self.exists(path) {
            return Err(VfsError::FileNotFound { path: path.to_string() })?;
        }

        Ok(Box::new(ReadableFile{
            client : self.client.clone(),
            addr : self.addr.clone(),
            file_name : String::from(path),
            position : 0,
        }))
    }

    fn create_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        let req = Command::CreateFile(CommandCreateFile{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        let result = match result {
            CommandResponse::CreateFile(value) => value,
            _ => { return Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}); },
        };

        match result {
            CommandResponseCreateFile::Failed => Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}),
            CommandResponseCreateFile::Success => Ok(Box::new(WritableFile {
                client : self.client.clone(),
                addr : self.addr.clone(),
                file_name : String::from(path),
                position : 0,
            })),
        }
    }

    fn append_file(&self, path: &str) -> VfsResult<Box<dyn Write>> {
        let meta = self.metadata(path)?;
        Ok(Box::new(WritableFile {
            client : self.client.clone(),
            addr : self.addr.clone(),
            file_name : String::from(path),
            position : meta.len,
        }))
    }

    fn metadata(&self, path: &str) -> VfsResult<VfsMetadata> {
        let req = Command::Metadata(CommandMetadata{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        match result {
            CommandResponse::Metadata(value) => meta_res_convert_cmd_vfs(value),
            _ => Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}),
        }
    }

    fn exists(&self, path: &str) -> bool {
        // TODO: Add more logging 
        // TODO: try to change return type to VfsResult<bool>
        //       At the moment 'false' does not mean, that the file either does not exist
        //       or that an error has occurred. An developer does not expect this.
        let req = Command::Exists(CommandExists{
            path: String::from(path),
        });

        let req = serde_json::to_string(&req);
        if let Err(e) = req {
            println!("Error: {:?}", e);
            return false;
        }
        let req = req.unwrap();
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send();
        if let Err(e) = result {
            println!("Error: {:?}", e);
            return false;
        }
        let result = result.unwrap().text();
        if let Err(e) = result {
            println!("Error: {:?}", e);
            return false;
        }
        let result:Result<CommandResponse, serde_json::Error> = serde_json::from_str(&result.unwrap());
        if let Err(e) = result {
            println!("Error: {:?}", e);
            return false;
        }
        match result.unwrap() {
            CommandResponse::Exists(value) => value,
            _ => false
        }
    }

    fn remove_file(&self, path: &str) -> VfsResult<()> {
        let req = Command::RemoveFile(CommandRemoveFile{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        let result = match result {
            CommandResponse::RemoveFile(value) => value,
            _ => { return Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}); },
        };

        match result {
            Err(e) => Err(VfsError::Other { message: format!("{:?}", e) }),
            Ok(_) => Ok(()),
        }
    }

    fn remove_dir(&self, path: &str) -> VfsResult<()> {
        let req = Command::RemoveDir(CommandRemoveDir{
            path: String::from(path),
        });
        let req = serde_json::to_string(&req)?;
        let result = self.client.post(&self.addr)
                                .body(req)
                                .send()?;
        let result = result.text()?;
        let result:CommandResponse = serde_json::from_str(&result)?;
        let result = match result {
            CommandResponse::RemoveDir(value) => value,
            _ => { return Err(VfsError::Other { message: String::from("Result doesn't match the request!" )}); },
        };

        match result {
            Err(e) => Err(VfsError::Other { message: format!("{:?}", e) }),
            Ok(_) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::MemoryFS;

    use super::*;
    use lazy_static::lazy_static;
    use std::sync::{Arc, Mutex};
    
    // Since we create a HttpsFSServer for each unit test, which are all executed 
    // in parallel we have to ensure, that each server is listening on a different
    // port. This is done with this shared variable.
    // WARNING: It will not be tested, whether a port is already used by another 
    //          program. In such a case, the corresponding unit test most likely 
    //          fails.
    lazy_static! {
        static ref PORT: Arc<Mutex<u16>> = Arc::new(Mutex::new(8344));
    }

    test_vfs!({
        let server_port;
        match PORT.lock() {
            Ok(mut x) => {
                println!("Number: {}", *x);
                server_port = *x;
                *x+=1;
            },
            Err(e) => panic!("Error: {:?}", e)
        }
        std::thread::spawn(move || {
            let fs = MemoryFS::new();
            let cert = load_certs("examples/cert/cert.crt").unwrap();
            let private_key = load_private_key("examples/cert/private-key.key").unwrap();
            let mut server = HttpsFSServer::new(server_port, cert, private_key, fs);
            let result = server.run();
            if let Err(e) = result {
                println!("WARNING: {:?}", e);
            }
        });

        // make sure, that the server is ready for the unit tests
        let duration = std::time::Duration::from_millis(10);
        std::thread::sleep(duration);

        // load self signed certificate
        // WARNING: When the certificate expire, than the unit tests will frail.
        //          In this case, a new certificate hast to be generated.
        let cert = HttpsFS::load_certificate("examples/cert/cert.crt").unwrap();
        HttpsFS::builder("localhost")
                .set_port(server_port)
                .add_root_certificate(cert)
                .build()
                .unwrap()
    });


}
