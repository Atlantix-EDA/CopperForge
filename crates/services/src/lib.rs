//! CopperForge backend services — one crate, one module per service.
//!
//! `autodim` (board-edge dimensioning) and `manuf` (board statistics +
//! manufacturability score), plus whatever comes next. Each is pure compute
//! over data the client already has, with serde-ready request/response
//! types.
//!
//! The client calls these **in-process** (`autodim::autodimension`,
//! `manuf::board_stats`), so features work from a bare clone, on wasm, with
//! no server running. The optional `serve` feature (native only) wraps the
//! same functions as an axum HTTP server for self-hosted or remote
//! backends — the plugin seam. It's additive: the in-process path is
//! unchanged whether or not it's enabled.

pub mod autodim;
pub mod manuf;

#[cfg(feature = "serve")]
pub mod serve;
