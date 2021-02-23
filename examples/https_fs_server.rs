use vfs::{load_certs, load_private_key, HttpsFSServer, MemoryFS};

fn main() {
    // create a file system, which the server uses to access the file system
    // Note: You can also put another HttpsFS in the HttpsFSServer, which would
    //       redirect the request to another HttpsFSServer. Obviously, this does
    //       not make much sense.
    let fs = MemoryFS::new();

    // It is a https server, therefore we need to load certificate, which the server
    // uses. For the example we use a self signed certificate. If you are interested
    // in how to use the certificate, see "/examples/cert/create.sh"
    let cert = load_certs("examples/cert/cert.crt").unwrap();

    // We need also a private key, which belongs to the certificate
    let private_key = load_private_key("examples/cert/private-key.key").unwrap();

    // Since this test will not be executed as root, we are not allowed to listen on
    // a tcp port below 1000, such as the https port 443. Therefore lets take a
    // different port.
    let port = 8443;

    // The server will use this method to validate, whether the login credentials
    // are valide or not. In this example, only the username 'user' and the password
    // 'pass' is accepted.
    // As authentication process, 'Basic' method as defined by the
    // [RFC7617](https://tools.ietf.org/html/rfc7617) is used.
    let credential_validator =
        |username: &str, password: &str| username == "user" && password == "pass";

    // Initiate the server object
    let mut server = HttpsFSServer::new(port, cert, private_key, fs, credential_validator);

    // Start the server.
    server.run().unwrap();
}
