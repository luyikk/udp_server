//! # udp_server
//!
//! High-performance async UDP server framework built on Tokio.
//!
//! ## Design
//!
//! The framework binds multiple UDP sockets to the same address (leveraging
//! `SO_REUSEPORT` on Unix for kernel-level load balancing). Each remote address
//! is abstracted as a [`UdpPeer`] with its own send/receive channel and a
//! dedicated handler task. Users provide a single async closure — the framework
//! handles socket lifecycle, peer creation/removal, and idle timeout cleanup.
//!
//! ## Quick Example
//!
//! ```rust,no_run
//! use udp_server::prelude::UdpServer;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! UdpServer::new("0.0.0.0:20001", |peer, mut reader, _| async move {
//!     while let Some(Ok(data)) = reader.recv().await {
//!         peer.send(&data).await?;
//!     }
//!     Ok(())
//! })?
//! .set_peer_timeout_sec(20)
//! .start(())
//! .await?;
//! # Ok(())
//! # }
//! ```

mod peer;
mod udp_serv;

/// Convenience re-exports so users can write `use udp_server::prelude::*`.
///
/// Includes:
/// - [`UdpServer`] — the server entry point
/// - [`UdpPeer`] / [`UDPPeer`] — peer handle
/// - [`UdpSender`] / [`UdpReader`] — channel type aliases for send/receive
pub mod prelude {
    pub use super::peer::*;
    pub use super::udp_serv::UdpServer;
}
