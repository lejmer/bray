use std::net::{
    SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket,
};

use crate::{PlatformError, PlatformOperation};

/// Resolves a host address through the native resolver.
pub fn resolve_socket_addresses(
    address: impl ToSocketAddrs,
) -> Result<Vec<SocketAddr>, PlatformError> {
    address
        .to_socket_addrs()
        .map(Iterator::collect)
        .map_err(|error| {
            PlatformError::from_io(
                PlatformOperation::SocketAddressResolution,
                &error,
            )
        })
}

/// Binds a native TCP listener.
pub fn bind_tcp_listener(
    address: impl ToSocketAddrs,
) -> Result<TcpListener, PlatformError> {
    TcpListener::bind(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketBind, &error)
    })
}

/// Connects a native TCP stream.
pub fn connect_tcp(address: impl ToSocketAddrs) -> Result<TcpStream, PlatformError> {
    TcpStream::connect(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketConnect, &error)
    })
}

/// Binds a native UDP socket.
pub fn bind_udp_socket(
    address: impl ToSocketAddrs,
) -> Result<UdpSocket, PlatformError> {
    UdpSocket::bind(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketBind, &error)
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::thread;

    use super::{bind_tcp_listener, connect_tcp, resolve_socket_addresses};

    #[test]
    fn native_tcp_handles_transfer_owned_bytes() {
        let listener = bind_tcp_listener(("127.0.0.1", 0))
            .unwrap_or_else(|error| panic!("listener must bind: {error:?}"));

        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("listener address must be available: {error:?}"));

        let server = thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .unwrap_or_else(|error| panic!("connection must be accepted: {error:?}"));

            let mut byte = [0];

            stream
                .read_exact(&mut byte)
                .unwrap_or_else(|error| panic!("server must read: {error:?}"));

            byte[0]
        });

        let mut client = connect_tcp(address)
            .unwrap_or_else(|error| panic!("client must connect: {error:?}"));

        client
            .write_all(&[42])
            .unwrap_or_else(|error| panic!("client must write: {error:?}"));

        assert_eq!(
            server
                .join()
                .unwrap_or_else(|_| panic!("server must not panic")),
            42
        );
    }

    #[test]
    fn native_address_resolution_retains_all_host_results() {
        let addresses = resolve_socket_addresses(("127.0.0.1", 80))
            .unwrap_or_else(|error| panic!("literal address must resolve: {error:?}"));

        assert!(!addresses.is_empty());
    }
}
