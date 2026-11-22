//! Waker for the Switch (`target_os = "horizon"`): libnx has no `pipe(2)` or `eventfd(2)`, and its
//! sockets are not files, so the waker is a UDP socket connected to itself on the loopback. `wake`
//! sends one byte; the socket is readable until `ack_and_reset` (or a full buffer) drains it.
use std::io;
use std::net::UdpSocket;
use std::os::fd::{AsRawFd, RawFd};

use crate::sys::Selector;
use crate::{Interest, Token};

#[derive(Debug)]
pub(crate) struct Waker {
    socket: UdpSocket,
}

impl Waker {
    #[allow(dead_code)] // Not used by the `poll(2)` implementation's own waker.
    pub(crate) fn new(selector: &Selector, token: Token) -> io::Result<Waker> {
        let waker = Waker::new_unregistered()?;
        selector.register(waker.socket.as_raw_fd(), token, Interest::READABLE)?;
        Ok(waker)
    }

    pub(crate) fn new_unregistered() -> io::Result<Waker> {
        let socket = UdpSocket::bind(("127.0.0.1", 0))?;
        socket.connect(socket.local_addr()?)?;
        socket.set_nonblocking(true)?;
        Ok(Waker { socket })
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        loop {
            match self.socket.send(&[1]) {
                Ok(_) => return Ok(()),
                // The receive buffer is full: it is already readable, but empty it and retry.
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => self.empty(),
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(err),
            }
        }
    }

    pub(crate) fn ack_and_reset(&self) {
        self.empty();
    }

    pub(crate) fn fd(&self) -> Option<RawFd> {
        Some(self.socket.as_raw_fd())
    }

    /// Only ever `true` for the `single_threaded.rs` implementation.
    pub(crate) fn woken(&self) -> bool {
        false
    }

    /// Reads the socket's datagrams until it would block; errors are ignored.
    fn empty(&self) {
        let mut buf = [0u8; 64];
        while self.socket.recv(&mut buf).is_ok() {}
    }
}

impl AsRawFd for Waker {
    fn as_raw_fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
}
