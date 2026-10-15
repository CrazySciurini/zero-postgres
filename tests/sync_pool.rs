//! Tests for the sync connection pool

#![cfg(feature = "sync")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use zero_postgres::sync::Pool;
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
fn max_concurrency_limits_checked_out_conns() {
    let mut opts = test_opts().unwrap();
    opts.pool_max_concurrency = Some(2);
    let pool = Arc::new(Pool::new(opts));
    let active = AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..20 {
                    let mut conn = pool.get().unwrap();
                    let now_active = active.fetch_add(1, Ordering::SeqCst) + 1;
                    assert!(now_active <= 2, "{now_active} connections checked out");
                    conn.query_drop("SELECT 1").unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                }
            });
        }
    });
}

#[test]
fn failed_connect_releases_permit() {
    let mut opts = test_opts().unwrap();
    opts.port = 1;
    opts.pool_max_concurrency = Some(1);
    let pool = Arc::new(Pool::new(opts));
    // With a leaked permit, the second call would block forever.
    assert!(pool.get().is_err());
    assert!(pool.get().is_err());
}
