use aqueue::Actor;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

pub type UDPPeer = Arc<Actor<UdpPeer>>;

/// UDP Peer
/// each address+port is equal to one UDP peer
pub struct UdpPeer {
    pub socket_id: usize,
    pub udp_sock: Arc<UdpSocket>,
    pub addr: SocketAddr,
}

unsafe impl Sync for UdpPeer {}

impl UdpPeer {
    #[inline]
    pub fn new(socket_id: usize, udp_sock: Arc<UdpSocket>, addr: SocketAddr) -> UDPPeer {
        Arc::new(Actor::new(Self {
            socket_id,
            udp_sock,
            addr,
        }))
    }

    /// send buf to peer
    #[inline]
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.udp_sock.send_to(buf, &self.addr).await
    }
}

#[async_trait::async_trait]
pub trait IUdpPeer {
    /// send buf to peer
    async fn send(&self, buf: &[u8]) -> io::Result<usize>;
}

#[async_trait::async_trait]
impl IUdpPeer for Actor<UdpPeer> {
    #[inline]
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.inner_call(|inner| async move { inner.get().send(buf).await })
            .await
    }
}
