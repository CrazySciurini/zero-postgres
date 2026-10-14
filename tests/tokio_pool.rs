//! Tests for the tokio connection pool

#![cfg(feature = "tokio")]

use std::sync::Arc;

use zero_postgres::tokio::Pool;
use zero_postgres::{Error, Opts};

fn test_opts() -> Result<Opts, Error> {
    let mut db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres".to_string());
    if !db_url.contains("tls_mode=") {
        if db_url.contains('?') {
            db_url.push_str("&tls_mode=disable");
        } else {
            db_url.push_str("?tls_mode=disable");
        }
    }
    Opts::try_from(db_url.as_str())
}

#[test]
fn pool_drop_outside_runtime() -> Result<(), Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = Arc::new(Pool::new(test_opts()?));
    let conn = runtime.block_on(pool.get())?;
    // Must not panic even though no runtime is current here.
    drop(conn);
    Ok(())
}
