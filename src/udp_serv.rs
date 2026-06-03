use async_lock::Mutex;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use crate::peer::{UDPPeer, UdpPeer, UdpReader};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::unbounded_channel;

/// Default maximum UDP payload size in bytes.
///
/// MTU is typically ≤1500 on the internet; 4096 covers Ethernet jumbo frames
/// on LANs without being excessively large.
pub const BUFF_MAX_SIZE: usize = 4096;

/// Per-socket context, created once for each listening UDP socket.
///
/// Each context runs its own `recv_from` loop in a dedicated Tokio task. The
/// `peers` map is keyed by [`SocketAddr`] so the first packet from an address
/// creates a new peer, and subsequent packets reuse it.
pub struct UdpContext {
    /// Monotonically increasing index assigned at startup (0, 1, …).
    pub id: usize,
    /// The Tokio UDP socket used for both receiving and replying to peers on
    /// this context.
    recv: Arc<UdpSocket>,
    /// Map of all currently active peers on this socket. Locked with
    /// `async_lock::Mutex` because both the recv loop and the peer-removal
    /// path (inside the handler spawn) need access.
    pub peers: Mutex<HashMap<SocketAddr, UDPPeer>>,
}

// The Arc<UdpSocket> inside UdpContext is Send + Sync, but the raw struct
// containing Mutex<HashMap<…>> needs explicit impls because the compiler cannot
// auto-derive them across the Mutex indirection.
unsafe impl Send for UdpContext {}
unsafe impl Sync for UdpContext {}

/// A multi-socket UDP server.
///
/// ## Type Parameters
///
/// - `I`: the user's handler function type.
/// - `T`: user-defined shared state, cloned for each spawned peer handler.
///
/// ## Usage
///
/// ```rust,no_run
/// # use udp_server::prelude::UdpServer;
/// # #[tokio::main]
/// # async fn main() -> anyhow::Result<()> {
/// UdpServer::new("0.0.0.0:20001", |peer, mut reader, _| async move {
///     while let Some(Ok(data)) = reader.recv().await {
///         peer.send(&data).await?;
///     }
///     Ok(())
/// })?
/// .set_peer_timeout_sec(30)
/// .start(())
/// .await?;
/// # Ok(())
/// # }
/// ```
pub struct UdpServer<I, T> {
    /// One context per listening socket. Each context runs an independent
    /// recv loop.
    udp_contexts: Vec<Arc<UdpContext>>,
    /// The user-supplied handler closure, wrapped in Arc so it can be shared
    /// across all spawned peer tasks.
    input: Arc<I>,
    /// Binds T into the struct type without actually storing a value.
    _ph: PhantomData<T>,
    /// Peer idle timeout in seconds. `None` means peers never expire.
    clean_sec: Option<u64>,
}

impl<I, R, T> UdpServer<I, T>
where
    // The handler: `Fn(UDPPeer, UdpReader, T) -> Future<Result<(), Box<dyn Error>>>`
    I: Fn(UDPPeer, UdpReader, T) -> R + Send + Sync + 'static,
    R: Future<Output = Result<(), Box<dyn Error>>> + Send + 'static,
    // Shared state: must be clonable for each peer task, and thread-safe.
    T: Sync + Send + Clone + 'static,
{
    /// Create a new [`UdpServer`] bound to `addr`.
    ///
    /// Internally creates one socket per CPU core (Unix) or one socket
    /// (Windows) and wraps each in a [`UdpContext`].
    ///
    /// `input` is the handler closure invoked for every new peer.
    pub fn new<A: ToSocketAddrs>(addr: A, input: I) -> io::Result<Self> {
        // Create N UDP sockets bound to the same address (N = num_cpus on
        // Unix, 1 on Windows). Each socket gets its own UdpContext and
        // dedicated recv task.
        let udp_list = create_udp_socket_list(&addr, get_cpu_count())?;
        let udp_contexts = udp_list
            .into_iter()
            .enumerate()
            .map(|(id, socket)| {
                Arc::new(UdpContext {
                    id,
                    recv: Arc::new(socket),
                    peers: Default::default(),
                })
            })
            .collect();
        Ok(UdpServer {
            udp_contexts,
            input: Arc::new(input),
            _ph: Default::default(),
            clean_sec: None,
        })
    }

    /// Set the peer idle timeout in seconds.
    ///
    /// When set, a background task runs every second and closes any peer whose
    /// last received packet is older than `sec`. The peer's channel receives
    /// `Err(io::ErrorKind::TimedOut)`, which the handler should treat as a
    /// signal to exit.
    ///
    /// ## Panics
    ///
    /// Panics if `sec == 0`.
    #[inline]
    pub fn set_peer_timeout_sec(mut self, sec: u64) -> UdpServer<I, T> {
        assert!(sec > 0, "timeout must be greater than 0");
        self.clean_sec = Some(sec);
        self
    }

    /// Start the server and block until it shuts down.
    ///
    /// This method:
    ///
    /// 1. Optionally spawns a timeout-checker task (if `set_peer_timeout_sec`
    ///    was called).
    /// 2. Spawns one recv-loop task per socket context.
    /// 3. Runs a dispatch loop on the main task: for each new peer, the user's
    ///    handler is spawned as a new Tokio task with a clone of `inner`.
    ///
    /// The server shuts down when all sender halves are dropped (which happens
    /// when the `UdpServer` itself is dropped), causing the dispatch channel
    /// to close.
    #[inline]
    pub async fn start(&self, inner: T) -> io::Result<()> {
        // ---- Timeout checker ----
        // Only spawn if the user configured a timeout. The task runs a 1 Hz
        // scan over every peer in every context, closing those whose
        // last_read_time exceeds the threshold.
        let need_check_timeout = {
            if let Some(clean_sec) = self.clean_sec {
                let clean_sec = clean_sec as i64;
                let contexts = self.udp_contexts.clone();
                tokio::spawn(async move {
                    loop {
                        let current = chrono::Utc::now().timestamp();
                        for context in contexts.iter() {
                            context.peers.lock().await.values().for_each(|peer| {
                                if current - peer.get_last_recv_sec() > clean_sec {
                                    peer.close();
                                }
                            });
                        }
                        tokio::time::sleep(Duration::from_secs(1)).await
                    }
                });
                true
            } else {
                false
            }
        };

        // ---- Recv loops ----
        // One task per socket context. Each task loops on recv_from,
        // dispatches data to the appropriate peer, and notifies the main
        // dispatch loop about new peers via `tx`.
        let (tx, mut rx) = unbounded_channel();
        for (index, udp_listen) in self.udp_contexts.iter().enumerate() {
            let create_peer_tx = tx.clone();
            let udp_context = udp_listen.clone();
            tokio::spawn(async move {
                log::debug!("start udp listen:{index}");
                let mut buff = [0; BUFF_MAX_SIZE];
                loop {
                    match udp_context.recv.recv_from(&mut buff).await {
                        Ok((size, addr)) => {
                            let peer = {
                                // Look up or insert the peer in the context's
                                // peer map. New peers get a (peer, reader)
                                // pair sent to the dispatch loop via
                                // create_peer_tx so the user's handler can be
                                // spawned.
                                udp_context
                                    .peers
                                    .lock()
                                    .await
                                    .entry(addr)
                                    .or_insert_with(|| {
                                        let (peer, reader) =
                                            UdpPeer::new(index, udp_context.recv.clone(), addr);
                                        log::trace!("create udp listen:{index} udp peer:{addr}");
                                        if let Err(err) =
                                            create_peer_tx.send((peer.clone(), reader, index, addr))
                                        {
                                            // The rx half was dropped — the
                                            // server is shutting down.
                                            panic!("create_peer_tx err:{}", err);
                                        }
                                        peer
                                    })
                                    .clone()
                            };

                            // Push the received bytes into the peer's channel.
                            // Two variants: with or without timestamp update,
                            // depending on whether timeout tracking is enabled.
                            if need_check_timeout {
                                if let Err(err) = peer
                                    .push_data_and_update_instant(buff[..size].to_vec())
                                    .await
                                {
                                    log::error!("peer push data and update instant is error:{err}");
                                }
                            } else if let Err(err) = peer.push_data(buff[..size].to_vec()) {
                                log::error!("peer push data is error:{err}");
                            }
                        }
                        Err(err) => {
                            // recv_from errors are typically transient
                            // (e.g. EWOULDBLOCK), log at trace to avoid noise.
                            log::trace!("udp:{index} recv_from error:{err}");
                        }
                    }
                }
            });
        }
        // Drop the original sender so the channel closes when all recv-loop
        // senders are dropped (which happens on server shutdown).
        drop(tx);

        // ---- Dispatch loop ----
        // Runs on the main task. Each new peer arrives as a tuple from a recv
        // task; we spawn the user's handler in a new Tokio task, remove the
        // peer from the context map when the handler exits.
        while let Some((peer, reader, index, addr)) = rx.recv().await {
            let inner = inner.clone();
            let input_fn = self.input.clone();
            let context = self
                .udp_contexts
                .get(index)
                .expect("not found context")
                .clone();
            tokio::spawn(async move {
                if let Err(err) = (input_fn)(peer, reader, inner).await {
                    log::error!("udp input error:{err}")
                }
                // Handler finished — remove the peer from the map so a future
                // packet from this address will create a fresh peer.
                context.peers.lock().await.remove(&addr);
            });
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Socket creation with socket2
// ---------------------------------------------------------------------------

/// Create and configure a UDP socket bound to `addr`.
///
/// Sets `SO_REUSEADDR` on all platforms. On Unix, also sets `SO_REUSEPORT` so
/// the kernel distributes packets across multiple sockets bound to the same port.
///
/// Socket send/recv buffers are sized at ~17.8 MB for high-throughput workloads.
fn create_udp_socket<A: ToSocketAddrs>(addr: &A) -> io::Result<std::net::UdpSocket> {
    // Resolve the address, ensuring exactly one result.
    let addr = {
        let mut addrs = addr.to_socket_addrs()?;
        let addr = match addrs.next() {
            Some(addr) => addr,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "no socket addresses could be resolved",
                ))
            }
        };
        if addrs.next().is_none() {
            Ok(addr)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "more than one address resolved",
            ))
        }
    }?;

    // Create the socket with the appropriate domain.
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else if addr.is_ipv6() {
        Domain::IPV6
    } else {
        return Err(io::Error::new(io::ErrorKind::Other, "not address AF_INET"));
    };

    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;

    // SO_REUSEPORT allows binding multiple sockets to the same port; the kernel
    // load-balances incoming packets across them. Only available on Unix.
    #[cfg(not(target_os = "windows"))]
    socket.set_reuse_port(true)?;

    socket.bind(&addr.into())?;

    // Large buffers reduce drops under burst load (~17.8 MB each).
    socket.set_send_buffer_size(1784 * 10000)?;
    socket.set_recv_buffer_size(1784 * 10000)?;

    Ok(socket.into())
}

/// Convert a std UDP socket into a Tokio [`UdpSocket`].
///
/// The socket is set to non-blocking mode before conversion, as required by
/// Tokio's [`UdpSocket::try_from`].
fn create_async_udp_socket<A: ToSocketAddrs>(addr: &A) -> io::Result<UdpSocket> {
    let std_sock = create_udp_socket(&addr)?;
    std_sock.set_nonblocking(true)?;
    let sock = UdpSocket::try_from(std_sock)?;
    Ok(sock)
}

/// Create `listen_count` Tokio UDP sockets all bound to the same address.
///
/// Each socket gets its own `UdpContext` and dedicated recv task at runtime,
/// enabling kernel-level load distribution when `listen_count > 1`.
fn create_udp_socket_list<A: ToSocketAddrs>(
    addr: &A,
    listen_count: usize,
) -> io::Result<Vec<UdpSocket>> {
    log::debug!("cpus:{listen_count}");
    let mut listens = Vec::with_capacity(listen_count);
    for _ in 0..listen_count {
        let sock = create_async_udp_socket(addr)?;
        listens.push(sock);
    }
    Ok(listens)
}

/// Return the number of UDP sockets to create.
///
/// On Unix: equal to the number of logical CPUs, because `SO_REUSEPORT`
/// distributes packets across all bound sockets.
///
/// On Windows: always 1, because `SO_REUSEPORT` is not available and multiple
/// sockets bound to the same port would conflict.
#[cfg(not(target_os = "windows"))]
fn get_cpu_count() -> usize {
    num_cpus::get()
}

#[cfg(target_os = "windows")]
fn get_cpu_count() -> usize {
    1
}
