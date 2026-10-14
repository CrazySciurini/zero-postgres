#![warn(
    clippy::expect_used,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::unwrap_used
)]

//! A high-performance PostgreSQL client library.
//!
//! [User Guide](https://elbaro.github.io/zero-postgres/)

// private
mod buffer_pool;
mod buffer_set;
mod error;
mod opts;
mod pipeline;
mod statement;
#[cfg(any(feature = "sync-tls", feature = "tokio-tls", feature = "compio-tls"))]
mod tls;

// pub
pub mod conversion;
pub mod handler;
pub mod protocol;
pub mod state;

#[cfg(feature = "sync")]
pub mod sync;

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "compio")]
pub mod compio;

#[cfg(feature = "diesel")]
pub mod diesel;

#[cfg(feature = "derive")]
pub use zero_postgres_derive as r#macro;

pub use buffer_pool::BufferPool;
pub use buffer_set::BufferSet;
pub use conversion::PG_EPOCH_JULIAN_DAY;
pub use error::{Error, Result, ServerError};
pub use handler::AsyncMessageHandler;
pub use opts::{Opts, TlsMode};
pub use pipeline::Ticket;
pub use state::action::AsyncMessage;
pub use state::extended::PreparedStatement;
pub use statement::{IntoStatement, StatementRef};

/// The rustls version used for TLS, for building [`Opts::tls_config`].
#[cfg(any(feature = "sync-tls", feature = "tokio-tls", feature = "compio-tls"))]
pub use rustls;
