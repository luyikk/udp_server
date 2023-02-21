# udp_server
fast rust udp server

[![Latest Version](https://img.shields.io/crates/v/udp_server.svg)](https://crates.io/crates/udp_server)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/udp_server)
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/luyikk/udp_server)](https://rust-reportcard.xuri.me/report/github.com/luyikk/udp_server)
[![Rust CI](https://github.com/luyikk/udp_server/actions/workflows/rust.yml/badge.svg)](https://github.com/luyikk/udp_server/actions/workflows/rust.yml)


## Examples echo
```rust
use log::LevelFilter;
use udp_server::prelude::{IUdpPeer, UdpServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .init();
    UdpServer::new("0.0.0.0:20001", |peer, mut reader, _| async move {
        while let Some(Ok(data)) = reader.recv().await {
            peer.send(&data).await?;
        }
        Ok(())
    })?
        .set_peer_timeout_sec(20)
        .start(())
        .await?;

    Ok(())
}
```
