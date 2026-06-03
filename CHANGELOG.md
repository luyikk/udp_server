# Changelog

## 1.1.0
#### Features
* Replace `net2` with `socket2` 0.6 — the `net2` crate is unmaintained and
  triggers future-incompatibility warnings. `SO_REUSEPORT` now uses `socket2`'s
  native `set_reuse_port()` instead of a separate Unix extension trait.
* Zero-copy data path — receive buffers switched from stack `[u8; 4096]` +
  `to_vec()` to `BytesMut` + `Bytes`. Received packets are shared through the
  channel via reference counting with no memcpy, eliminating per-packet heap
  copies (~400 MB/s saved at 100K PPS).
* Replace `async_lock::Mutex<HashMap>` with `DashMap` — the peer map is now
  lock-free, sharded internally. Recv lookups, handler removals, and timeout
  scans no longer contend on a single mutex.
* Configurable socket buffer size — new `set_buffer_size(n)` builder method.
  Socket creation deferred from `new()` to `start()` so builder options take
  effect.
* Fix all `clippy` warnings — migrate to `io::Error::other()` (Rust 1.95+).

## 1.0.4
#### Features
* update async-lock to 3.3

## 1.0.3
#### Features
* use Mutex because RwLock doesn't mean much.

## 1.0.2
#### Features
* use atomic and RwLock

## 1.0.1
#### Features
* Revise udp socket recv_from err log level to trace level.  
  because it's too verbose

## 1.0.0
#### Features
*  change input function define
*  change set_clean_sec to set_peer_timeout_sec
*  update README.md
*  update version to 1.0.0

## 0.5.1
#### Features
*  optimization does not check timeout performance


## 0.5.0
#### Features
* New code, new performance
