use std::io::{self, Read, Write};
use std::net::{
    SocketAddr, TcpListener as StdTcpListener, TcpStream as StdTcpStream,
    ToSocketAddrs, UdpSocket as StdUdpSocket,
};

use mio::net::{TcpListener, TcpStream, UdpSocket};

use crate::{
    NativeEventPoller, NativePollEvent, NativePollInterest, PlatformError,
    PlatformOperation,
};

macro_rules! poll_registration {
    () => {
        /// Registers this socket with a native event poller.
        pub fn register(
            &mut self,
            poller: &NativeEventPoller,
            event: NativePollEvent,
            interest: NativePollInterest,
        ) -> Result<(), PlatformError> {
            poller.register_source(&mut self.0, event, interest)
        }

        /// Changes this socket's poll registration.
        pub fn reregister(
            &mut self,
            poller: &NativeEventPoller,
            event: NativePollEvent,
            interest: NativePollInterest,
        ) -> Result<(), PlatformError> {
            poller.reregister_source(&mut self.0, event, interest)
        }

        /// Removes this socket from a native event poller.
        pub fn deregister(
            &mut self,
            poller: &NativeEventPoller,
        ) -> Result<(), PlatformError> {
            poller.deregister_source(&mut self.0)
        }
    };
}

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

/// Owned nonblocking native TCP listener.
#[derive(Debug)]
pub struct NativeTcpListener(TcpListener);

impl NativeTcpListener {
    /// Returns the listener's bound native address.
    pub fn local_addr(&self) -> Result<SocketAddr, PlatformError> {
        socket_address(self.0.local_addr())
    }

    /// Accepts one ready connection without blocking.
    pub fn accept(
        &self,
    ) -> Result<(NativeTcpStream, SocketAddr), PlatformError> {
        self.0
            .accept()
            .map(|(stream, address)| (NativeTcpStream(stream), address))
            .map_err(|error| {
                PlatformError::from_io(PlatformOperation::SocketAccept, &error)
            })
    }

    poll_registration!();
}

/// Owned nonblocking native TCP stream.
#[derive(Debug)]
pub struct NativeTcpStream(TcpStream);

impl NativeTcpStream {
    /// Returns the stream's local native address.
    pub fn local_addr(&self) -> Result<SocketAddr, PlatformError> {
        socket_address(self.0.local_addr())
    }

    /// Returns the stream's peer native address.
    pub fn peer_addr(&self) -> Result<SocketAddr, PlatformError> {
        socket_address(self.0.peer_addr())
    }

    poll_registration!();
}

impl Read for NativeTcpStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for NativeTcpStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Owned nonblocking native UDP socket.
#[derive(Debug)]
pub struct NativeUdpSocket(UdpSocket);

impl NativeUdpSocket {
    /// Returns the socket's bound native address.
    pub fn local_addr(&self) -> Result<SocketAddr, PlatformError> {
        socket_address(self.0.local_addr())
    }

    /// Receives one datagram without blocking.
    pub fn receive_from(
        &self,
        buffer: &mut [u8],
    ) -> Result<(usize, SocketAddr), PlatformError> {
        self.0.recv_from(buffer).map_err(|error| {
            PlatformError::from_io(PlatformOperation::SocketReceive, &error)
        })
    }

    /// Sends one datagram without blocking.
    pub fn send_to(
        &self,
        buffer: &[u8],
        target: SocketAddr,
    ) -> Result<usize, PlatformError> {
        self.0.send_to(buffer, target).map_err(|error| {
            PlatformError::from_io(PlatformOperation::SocketSend, &error)
        })
    }

    poll_registration!();
}

/// Binds a nonblocking native TCP listener.
pub fn bind_tcp_listener(
    address: impl ToSocketAddrs,
) -> Result<NativeTcpListener, PlatformError> {
    let listener = StdTcpListener::bind(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketBind, &error)
    })?;

    listener.set_nonblocking(true).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketConfiguration, &error)
    })?;

    Ok(NativeTcpListener(TcpListener::from_std(listener)))
}

/// Connects a nonblocking native TCP stream.
pub fn connect_tcp(
    address: impl ToSocketAddrs,
) -> Result<NativeTcpStream, PlatformError> {
    let stream = StdTcpStream::connect(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketConnect, &error)
    })?;

    stream.set_nonblocking(true).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketConfiguration, &error)
    })?;

    Ok(NativeTcpStream(TcpStream::from_std(stream)))
}

/// Binds a nonblocking native UDP socket.
pub fn bind_udp_socket(
    address: impl ToSocketAddrs,
) -> Result<NativeUdpSocket, PlatformError> {
    let socket = StdUdpSocket::bind(address).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketBind, &error)
    })?;

    socket.set_nonblocking(true).map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketConfiguration, &error)
    })?;

    Ok(NativeUdpSocket(UdpSocket::from_std(socket)))
}

fn socket_address(
    result: io::Result<SocketAddr>,
) -> Result<SocketAddr, PlatformError> {
    result.map_err(|error| {
        PlatformError::from_io(PlatformOperation::SocketAddress, &error)
    })
}

#[cfg(test)]
mod tests {
    use std::io::{ErrorKind, Read, Write};
    use std::thread;
    use std::time::Duration;

    use crate::{
        MonotonicClock, NativeEventPoller, NativePollEvent, NativePollInterest,
        NativePollReady,
    };

    use super::{bind_tcp_listener, connect_tcp, resolve_socket_addresses};

    #[test]
    fn native_tcp_handles_transfer_owned_bytes() {
        let listener = bind_tcp_listener(("127.0.0.1", 0))
            .unwrap_or_else(|error| panic!("listener must bind: {error:?}"));

        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("listener address must be available: {error:?}"));

        let server = thread::spawn(move || {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind()
                            == crate::PlatformErrorKind::Io(ErrorKind::WouldBlock) =>
                    {
                        thread::yield_now();
                    }
                    Err(error) => panic!("connection must be accepted: {error:?}"),
                }
            };

            let mut byte = [0];

            loop {
                match stream.read_exact(&mut byte) {
                    Ok(()) => break,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(error) => panic!("server must read: {error:?}"),
                }
            }

            byte[0]
        });

        let mut client = connect_tcp(address)
            .unwrap_or_else(|error| panic!("client must connect: {error:?}"));

        loop {
            match client.write_all(&[42]) {
                Ok(()) => break,
                Err(error) if error.kind() == ErrorKind::WouldBlock => {
                    thread::yield_now();
                }
                Err(error) => panic!("client must write: {error:?}"),
            }
        }

        assert_eq!(
            server
                .join()
                .unwrap_or_else(|_| panic!("server must not panic")),
            42
        );
    }

    #[test]
    fn native_poller_reports_socket_readiness_and_timeouts() {
        let mut listener = bind_tcp_listener(("127.0.0.1", 0))
            .unwrap_or_else(|error| panic!("listener must bind: {error:?}"));

        let address = listener
            .local_addr()
            .unwrap_or_else(|error| panic!("listener address must be available: {error:?}"));

        let mut poller = NativeEventPoller::new()
            .unwrap_or_else(|error| panic!("poller must initialize: {error:?}"));

        let event = NativePollEvent::new(17);

        listener
            .register(&poller, event, NativePollInterest::Readable)
            .unwrap_or_else(|error| panic!("listener must register: {error:?}"));

        assert_eq!(
            poller
                .wait(MonotonicClock.deadline_after(Duration::ZERO))
                .unwrap_or_else(|error| panic!("zero-time poll must succeed: {error:?}")),
            None
        );

        let _client = connect_tcp(address)
            .unwrap_or_else(|error| panic!("client must connect: {error:?}"));

        let ready = poller
            .wait(MonotonicClock.deadline_after(Duration::from_secs(1)))
            .unwrap_or_else(|error| panic!("socket poll must succeed: {error:?}"));

        assert!(matches!(
            ready,
            Some(NativePollReady::Io {
                event: ready_event,
                readable: true,
                ..
            }) if ready_event == event
        ));
    }

    #[test]
    fn native_address_resolution_retains_all_host_results() {
        let addresses = resolve_socket_addresses(("127.0.0.1", 80))
            .unwrap_or_else(|error| panic!("literal address must resolve: {error:?}"));

        assert!(!addresses.is_empty());
    }
}
