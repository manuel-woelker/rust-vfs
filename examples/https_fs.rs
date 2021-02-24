use vfs::HttpsFS;
use vfs::VfsPath;
use chrono::prelude::*;
use std::io::Read;

// this is an example for the HttpsFS which creates a file "example.txt" and
// writes one new line to it, containing the current date and "Hello World!".
// Afterwards it reads the whole file and prints the content to the stdout.
//
// As long as the server is not restarted, the output of this program will
// change with each call.
//
// Since the example HttpsFSServer (https_fs_server.rs) is using a MemoryFS,
// the content of the server are lost after a restart.
fn main() -> vfs::VfsResult<()> {

    // load the certificate by our server (https_fs_server.rs)
    let cert = HttpsFS::load_certificate("examples/cert/cert.crt").unwrap();

    // create a server
    // You can not access the server from a different host, since our certificate
    // is issued for the localhost and you have to use https://localhost:8443 to
    // access the server. You can not use IPs, i.g. https://127.0.0.1:8443, since
    // we didn't issue the certificate for the IP.
    let builder = HttpsFS::builder("localhost")
                          // Set the port, as default the client uses 443
                          .set_port(8443)
                          // we need to add our self signed certificate as root
                          // certificate, otherwise the client don't connect to the
                          // HttpsFSServer.
                          // If the server uses a certificate issued by a official
                          // certificate authority, than we don't need to add an additional
                          // certificate.
                          .add_root_certificate(cert);
    let root : VfsPath = builder.build().unwrap().into();
    let root = root.join("example.txt")?;

    // make sure that file exists
    if !root.exists() {
        root.create_file()?;
    }
    
    // add additional a new line
    let mut file = root.append_file()?;
    let time = Local::now();
    let line = format!("{}: Hello World!\n", time);
    file.write(line.as_bytes())?;

    // read file content
    let mut content = String::new();
    let file = root.open_file()?;

    // One should really use a BufReader, which reads files in chunks of 8kb.
    // The read() of the Read trait, issues a new request to the HttpsFSServer with 
    // each call, even if only on byte is read. The headers of the http-protocol needs
    // several hundred bytes, which makes small reads inefficient.
    let mut buffed_file = std::io::BufReader::new(file);
    buffed_file.read_to_string(&mut content)?;
    println!("Content example.txt: \n{}", content);

    Ok(())
}
