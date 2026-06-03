# udp_server

[![Latest Version](https://img.shields.io/crates/v/udp_server.svg)](https://crates.io/crates/udp_server)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/udp_server)
[![Rust CI](https://github.com/luyikk/udp_server/actions/workflows/rust.yml/badge.svg)](https://github.com/luyikk/udp_server/actions/workflows/rust.yml)

**[English](#english) | [中文](#chinese)**

---

<a name="english"></a>
# udp_server

High-performance async UDP server framework built on [Tokio](https://tokio.rs/). Each client address is treated as an independent "peer" with its own channel — write a single async handler, and the framework manages sockets, peer lifecycle, and timeout cleanup for you.

## Features

- **Multi-socket parallelism** — binds one socket per CPU core via `SO_REUSEPORT` on Unix, kernel distributes packets across tasks.
- **Per-address peers** — each remote `SocketAddr` gets its own `UdpPeer` and dedicated handler task.
- **Automatic peer expiry** — optional timeout evicts idle peers after configurable seconds of inactivity.
- **Large socket buffers** — send/recv buffers sized at ~17 MB for bursty workloads.

## Installation

```toml
[dependencies]
udp_server = "1"
```

## Quick Start

### Echo Server

```rust
use log::LevelFilter;
use udp_server::prelude::UdpServer;

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

### With Shared State

The handler's third generic parameter `T` is user-defined shared state, cloned for each peer task:

```rust
use std::sync::Arc;
use udp_server::prelude::UdpServer;

struct MyState { prefix: String }

impl MyState {
    fn process(&self, data: &[u8]) -> Vec<u8> {
        let mut reply = self.prefix.as_bytes().to_vec();
        reply.extend_from_slice(data);
        reply
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let state = Arc::new(MyState { prefix: "echo: ".into() });

    UdpServer::new("0.0.0.0:20001", |peer, mut reader, state| async move {
        while let Some(Ok(data)) = reader.recv().await {
            peer.send(&state.process(&data)).await?;
        }
        Ok(())
    })?
    .start(state)
    .await?;

    Ok(())
}
```

## API

### `UdpServer::new(addr, handler)`

Binds to `addr` and creates the server. Handler signature: `Fn(UDPPeer, UdpReader, T) -> Future<Output = Result<(), Box<dyn Error>>>`.

| Type | Description |
| --- | --- |
| **`UDPPeer`** (`Arc<UdpPeer>`) | Handle to the remote client. `send(&[u8])` sends data, `get_addr()` returns the address, `close()` force-closes the channel. |
| **`UdpReader`** (`UnboundedReceiver<io::Result<Vec<u8>>>`) | Receives packets from this peer. Returns `Err(TimedOut)` when evicted by the timeout checker. |

The handler runs in its own Tokio task per peer. When the handler future completes or errors, the peer is automatically removed.

### Builder

| Method | Description |
| --- | --- |
| `.set_peer_timeout_sec(n)` | Evict peers idle for more than `n` seconds. A background task checks every 1s. |
| `.start(inner)` | Start the server. Returns when the server shuts down. |

## Running the Examples

```bash
cargo run --example echo_server                          # Start echo server
cargo run --example echo_client -- --addr 127.0.0.1:20001 --task 50  # Benchmark
```

## How It Works

1. On startup, `N` UDP sockets are bound to the same address (Unix `N = num_cpus`, Windows `N = 1`).
2. Each socket runs a dedicated `recv_from` loop. When a packet arrives from a new address, a `UdpPeer` is created and the user's handler is spawned.
3. Data is pushed through an unbounded channel; the handler reads via `reader.recv().await`.
4. If timeout is enabled, a background task scans `last_read_time` every second and sends `Err(TimedOut)` through the channel of stale peers.

## License

Licensed under either of MIT or Apache-2.0 at your option.

---

<a name="chinese"></a>
# udp_server

高性能异步 UDP 服务端框架，基于 [Tokio](https://tokio.rs/) 构建。每个客户端地址被视为一个独立的 "peer" 并拥有自己的 channel —— 只需编写一个 async handler，框架负责 socket 管理、peer 生命周期和超时清理。

## 特性

- **多 socket 并行** — 利用 `SO_REUSEPORT`（Unix）绑定与 CPU 核数相同数量的 socket，由内核负载均衡分发数据包。
- **按地址分 peer** — 每个远端 `SocketAddr` 独立一个 `UdpPeer` 和专属 handler 任务。
- **自动超时清理** — 可选 peer 超时，后台定时扫描并关闭空闲 peer。
- **大缓冲区** — 收发缓冲区默认约 17 MB，应对突发流量。

## 安装

```toml
[dependencies]
udp_server = "1"
```

## 快速上手

### Echo 服务端

```rust
use log::LevelFilter;
use udp_server::prelude::UdpServer;

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

### 带共享状态

handler 的第三个泛型参数 `T` 是用户自定义的共享状态，每个 peer 任务启动时会 clone 一份：

```rust
use std::sync::Arc;
use udp_server::prelude::UdpServer;

struct MyState { prefix: String }

impl MyState {
    fn process(&self, data: &[u8]) -> Vec<u8> {
        let mut reply = self.prefix.as_bytes().to_vec();
        reply.extend_from_slice(data);
        reply
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let state = Arc::new(MyState { prefix: "echo: ".into() });

    UdpServer::new("0.0.0.0:20001", |peer, mut reader, state| async move {
        while let Some(Ok(data)) = reader.recv().await {
            peer.send(&state.process(&data)).await?;
        }
        Ok(())
    })?
    .start(state)
    .await?;

    Ok(())
}
```

## API

### `UdpServer::new(addr, handler)`

绑定地址并创建服务端。handler 签名为 `Fn(UDPPeer, UdpReader, T) -> Future<Output = Result<(), Box<dyn Error>>>`。

| 类型 | 说明 |
| --- | --- |
| **`UDPPeer`** (`Arc<UdpPeer>`) | 远端 peer 句柄。`send(&[u8])` 发送数据、`get_addr()` 获取地址、`close()` 强制关闭。 |
| **`UdpReader`** (`UnboundedReceiver<io::Result<Vec<u8>>>`) | 接收该 peer 发来的数据。超时被驱逐时会收到 `Err(TimedOut)`。 |

handler 在独立 Tokio 任务中运行，future 完成或出错时自动清理 peer。

### Builder

| 方法 | 说明 |
| --- | --- |
| `.set_peer_timeout_sec(n)` | 空闲超过 `n` 秒的 peer 将被驱逐，后台每秒检查一次。 |
| `.start(inner)` | 启动服务端，服务端关闭时返回。 |

## 运行示例

```bash
cargo run --example echo_server                          # 启动 echo 服务端
cargo run --example echo_client -- --addr 127.0.0.1:20001 --task 50  # 压测
```

## 工作原理

1. 启动时绑定 `N` 个 UDP socket 到同一地址（Unix `N = num_cpus`，Windows `N = 1`）。
2. 每个 socket 独立循环 `recv_from`，新地址到达时创建 `UdpPeer` 并 spawn handler。
3. 数据通过 unbounded channel 推送给 handler，handler 用 `reader.recv().await` 消费。
4. 若开启超时，后台任务每秒扫描 `last_read_time`，向超时 peer 的 channel 发送 `Err(TimedOut)`。

## License

MIT / Apache-2.0 双协议授权。