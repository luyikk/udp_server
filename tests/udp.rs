#![feature(async_closure)]
use udp_server::{Error, UdpServer};
use std::cell::RefCell;
use tokio::net::UdpSocket;
use futures::executor::block_on;
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
#[should_panic]
fn test_error() {
    fn child_error() -> Result<(), Error> {
        Err(Error::Logic("test error".to_string()))
    }
    let x = child_error();

    if let Err(e) = x {
        println!("{}", e);
        assert!(false)
    }
}

#[tokio::test]
async fn test_udp_server(){

    let mut a = UdpServer::new_inner("0.0.0.0:5555", Arc::new(Mutex::new(0))).await.unwrap();

    a.set_err_input(|peer,err|{
        match peer {
            Some(peer)=>{
                block_on(async move{
                    println!("{:?}-{}",peer.lock().await,err);
                });
            },
            None=>  println!("{}",err)
        }

        true
    });

    a.set_input(async move |inner,peer,data|{
        let mut un_peer = peer.lock().await;
        match &un_peer.token {
            Some(x)=>{
                *x.borrow_mut()+=1;
                match inner.upgrade() {
                    Some(inner)=> {
                       let v= block_on(async move {
                            let mut inner = inner.lock().await;
                            *inner +=1;
                            println!("inner:{}",inner);
                            return inner.clone();
                        });

                        if v ==1000{
                            return Err("stop into".into());
                        }
                    },
                    None=>{}
                }
            },
            None=> {
                un_peer.token = Some(RefCell::new(1));
                if let Some(inner) = inner.upgrade() {
                    block_on(async move {
                        let mut inner = inner.lock().await;
                        *inner += 1;
                        println!("inner:{}", inner);
                    });
                }
            }
        }

        un_peer.send(&data).await?;

        Ok(())
    });

    let ph= a.start();


    let mut sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender.connect("127.0.0.1:5555").await.unwrap();
    let message = b"hello!";
    for _ in 0..1000 {
        sender.send(message).await.unwrap();
    }

    ph.await.unwrap();

    let mut recv_buf = [0u8; 32];
    let len= sender.recv(&mut recv_buf[..]).await.unwrap();

    assert_eq!(len,message.len());


}