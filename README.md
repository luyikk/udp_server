# udp_server
fast rust udp server

## Examples echo
```rust
#![feature(async_closure)]
use udp_server::UdpServer;
use std::cell::RefCell;
use std::error::Error;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    let mut a = UdpServer::<_,_,i32,_>::new("0.0.0.0:5555").await?;
    a.set_input(async move |_,peer,data|{
        let mut un_peer = peer.lock().await;     
        un_peer.send(&data).await?;
        Ok(())
    });

    a.start().await?;
    Ok(())
}
```

#if you need to use token
## Examples token echo
```rust
#![feature(async_closure)]
use udp_server::UdpServer;
use std::cell::RefCell;
use std::error::Error;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    let mut a = UdpServer::<_,_,i32,_>::new("0.0.0.0:5555").await?;
    a.set_input(async move |_,peer,data|{
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
        Ok(())
    });

    a.start().await?;
    Ok(())
}
```
