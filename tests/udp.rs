#![feature(async_closure)]
use udp_server::{Error, UdpServer};
use tokio::net::UdpSocket;
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
async fn test_udp_inner_server(){

    let mut a = UdpServer::new_inner("127.0.0.1:5555", Arc::new(Mutex::new(0))).await.unwrap();

    a.set_err_input(|peer,err|{
        match peer {
            Some(peer)=> {
                println!("{:?}-{}", peer, err);
            },
            None=>  println!("{}",err)
        }

        true
    });

    a.set_input(async move |inner,peer,data|{
        let mut token = peer.token.lock().await;

        if !token.have(){
            token.set(1);
            if let Some(inner)=inner.upgrade(){
                let mut inner = inner.lock().await;
                *inner += 1;
                println!("inner:{}", inner);
            }
        }
        else{
            let value=token.get().unwrap();
            *value+=1;
            match inner.upgrade() {
                Some(inner) => {

                    let mut inner = inner.lock().await;
                    *inner += 1;
                    println!("inner:{}", inner);
                    if  *inner == 1000 {
                        return Err("stop".into());
                    }
                },
                None => {}
            }
        }
        peer.send(&data).await?;

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



#[tokio::test]
async fn test_udp_new_server(){

    let mut a = UdpServer::new("127.0.0.1:6666").await.unwrap();

    a.set_err_input(|peer,err|{
        match peer {
            Some(peer)=>{
                    println!("{:?}-{}",peer,err);
            },
            None=>  println!("{}",err)
        }

        true
    });

    a.set_input(async move |_,peer,data|{
        let mut token = peer.token.lock().await;
        match token.get() {
            Some(x)=>{
                *x+=1;
                if *x >=100{
                    return Err("stop".into());
                }
            },
            None=> {
                token.set(0);
            }
        }

        peer.send(&data).await?;

        Ok(())
    });

    let ph= a.start();


    let mut sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender.connect("127.0.0.1:6666").await.unwrap();
    let message = b"hello!";

    for _ in 0..1000 {
        sender.send(message).await.unwrap();
    }

    ph.await.unwrap();

    let mut recv_buf = [0u8; 32];
    let len= sender.recv(&mut recv_buf[..]).await.unwrap();

    assert_eq!(len,message.len());


}