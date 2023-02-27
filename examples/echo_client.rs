use clap::Parser;
use log::LevelFilter;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

static INC: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    env_logger::Builder::new()
        .filter_level(LevelFilter::Debug)
        .init();

    tokio::spawn(async move {
        loop {
            println!("TPS:{}", INC.swap(0, Ordering::AcqRel));
            tokio::time::sleep(Duration::from_secs(1)).await
        }
    });

    let joins = (0..opt.task)
        .map(|_| {
            let addr = opt.addr.clone();
            tokio::spawn(async move { run(&addr, 60).await })
        })
        .collect::<Vec<_>>();

    for join in joins {
        join.await??;
    }

    Ok(())
}

#[derive(Parser)]
#[clap(name = "test udp echo client")]
struct Opt {
    #[clap(short, long, value_parser)]
    addr: String,
    #[clap(short, long, value_parser, default_value_t = 50)]
    task: u32,
}

async fn run(addr: &str, time: u64) -> anyhow::Result<()> {
    let sender = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    sender.connect(addr).await?;

    let reader = sender.clone();
    tokio::spawn(async move {
        let mut recv_buf = [0u8; 4096];
        loop {
            match reader.recv(&mut recv_buf[..]).await {
                Ok(size) => {
                    INC.fetch_add(1, Ordering::Release);
                    assert_eq!(&recv_buf[..size], b"hello!");
                    reader.send(&recv_buf[..size]).await.unwrap();
                }
                Err(err) => {
                    log::error!("error:{}", err);
                    break;
                }
            }
        }
    });

    let message = b"hello!";
    sender.send(message).await?;

    tokio::time::sleep(Duration::from_secs(time)).await;
    Ok(())
}
