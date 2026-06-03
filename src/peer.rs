use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

/// A shared, reference-counted handle to a [`UdpPeer`].
///
/// Multiple parts of the framework (the recv loop, the timeout checker, the user
/// handler) can hold a clone of this to interact with the same remote client.
pub type UDPPeer = Arc<UdpPeer>;

/// Transmit end of a peer's data channel.
///
/// The socket receive loop pushes received bytes into this sender; the user
/// handler reads them from the paired [`UdpReader`].
pub type UdpSender = UnboundedSender<io::Result<Vec<u8>>>;

/// Receive end of a peer's data channel.
///
/// The user handler calls `reader.recv().await` to consume incoming packets.
/// When the peer is evicted by the timeout checker, the next `recv()` returns
/// `Some(Err(io::ErrorKind::TimedOut))`.
pub type UdpReader = UnboundedReceiver<io::Result<Vec<u8>>>;

/// A single remote UDP client, keyed by its [`SocketAddr`].
///
/// ## Lifecycle
///
/// 1. Created when a packet arrives from a previously unseen address.
/// 2. Data is pushed through the internal [`UdpSender`] channel.
/// 3. The user's handler reads from the channel via [`UdpReader`].
/// 4. When the handler future completes (or errors), the peer is removed from
///    the socket's peer map and dropped.
/// 5. If a timeout is configured, the timeout checker may call [`close`] to
///    push an error into the channel before step 4, signalling the handler to
///    shut down.
pub struct UdpPeer {
    /// Index of the socket that this peer was created from.
    pub socket_id: usize,
    /// The Tokio UDP socket shared across all peers on the same listening socket.
    pub udp_sock: Arc<UdpSocket>,
    /// Remote address of this peer.
    pub addr: SocketAddr,
    /// Transmit half of the unbounded channel for pushing received data to the
    /// user handler.
    sender: UdpSender,
    /// Timestamp (seconds since epoch) of the last received packet. Updated by
    /// the socket recv loop via [`push_data_and_update_instant`], read by the
    /// timeout checker task.
    last_read_time: AtomicI64,
}

impl Drop for UdpPeer {
    fn drop(&mut self) {
        log::trace!(
            "udp_listen socket:{} udp peer:{} drop",
            self.socket_id,
            self.addr
        )
    }
}

impl UdpPeer {
    /// Create a new peer and return the shared handle plus the reader half of
    /// its data channel.
    ///
    /// The returned [`UdpReader`] is given to the user's handler; the
    /// [`UDPPeer`] handle is stored in the socket's peer map and also passed to
    /// the handler for sending replies.
    #[inline]
    pub fn new(
        socket_id: usize,
        udp_sock: Arc<UdpSocket>,
        addr: SocketAddr,
    ) -> (UDPPeer, UdpReader) {
        let (tx, rx) = unbounded_channel();
        (
            Arc::new(Self {
                socket_id,
                udp_sock,
                addr,
                sender: tx,
                last_read_time: AtomicI64::new(timestamp_sec()),
            }),
            rx,
        )
    }

    /// Return the last recv timestamp for timeout checking.
    ///
    /// Uses `Ordering::Acquire` to ensure the read synchronizes with the
    /// `Release` store in [`push_data_and_update_instant`].
    #[inline]
    pub(crate) fn get_last_recv_sec(&self) -> i64 {
        self.last_read_time.load(Ordering::Acquire)
    }

    /// Push a received packet into the peer's channel **without** updating the
    /// last-recv timestamp.
    ///
    /// Used when peer timeout is disabled — the timestamp is irrelevant, so we
    /// skip the atomic store for a slight performance gain.
    #[inline]
    pub(crate) fn push_data(&self, buf: Vec<u8>) -> io::Result<()> {
        if let Err(err) = self.sender.send(Ok(buf)) {
            Err(io::Error::new(ErrorKind::Other, err))
        } else {
            Ok(())
        }
    }

    /// Push a received packet **and** update the last-recv timestamp.
    ///
    /// Only called when peer timeout is enabled. The atomic store uses
    /// `Ordering::Release` to pair with the `Acquire` load in
    /// [`get_last_recv_sec`].
    #[inline]
    pub(crate) async fn push_data_and_update_instant(&self, buf: Vec<u8>) -> io::Result<()> {
        self.last_read_time
            .store(timestamp_sec(), Ordering::Release);
        self.push_data(buf)
    }

    /// Return the index of the socket this peer belongs to.
    #[inline]
    pub fn get_socket_id(&self) -> usize {
        self.socket_id
    }

    /// Return the remote address of this peer.
    #[inline]
    pub fn get_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a buffer to the remote peer via the shared UDP socket.
    ///
    /// This is the primary method the user's handler calls to reply to the
    /// client.
    #[inline]
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.udp_sock.send_to(buf, &self.addr).await
    }

    /// Signal this peer to shut down.
    ///
    /// Pushes `Err(io::ErrorKind::TimedOut)` into the channel. The user's
    /// handler will see this error on the next `reader.recv().await` and should
    /// exit its loop, which causes the peer to be removed from the peer map.
    #[inline]
    pub fn close(&self) {
        if let Err(err) = self.sender.send(Err(io::Error::new(
            ErrorKind::TimedOut,
            "udp peer need close",
        ))) {
            log::error!("send timeout to udp peer:{} error:{err}", self.get_addr());
        }
    }
}

/// Return the current UTC timestamp in seconds since epoch.
///
/// Used as the "last seen" value for peer timeout tracking.
#[inline]
fn timestamp_sec() -> i64 {
    chrono::Utc::now().timestamp()
}
