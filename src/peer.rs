use aqueue::Actor;
use std::io;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub type UDPPeer = Arc<Actor<UdpPeer>>;

/// UDP Peer
/// each address+port is equal to one UDP peer
pub struct UdpPeer {
    pub socket_id: usize,
    pub udp_sock: Arc<UdpSocket>,
    pub addr: SocketAddr,
    tx: UnboundedSender<Vec<u8>>,
    rx: Option<UnboundedReceiver<Vec<u8>>>,
}

impl Drop for UdpPeer {
    fn drop(&mut self) {
        log::debug!("udp_listen socket:{} udp peer:{} drop",self.socket_id,self.addr)
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
        }))
    }

    /// send buf to peer
    #[inline]
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.udp_sock.send_to(buf, &self.addr).await
    }

    /// get read rx
    #[inline]
    fn get_read(&mut self) -> Option<UnboundedReceiver<Vec<u8>>> {
        self.rx.take()
    }
}

#[async_trait::async_trait]
pub trait IUdpPeer {
    /// get addr
    async fn get_addr(&self)->SocketAddr;
    /// send buf to peer
    async fn send(&self, buf: &[u8]) -> io::Result<usize>;
    /// get read rx
    async fn get_reader(&self) -> Option<UnboundedReceiver<Vec<u8>>>;

}

#[async_trait::async_trait]
impl IUdpPeer for Actor<UdpPeer> {
    #[inline]
    async fn get_addr(&self) -> SocketAddr {
       unsafe{
           self.deref_inner().addr
       }
    }

    #[inline]
    async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.inner_call(|inner| async move { inner.get().send(buf).await })
            .await
    }

    #[inline]
    async fn get_reader(&self) -> Option<UnboundedReceiver<Vec<u8>>> {
        self.inner_call(|inner| async move { inner.get_mut().get_read() })
            .await
    }

}


#[async_trait::async_trait]
pub(crate) trait IUdpPeerPushData{
    /// get socket id
    async fn get_socket_id(&self)->usize;
    /// push data to read tx
    async fn push_data(&self, buf: Vec<u8>) -> io::Result<()>;
}

#[async_trait::async_trait]
impl IUdpPeerPushData for Actor<UdpPeer> {
    #[inline]
    async fn get_socket_id(&self) -> usize {
        unsafe{
            self.deref_inner().socket_id
        }
    }

    #[inline]
    async fn push_data(&self, buf: Vec<u8>) -> io::Result<()> {
        unsafe {
            if let Err(err) = self.deref_inner().tx.send(buf) {
                Err(io::Error::new(ErrorKind::Other, err))
            } else {
                Ok(())
            }
        }
    }
}