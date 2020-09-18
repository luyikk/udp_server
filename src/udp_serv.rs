use crate::error;
use net2::{UdpBuilder, UdpSocketExt};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::future::Future;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use tokio::net::udp::{RecvHalf, SendHalf};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::{delay_for, Duration};



#[cfg(not(target_os = "windows"))]
use net2::unix::UnixUdpBuilderExt;


/// UDP 单个包最大大小,默认4096 主要看MTU一般不会超过1500在internet 上
/// 如果局域网有可能大点,4096一般够用.
pub const BUFF_MAX_SIZE: usize = 4096;


/// UDP SOCKET 上下文,包含 id, 和Pees, 用于收发数据
pub struct UdpContext<T: Send> {
    pub id: usize,
    recv: Arc<Mutex<RecvHalf>>,
    pub send: Arc<Mutex<SendHalf>>,
    pub peers: Arc<Mutex<HashMap<SocketAddr, Arc<Peer<T>>>>>,
}

/// 错误输入类型
pub type ErrorInput<T>=Arc<Mutex<dyn Fn(Option<Arc<Peer<T>>>, Box<dyn Error>)->bool + Send>>;

/// UDP 服务器对象
/// I 用来限制必须input的FN 原型,
/// R 用来限制 必须input的是 异步函数
/// T 用来设置返回值
///
/// # Examples
/// ```
/// #![feature(async_closure)]
/// use udp_server::UdpServer;
/// use tokio::net::UdpSocket;
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// #[tokio::main]
/// async fn main() {
///    let mut a = UdpServer::new("127.0.0.1:5555").await.unwrap();
///    a.set_input(async move |_,peer,data|{
///         let mut token = peer.token.lock().await;
///         match token.get() {
///             Some(x)=>{
///                 *x+=1;
///                 }
///             None=>{
///                 token.set(Some(1));
///             }
///         }
///         peer.send(&data).await?;
///         Err("stop it".into())
///     });
///
///  let mut sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
///  sender.connect("127.0.0.1:5555").await.unwrap();
///  let message = b"hello!";
///  for _ in 0..100 {
///     sender.send(message).await.unwrap();
///  }
///
///  a.start().await.unwrap();
/// }
///
///
/// ```
pub struct UdpServer<I, R, T,S>
    where
        I: Fn(Arc<S>,Arc<Peer<T>>, Vec<u8>) -> R + Send + Sync + 'static,
        R: Future<Output = Result<(), Box<dyn Error>>>,
        T: Send + 'static,
        S: Sync +Send+'static
{
    inner:Arc<S>,
    udp_contexts: Vec<UdpContext<T>>,
    input: Option<Arc<I>>,
    error_input: Option<ErrorInput<T>>,
}



/// 用来存储Token
#[derive(Debug)]
pub struct TokenStore<T:Send>(pub Option<T>);

impl<T:Send> TokenStore<T>{
    pub fn have(&self)->bool{
        self.0.is_some()
    }

    pub fn get(&mut self)->Option<&mut T> {
        self.0.as_mut()
    }

    pub fn set(&mut self,v:Option<T>) {
        self.0 = v;
    }
}


/// 用来存储SendHalf 并实现Write
#[derive(Debug)]
pub struct UdpSend(pub Arc<Mutex<SendHalf>>,pub SocketAddr);

impl UdpSend{
    pub async fn send(&self,buf: &[u8])->std::io::Result<usize> {
        self.0.lock().await.send_to(buf,&self.1).await
    }
}


/// Peer 对象
/// 用来标识client
/// socket_id 标识 所在哪个UDPContent,一般只在头一次接收到数据的时候设置
/// 一般用来所在Peer位置,linux 服务器上启用了 reuse_port
/// 所以后续收到数据包不一定来自于此 所在哪个UDPContent
/// 而windows上只有一个socket,所以始终只有1个
/// # token
/// 你可以自定义放一些和用户有关的数据,这样的话方便你对用户进行区别,已提取用户逻辑数据
#[derive(Debug)]
pub struct Peer<T: Send> {
    pub socket_id: usize,
    pub addr: SocketAddr,
    pub token: Arc<Mutex<TokenStore<T>>>,
    pub udp_sock: Arc<UdpSend>,
}


impl<T: Send> Peer<T> {
    /// Send 发送数据包
    /// 作为最基本的函数之一,它采用了tokio的async send_to
    /// 首先,他会去弱指针里面拿到强指针,如果没有他会爆错
    pub async fn send(&self, data: &[u8]) -> Result<usize, std::io::Error> {
        self.udp_sock.send(data).await
    }
}

impl <I,R,T> UdpServer<I,R,T,()>  where
    I: Fn(Arc<()>,Arc<Peer<T>>, Vec<u8>) -> R + Send + Sync + 'static,
    R: Future<Output = Result<(), Box<dyn Error>>> + Send,
    T: Send + 'static{

    pub async fn new<A: ToSocketAddrs>(addr:A)->Result<Self, Box<dyn Error>> {
        Self::new_inner(addr,Arc::new(())).await
    }
}

impl<I, R, T, S> UdpServer<I, R, T, S>
    where
        I: Fn(Arc<S>,Arc<Peer<T>>, Vec<u8>) -> R + Send + Sync + 'static,
        R: Future<Output = Result<(), Box<dyn Error>>> + Send,
        T: Send + 'static,
        S: Sync +Send + 'static{
    ///用于非windows 创建socket,和windows的区别在于开启了 reuse_port
    #[cfg(not(target_os = "windows"))]
    fn make_udp_client<A: ToSocketAddrs>(addr: &A) -> Result<std::net::UdpSocket, Box<dyn Error>> {
        let res = UdpBuilder::new_v4()?
            .reuse_address(true)?
            .reuse_port(true)?
            .bind(addr)?;
        Ok(res)
    }



    ///用于windows创建socket
    #[cfg(target_os = "windows")]
    fn make_udp_client<A: ToSocketAddrs>(addr: &A) -> Result<std::net::UdpSocket, Box<dyn Error>> {
        let res = UdpBuilder::new_v4()?.reuse_address(true)?.bind(addr)?;
        Ok(res)
    }

    ///创建udp socket,并设置buffer 大小
    fn create_udp_socket<A: ToSocketAddrs>(
        addr: &A,
    ) -> Result<std::net::UdpSocket, Box<dyn Error>> {
        let res = Self::make_udp_client(addr)?;
        res.set_send_buffer_size(1784 * 10000)?;
        res.set_recv_buffer_size(1784 * 10000)?;
        Ok(res)
    }

    /// 创建tokio的udpsocket ,从std 创建
    fn create_async_udp_socket<A: ToSocketAddrs>(addr: &A) -> Result<UdpSocket, Box<dyn Error>> {
        let std_sock = Self::create_udp_socket(&addr)?;
        let sock = UdpSocket::try_from(std_sock)?;
        Ok(sock)
    }

    /// 创建tikio UDPClient
    /// listen_count 表示需要监听多少份的UDP SOCKET
    fn create_udp_socket_list<A: ToSocketAddrs>(
        addr: &A,
        listen_count: usize,
    ) -> Result<Vec<UdpSocket>, Box<dyn Error>> {
        println!("cpus:{}", listen_count);
        let mut listens = vec![];
        for _ in 0..listen_count {
            let sock = Self::create_async_udp_socket(addr)?;
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

    /// 创建UdpServer
    /// 如果是linux 是系统,他会根据CPU核心数创建等比的UDP SOCKET 监听同一端口
    /// 已达到 M级的DPS 数量
    pub async fn new_inner<A: ToSocketAddrs>(addr: A, inner:Arc<S>) -> Result<Self, Box<dyn Error>> {
        let udp_list = Self::create_udp_socket_list(&addr, Self::get_cpu_count())?;

        let mut udp_map = vec![];

        let mut id = 1;
        for udp in udp_list {
            let (recv, send) = udp.split();
            udp_map.push(UdpContext {
                id,
                recv: Arc::new(Mutex::new(recv)),
                send: Arc::new(Mutex::new( send)),
                peers: Arc::new(Mutex::new(HashMap::new())),
            });
            id += 1;
        }

        Ok(UdpServer {
            inner,
            udp_contexts: udp_map,
            input: None,
            error_input: None,
        })
    }



    /// 设置收包函数
    /// 此函数必须符合 async 模式
    pub fn set_input(&mut self, input: I) {
        self.input = Some(Arc::new(input));
    }

    /// 设置错误输出
    /// 返回bool 如果 true　表示停止服务
    pub fn set_err_input<P: Fn(Option<Arc<Peer<T>>>, Box<dyn Error>)->bool + Send + 'static>(&mut self, err_input: P) {
        self.error_input = Some(Arc::new(Mutex::new(err_input)));
    }

    /// 根据地址删除peer
    pub fn remove_peer(&self,addr:SocketAddr)->bool{
        for udp_server in  self.udp_contexts.iter() {
            let mut res= udp_server.peers.try_lock();
            if let Ok(ref mut peer_dict)=res {
                return peer_dict.remove(&addr).is_some();
            }
        }
        false
    }

    /// 启动服务
    /// 如果input 发生异常,将会发生错误
    /// 这个时候回触发 err_input, 如果没有使用 set_err_input 设置错误回调
    /// 那么 就会输出默认的 err_input,如果输出默认的 err_input 那么整个服务将会停止
    /// 所以如果不想服务停止,那么必须自己实现 err_input 并且返回 false
    pub async fn start(&self) -> Result<(), Box<dyn Error>> {
        if let Some(input) = &self.input {
            let mut tasks = vec![];
            for udp_sock in self.udp_contexts.iter() {
                let recv_sock = Arc::downgrade(&udp_sock.recv);
                let send_sock = udp_sock.send.clone();
                let input=input.clone();
                let id = udp_sock.id;
                let pees_ptr = udp_sock.peers.clone();
                let inner= self.inner.clone();
                let err_input = {
                    if let Some(err) = &self.error_input {
                        let x = err;
                        x.clone()
                    } else {
                        Arc::new(Mutex::new(|peer:Option<Arc<Peer<T>>>, err:Box<dyn Error>| {
                            match peer {
                                Some(peer) => {
                                    println!("{}-{}", peer.addr, err);
                                }
                                None => {
                                    println!("{}", err);
                                }
                            }
                            true
                        }))
                    }
                };

                let pd = tokio::spawn(async move {
                    let wk = recv_sock.upgrade();
                    if let Some(sock_mutex) = wk {
                        let mut buff = [0; BUFF_MAX_SIZE];
                        loop {
                            let res = {
                                let mut sock = sock_mutex.lock().await;
                                sock.recv_from(&mut buff).await
                            };

                            if let Ok((size, addr)) = res {
                                let peer = {
                                    let mut lock_pees = pees_ptr.lock().await;
                                    let res = lock_pees.entry(addr).or_insert_with(|| {
                                        Arc::new(Peer {
                                            socket_id: id,
                                            addr,
                                            token: Arc::new(Mutex::new(TokenStore(None))),
                                            udp_sock: Arc::new(UdpSend(send_sock.clone(), addr))
                                        })
                                    });
                                    res.clone()
                                };


                                let err = {
                                    let res =
                                        input(inner.clone(), peer.clone(), buff[0..size].to_vec()).await;

                                    match res {
                                        Err(er) => Some(format!("{}", er)),
                                        Ok(()) => None,
                                    }
                                };

                                if let Some(err_msg) = err {
                                    let error = err_input.lock().await;
                                    let stop = error(
                                        Some(peer),
                                        err_msg.into(),
                                    );
                                    if stop {
                                        return;
                                    }
                                }

                            } else if let Err(er) = res {
                                let error = err_input.lock().await;
                                let stop= error(None, error::Error::IOError(er).into());
                                if stop{
                                    return;
                                }
                            }
                        }
                    } else {
                        delay_for(Duration::from_millis(1)).await;
                    }
                });
                tasks.push(pd);
            }

            for task in tasks {
                task.await?;
            }

            Ok(())
        } else {
            panic!("not found input")
        }
    }
}
