use log::LevelFilter;
use udp_server::prelude::UdpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Trace)
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
