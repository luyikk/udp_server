# udp_server

[![Latest Version](https://img.shields.io/crates/v/udp_server.svg)](https://crates.io/crates/udp_server)
[![Rust Documentation](https://img.shields.io/badge/api-rustdoc-blue.svg)](https://docs.rs/udp_server)
[![Rust CI](https://github.com/luyikk/udp_server/actions/workflows/rust.yml/badge.svg)](https://github.com/luyikk/udp_server/actions/workflows/rust.yml)

**[English](#english) | [中文](#chinese)**

---

<a name="english"></a>
# udp_server

High-performance async UDP server framework built on [Tokio](https://tokio.rs/). Each client address is an independent "peer" with its own channel — write one async handler, and the framework handles sockets, peer lifecycle, and timeout cleanup.

## Features

- **Multi-socket parallelism** — binds one socket per CPU core via `SO_REUSEPORT` (Unix), kernel load-balances packets.
- **Per-address peers** — each remote `SocketAddr` gets a `UdpPeer` with a dedicated handler task.
- **Zero-copy data path** — received packets are shared as reference-counted `Bytes`; no data copying through the channel.
- **Lock-free peer map** — concurrent `DashMap` eliminates mutex contention between recv and cleanup paths.
- **Automatic peer expiry** — optional idle timeout (configurable per second).
- **Configurable socket buffers** — tune send/recv buffer size via the builder API.

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

The handler's third parameter `T` is user-defined state, cloned for each peer:

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

Creates the server (resolves `addr` immediately, defers socket creation to `start()`). Handler signature:

```
Fn(UDPPeer, UdpReader, T) -> Future<Output = Result<(), Box<dyn Error>>>
```

| Type | Description |
| --- | --- |
| **`UDPPeer`** (`Arc<UdpPeer>`) | Handle to the remote peer. `send(&[u8])` sends, `get_addr()` returns address, `close()` force-closes. |
| **`UdpReader`** (`UnboundedReceiver<io::Result<Bytes>>`) | Incoming packets as zero-copy `Bytes`. `Err(TimedOut)` signals peer eviction. |

The handler runs in its own Tokio task per peer; the peer is auto-removed when the handler completes or errors.

### Builder

| Method | Description |
| --- | --- |
| `.set_peer_timeout_sec(n)` | Evict peers idle for more than `n` seconds. |
| `.set_buffer_size(n)` | Set socket send/recv buffer size in bytes (default ~17.8 MB). |
| `.start(inner)` | Bind sockets and start the server. Blocks until shutdown. |

## Running the Examples

```bash
cargo run --example echo_server
cargo run --example echo_client -- --addr 127.0.0.1:20001 --task 50
```

## How It Works

1. `new()` resolves the address; `start()` creates `N` sockets bound to it (Unix `N = num_cpus`, Windows `N = 1`).
2. Each socket runs a dedicated `recv_from` loop. Packets from new addresses spawn a `UdpPeer` and the user's handler.
3. Data moves through an unbounded channel as `Bytes` (zero-copy); handlers read via `reader.recv().await`.
4. If timeout is configured, a background task scans every 1s and pushes `Err(TimedOut)` into stale peer channels.

## License

MIT or Apache-2.0, at your option.

---

<a name="chinese"></a>
# udp_server

高性能异步 UDP 服务端框架，基于 [Tokio](https://tokio.rs/) 构建。每个客户端地址被抽象为独立 "peer" 并拥有自己的 channel —— 只需编写一个 async handler，框架负责 socket 管理、peer 生命周期和超时清理。

## 特性

- **多 socket 并行** — 利用 `SO_REUSEPORT`（Unix）绑定与 CPU 核数相同数量的 socket，由内核负载均衡分发。
- **按地址分 peer** — 每个远端 `SocketAddr` 独立一个 `UdpPeer` 和专属 handler 任务。
- **零拷贝数据路径** — 接收的数据包以引用计数 `Bytes` 传递，channel 中无数据拷贝。
- **无锁 peer 表** — 并发 `DashMap` 消除了 recv 和清理路径间的互斥锁竞争。
- **自动超时清理** — 可选空闲超时，秒级可配。
- **可配置缓冲区** — 通过 builder API 调整 socket 收发缓冲区大小。

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

创建服务端（立即解析地址，延迟到 `start()` 才创建 socket）。handler 签名：

```
Fn(UDPPeer, UdpReader, T) -> Future<Output = Result<(), Box<dyn Error>>>
```

| 类型 | 说明 |
| --- | --- |
| **`UDPPeer`** (`Arc<UdpPeer>`) | 远端 peer 句柄。`send(&[u8])` 发送数据、`get_addr()` 获取地址、`close()` 强制关闭。 |
| **`UdpReader`** (`UnboundedReceiver<io::Result<Bytes>>`) | 接收零拷贝 `Bytes` 数据包。超时被驱逐时收到 `Err(TimedOut)`。 |

handler 在独立 Tokio 任务中运行，future 完成或出错时自动清理 peer。

### Builder

| 方法 | 说明 |
| --- | --- |
| `.set_peer_timeout_sec(n)` | 空闲超过 `n` 秒的 peer 将被驱逐。 |
| `.set_buffer_size(n)` | 设置 socket 收发缓冲区大小（字节），默认约 17.8 MB。 |
| `.start(inner)` | 绑定 socket 并启动服务端，服务端关闭时返回。 |

## 运行示例

```bash
cargo run --example echo_server
cargo run --example echo_client -- --addr 127.0.0.1:20001 --task 50
```

## 工作原理

1. `new()` 解析地址；`start()` 创建 `N` 个 socket 绑定到该地址（Unix `N = CPU 核数`，Windows `N = 1`）。
2. 每个 socket 独立循环 `recv_from`。新地址到达时创建 `UdpPeer` 并 spawn handler。
3. 数据以 `Bytes`（零拷贝）通过 unbounded channel 推送，handler 用 `reader.recv().await` 消费。
4. 若开启超时，后台任务每秒扫描并将 `Err(TimedOut)` 推入超时 peer 的 channel。

## License

MIT / Apache-2.0 双协议授权。
