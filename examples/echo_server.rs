use log::LevelFilter;
use udp_server::prelude::{IUdpPeer, UdpServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .init();
    UdpServer::new("0.0.0.0:20001", |peer, data, _| async move {
        peer.send(&data).await?;
        Ok(())
    })?
    .start(())
    .await?;

    Ok(())
}
