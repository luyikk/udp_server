use anyhow::Context;
use log::LevelFilter;
use udp_server::prelude::{IUdpPeer, UdpServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .init();
    UdpServer::new("0.0.0.0:20001", |peer, _| async move {
        let mut reader = peer.get_reader().await.context("not reader")?;
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
