use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::future::Future;
use std::io;
use std::marker::PhantomData;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use crate::peer::{UDPPeer, UdpPeer};
use async_lock::Mutex;
use net2::{UdpBuilder, UdpSocketExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::unbounded_channel;

///The maximum size of a single UDP packet is 4096 by default. The MTU is generally not more than 1500 on the Internet
///If the LAN is likely to be larger, 4096 is generally enough
pub const BUFF_MAX_SIZE: usize = 4096;

/// UDP Context
/// each bind will create a
pub struct UdpContext {
    pub id: usize,
    recv: Arc<UdpSocket>,
    pub peers: Mutex<HashMap<SocketAddr, UDPPeer>>,
}

/// UDP Server listen
pub struct UdpServer<I, T> {
    udp_contexts: Vec<UdpContext>,
    input: I,
    _ph: PhantomData<T>,
}

impl<I, R, T> UdpServer<I, T>
where
    I: Fn(UDPPeer, Vec<u8>, &T) -> R + Send + Sync + 'static,
    R: Future<Output = Result<(), Box<dyn Error>>> + Send + 'static,
{
    pub fn new<A: ToSocketAddrs>(addr: A, input: I) -> io::Result<Self> {
        let udp_list = create_udp_socket_list(&addr, get_cpu_count())?;
        let udp_contexts = udp_list
            .into_iter()
            .enumerate()
            .map(|(id, socket)| UdpContext {
                id,
                recv: Arc::new(socket),
                peers: Default::default(),
            })
            .collect();
        Ok(UdpServer {
            udp_contexts,
            input,
            _ph: Default::default(),
        })
    }

    /// remove peer clean memory
    #[inline]
    pub async fn remove_peer(&self, addr: SocketAddr) -> bool {
        for udp_server in self.udp_contexts.iter() {
            if udp_server.peers.lock().await.remove(&addr).is_some() {
                return true;
            }
        }
        false
    }

    #[inline]
    pub async fn start(&self, inner: T) -> io::Result<()> {
        let (tx, mut rx) = unbounded_channel();
        for (index, udp_listen) in self.udp_contexts.iter().enumerate() {
            let send_data_tx = tx.clone();
            let udp_socket = udp_listen.recv.clone();
            tokio::spawn(async move {
                log::debug!("start udp listen:{index}");
                let mut buff = [0; BUFF_MAX_SIZE];
                loop {
                    match udp_socket.recv_from(&mut buff).await {
                        Ok((size, addr)) => {
                            if let Err(err) =
                                send_data_tx.send((index, addr, buff[..size].to_vec()))
                            {
                                log::error!("send_data_tx is error:{err}");
                                break;
                            }
                        }
                        Err(err) => {
                            log::error!("udp:{index} recv_from error:{err}");
                        }
                    }
                }
            });
        }
        drop(tx);
        while let Some((index, addr, data)) = rx.recv().await {
            let peer = {
                let context = self.udp_contexts.get(index).unwrap();
                context
                    .peers
                    .lock()
                    .await
                    .entry(addr)
                    .or_insert_with(|| UdpPeer::new(index, context.recv.clone(), addr))
                    .clone()
            };

            if let Err(err) = (self.input)(peer, data, &inner).await {
                log::error!("udp input error:{err}")
            }
        }
        Ok(())
    }
}

///Create udp socket for windows
#[cfg(target_os = "windows")]
fn make_udp_client<A: ToSocketAddrs>(addr: &A) -> io::Result<std::net::UdpSocket> {
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
    };
    let addr: SocketAddr = addr?;
    if addr.is_ipv4() {
        Ok(UdpBuilder::new_v4()?.reuse_address(true)?.bind(addr)?)
    } else if addr.is_ipv6() {
        Ok(UdpBuilder::new_v6()?.reuse_address(true)?.bind(addr)?)
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "not address AF_INET"))
    }
}

///It is used to create udp sockets for non-windows. The difference from windows is that reuse_port
#[cfg(not(target_os = "windows"))]
fn make_udp_client<A: ToSocketAddrs>(addr: &A) -> io::Result<std::net::UdpSocket> {
    use net2::unix::UnixUdpBuilderExt;
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
    };
    let addr: SocketAddr = addr?;
    if addr.is_ipv4() {
        Ok(UdpBuilder::new_v4()?
            .reuse_address(true)?
            .reuse_port(true)?
            .bind(addr)?)
    } else if addr.is_ipv6() {
        Ok(UdpBuilder::new_v6()?
            .reuse_address(true)?
            .reuse_port(true)?
            .bind(addr)?)
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "not address AF_INET"))
    }
}

///Create a udp socket and set the buffer size
fn create_udp_socket<A: ToSocketAddrs>(addr: &A) -> io::Result<std::net::UdpSocket> {
    let res = make_udp_client(addr)?;
    res.set_send_buffer_size(1784 * 10000)?;
    res.set_recv_buffer_size(1784 * 10000)?;
    Ok(res)
}

/// From std socket create tokio udp socket
fn create_async_udp_socket<A: ToSocketAddrs>(addr: &A) -> io::Result<UdpSocket> {
    let std_sock = create_udp_socket(&addr)?;
    std_sock.set_nonblocking(true)?;
    let sock = UdpSocket::try_from(std_sock)?;
    Ok(sock)
}

/// create tokio UDP socket list
/// listen_count indicates how many UDP SOCKETS to listen
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

#[cfg(not(target_os = "windows"))]
fn get_cpu_count() -> usize {
    num_cpus::get()
}

#[cfg(target_os = "windows")]
fn get_cpu_count() -> usize {
    1
}
