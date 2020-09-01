# udp_server
fast rust udp server

## Examples echo
```rust
#![feature(async_closure)]
use udp_server::UdpServer;
use std::error::Error;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    let mut a = UdpServer::<_,_,i32,_>::new("0.0.0.0:5555").await?;
    a.set_input(async move |_,peer,data|{
        peer.send(&data).await?;
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
use std::error::Error;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    let mut a = UdpServer::new("0.0.0.0:5555").await?;
    a.set_input(async move |_,peer,data|{
        let mut token = peer.token.lock().await;
        match token.get() {
            Some(x)=>{
                *x+=1;
            },
            None=>{
                token.set(Some(1));
            }
        }
        peer.send(&data).await?;
        Ok(())
    });

    a.start().await?;
    Ok(())
}
```
