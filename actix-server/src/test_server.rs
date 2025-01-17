use std::sync::mpsc;
use std::{io, net, thread};

use actix_rt::{net::TcpStream, System};

use crate::{Server, ServerBuilder, ServerHandle, ServiceFactory};

/// A testing server.
///
/// `TestServer` is very simple test server that simplify process of writing integration tests for
/// network applications.
///
/// # Examples
/// ```
/// use actix_service::fn_service;
/// use actix_server::TestServer;
///
/// #[actix_rt::main]
/// async fn main() {
///     let srv = TestServer::with(|| fn_service(
///         |sock| async move {
///             println!("New connection: {:?}", sock);
///             Ok::<_, ()>(())
///         }
///     ));
///
///     println!("SOCKET: {:?}", srv.connect());
/// }
/// ```
pub struct TestServer;

/// Test server runtime
pub struct TestServerRuntime {
    addr: net::SocketAddr,
    host: String,
    port: u16,
    server_handle: ServerHandle,
    thread_handle: Option<thread::JoinHandle<io::Result<()>>>,
}

impl TestServer {
    /// Start new server with server builder.
    pub fn start<F>(mut factory: F) -> TestServerRuntime
    where
        F: FnMut(ServerBuilder) -> ServerBuilder + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();

        // run server in separate thread
        let thread_handle = thread::spawn(move || {
            System::new().block_on(async {
                let server = factory(Server::build()).workers(1).disable_signals().run();
                tx.send(server.handle()).unwrap();
                server.await
            })
        });

        let server_handle = rx.recv().unwrap();

        TestServerRuntime {
            addr: "127.0.0.1:0".parse().unwrap(),
            host: "127.0.0.1".to_string(),
            port: 0,
            server_handle,
            thread_handle: Some(thread_handle),
        }
    }

    /// Start new test server with application factory.
    pub fn with<F: ServiceFactory<TcpStream>>(factory: F) -> TestServerRuntime {
        let (tx, rx) = mpsc::channel();

        // run server in separate thread
        let thread_handle = thread::spawn(move || {
            let sys = System::new();
            let tcp = net::TcpListener::bind("127.0.0.1:0").unwrap();
            let local_addr = tcp.local_addr().unwrap();

            sys.block_on(async {
                let server = Server::build()
                    .listen("test", tcp, factory)
                    .unwrap()
                    .workers(1)
                    .disable_signals()
                    .run();

                tx.send((server.handle(), local_addr)).unwrap();
                server.await
            })
        });

        let (server_handle, addr) = rx.recv().unwrap();

        let host = format!("{}", addr.ip());
        let port = addr.port();

        TestServerRuntime {
            addr,
            host,
            port,
            server_handle,
            thread_handle: Some(thread_handle),
        }
    }

    /// Get first available unused local address.
    pub fn unused_addr() -> net::SocketAddr {
        let addr: net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let socket = mio::net::TcpSocket::new_v4().unwrap();
        socket.bind(addr).unwrap();
        socket.set_reuseaddr(true).unwrap();
        let tcp = socket.listen(1024).unwrap();
        tcp.local_addr().unwrap()
    }
}

impl TestServerRuntime {
    /// Test server host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Test server port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get test server address.
    pub fn addr(&self) -> net::SocketAddr {
        self.addr
    }

    /// Stop server.
    fn stop(&mut self) {
        let _ = self.server_handle.stop(false);
        self.thread_handle.take().unwrap().join().unwrap().unwrap();
    }

    /// Connect to server, returning a Tokio `TcpStream`.
    pub fn connect(&self) -> std::io::Result<TcpStream> {
        TcpStream::from_std(net::TcpStream::connect(self.addr)?)
    }
}

impl Drop for TestServerRuntime {
    fn drop(&mut self) {
        self.stop()
    }
}
