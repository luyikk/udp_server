# udp_server
fast rust udp server

## Examples echo

```rust
#![feature(async_closure)]
use udp_server::UdpServer;
use std::cell::RefCell;

#[tokio::main]
async fn main()->Result<(),Box<dyn Error>> {
    let mut a = UdpServer::new("0.0.0.0:5555").await?;
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
            OK(())
        });    

    a.start().await?;
    Ok(())
 }
```
