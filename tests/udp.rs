#![feature(async_closure)]
use udp_server::{Error, UdpServer};
use std::cell::RefCell;
use tokio::net::UdpSocket;

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
    let mut a = UdpServer::new("0.0.0.0:5555").await.unwrap();

    a.set_err_input(|peer,err|{
        match peer {
            Some(peer)=>{
                let msg=format!("{}",err);
                tokio::spawn(async move{
                    println!("{:?}-{}",peer.lock().await,msg);
                });
            },
            None=>  println!("{}",err)
        }

        true
    });

    a.set_input(async move |peer,data|{
        let mut un_peer = peer.lock().await;
        match &un_peer.token {
            Some(x)=>{
                *x.borrow_mut()+=1;

            },
            None=>{
                un_peer.token=Some(RefCell::new(1));
            }
        }

        un_peer.send(&data).await?;
        Err("test".into())
    });

    let ph= a.start();


    let mut sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender.connect("127.0.0.1:5555").await.unwrap();
    let message = b"hello!";
    for _ in 0..100 {
        sender.send(message).await.unwrap();
    }

    ph.await.unwrap();

    let mut recv_buf = [0u8; 32];
    let len= sender.recv(&mut recv_buf[..]).await.unwrap();

    assert_eq!(len,message.len());


}