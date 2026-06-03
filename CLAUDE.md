# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo check          # Fast compilation check (no codegen)
cargo build          # Full build
cargo test           # Run all tests
cargo fmt --all -- --check   # Check formatting (CI gate)
cargo clippy -- -D warnings  # Lint with warnings as errors (CI gate)
```

There is no `cargo run` — this is a library crate. Use the examples:

```bash
cargo run --example echo_server   # Start echo server on 0.0.0.0:20001
cargo run --example echo_client -- --addr 127.0.0.1:20001 --task 50   # Benchmark client
```

## Architecture

This is a high-throughput UDP server **framework** built on Tokio. The user provides a closure `Fn(UDPPeer, UdpReader, T) -> Future<Result>`; the framework handles socket management, peer tracking, and data dispatch.

### Core flow

1. **`UdpServer::new(addr, handler)`** creates `N` UdpSockets bound to the same address, where N = `num_cpus` on Unix, `1` on Windows. This leverages `SO_REUSEPORT` (Unix-only via `net2`) so the kernel load-balances incoming packets across sockets.

2. **`start(inner)`** spawns one Tokio task per socket that loops on `recv_from`. On each packet:
   - Looks up or creates a `UDPPeer` (keyed by `SocketAddr`) in the socket's `HashMap<SocketAddr, UDPPeer>`.
   - Pushes the raw bytes into the peer's `UnboundedSender<Vec<u8>>`.
   - On the **first** packet from a new address, sends `(peer.clone(), reader, socket_index, addr)` through a shared `UnboundedSender` to the main loop.

3. **Main loop** (the `rx.recv().await` loop in `start`) receives new-peer notifications. For each new peer, it spawns a Tokio task that calls the user's handler `(input_fn)(peer, reader, inner)`. The handler reads from `UdpReader` (the `UnboundedReceiver` side) and uses `peer.send(&buf)` to reply. When the handler future completes, the peer is removed from the socket's HashMap.

4. **Peer timeout** (opt-in via `set_peer_timeout_sec(n)`): a background task runs every 1s, checks `last_read_time` (AtomicI64) on every peer in every socket's HashMap, and calls `peer.close()` on stale ones. `close()` sends an `Err(TimedOut)` through the channel, which the user's handler sees as an error from `reader.recv().await`.

### Key types and files

| File | Purpose |
|---|---|
| `src/lib.rs` | Re-exports via `pub mod prelude` |
| `src/udp_serv.rs` | `UdpServer` struct, `UdpContext`, socket creation with `SO_REUSEPORT`/buffer sizing |
| `src/peer.rs` | `UdpPeer` (per-address peer), `UdpReader`/`UdpSender` type aliases (unbounded channel) |

- **`UDPPeer`** = `Arc<UdpPeer>` — a handle to a remote UDP client. Has `send(&[u8])` and `close()`.
- **`UdpReader`** = `UnboundedReceiver<io::Result<Vec<u8>>>` — the handler reads packets from this. Receives `Err(TimedOut)` when the peer is evicted.
- **`BUFF_MAX_SIZE`** = 4096 bytes — max UDP payload per packet.
- **Buffer sizing**: send/recv socket buffers are set to `1784 * 10000` bytes (~17 MB) for high-throughput scenarios.

### Platform differences

- **Unix**: `SO_REUSEPORT` is set → N sockets, true parallelism. `num_cpus` determines socket count.
- **Windows**: no `SO_REUSEPORT` → 1 socket always (`get_cpu_count()` returns 1). The `net2::UdpBuilder` is still used for `SO_REUSEADDR`.

## Publishing

Triggered by pushing a `v*` tag (e.g., `v1.0.5`). The CI workflow in `.github/workflows/publish.yml` creates a GitHub release from `CHANGELOG.md`, then runs `cargo publish`. Update `Cargo.toml` version and `CHANGELOG.md` before tagging.

## README.md
Add bilingual instructions in English and Chinese, referring to: https://github.com/luyikk/rust_netx/blob/master/README.md