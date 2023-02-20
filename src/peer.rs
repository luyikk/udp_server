use aqueue::Actor;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub type UDPPeer = Arc<Actor<UdpPeer>>;

/// UDP Peer
/// each address+port is equal to one UDP peer
pub struct UdpPeer {
    pub socket_id: usize,
    pub udp_sock: Arc<UdpSocket>,
    pub addr: SocketAddr,
    tx: UnboundedSender<io::Result<Vec<u8>>>,
    rx: Option<UnboundedReceiver<io::Result<Vec<u8>>>>,
    last_read_time: Instant,
}

impl Drop for UdpPeer {
    fn drop(&mut self) {
        log::debug!(
            "udp_listen socket:{} udp peer:{} drop",
            self.socket_id,
            self.addr
        )
    }
}

unsafe impl Sync for UdpPeer {}

impl UdpPeer {
    #[inline]
    pub fn new(socket_id: usize, udp_sock: Arc<UdpSocket>, addr: SocketAddr) -> UDPPeer {
        let (tx, rx) = unbounded_channel();
        Arc::new(Actor::new(Self {
            socket_id,
            udp_sock,
            addr,
            tx,
            rx: Some(rx),
            last_read_time: Instant::now(),
        }))
    }

    /// send buf to peer
    #[inline]
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.udp_sock.send_to(buf, &self.addr).await
    }

    /// get read rx
    #[inline]
    fn get_read(&mut self) -> Option<UnboundedReceiver<io::Result<Vec<u8>>>> {
        self.rx.take()
    }
}

#[async_trait::async_trait]
pub trait IUdpPeer {
    /// get addr
    fn get_addr(&self) -> SocketAddr;
    /// send buf to peer
    async fn send(&self, buf: &[u8]) -> io::Result<usize>;
    /// get read rx
    async fn get_reader(&self) -> Option<UnboundedReceiver<io::Result<Vec<u8>>>>;
    /// close peer
    async fn close(&self);
}

#[async_trait::async_trait]
impl IUdpPeer for Actor<UdpPeer> {
    #[inline]
    fn get_addr(&self) -> SocketAddr {
        unsafe { self.deref_inner().addr }
    }

    #[inline]
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.inner_call(|inner| async move { inner.get().send(buf).await })
            .await
    }

    #[inline]
    async fn get_reader(&self) -> Option<UnboundedReceiver<io::Result<Vec<u8>>>> {
        self.inner_call(|inner| async move { inner.get_mut().get_read() })
            .await
    }

    #[inline]
    async fn close(&self) {
        unsafe {
            if let Err(err) = self
                .deref_inner()
                .tx
                .send(Err(io::Error::new(ErrorKind::BrokenPipe, "peer is close")))
            {
                log::error!("close peer:{} error:{err}", self.get_addr());
            }

            self.deref_inner().tx.closed().await;
        }
    }
}

#[async_trait::async_trait]
pub(crate) trait IUdpPeerPushData {
    /// get last recv sec
    fn get_last_recv_sec(&self) -> u64;
    /// get socket id
    fn get_socket_id(&self) -> usize;
    /// push data to read tx
    async fn push_data(&self, buf: Vec<u8>) -> io::Result<()>;
}

#[async_trait::async_trait]
impl IUdpPeerPushData for Actor<UdpPeer> {
    #[inline]
    fn get_last_recv_sec(&self) -> u64 {
        unsafe { self.deref_inner().last_read_time.elapsed().as_secs() }
    }

    #[inline]
    fn get_socket_id(&self) -> usize {
        unsafe { self.deref_inner().socket_id }
    }

    #[inline]
    async fn push_data(&self, buf: Vec<u8>) -> io::Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().last_read_time = Instant::now();
            if let Err(err) = inner.get().tx.send(Ok(buf)) {
                Err(io::Error::new(ErrorKind::Other, err))
            } else {
                Ok(())
            }
        })
        .await
    }
}
