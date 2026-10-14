//! Tests for the compio connection pool

#![cfg(feature = "compio")]
#![allow(clippy::panic_in_result_fn)]

use std::cell::Cell;
use std::rc::Rc;

use zero_postgres::compio::Pool;
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

#[compio::test]
async fn pool_max_concurrency() -> Result<(), Error> {
    let mut opts = test_opts()?;
    opts.pool_max_concurrency = Some(1);
    let pool = Pool::new(opts);

    let mut first = pool.get().await?;
    let acquired = Rc::new(Cell::new(false));
    let waiter = compio::runtime::spawn({
        let pool = Rc::clone(&pool);
        let acquired = Rc::clone(&acquired);
        async move {
            let conn = pool.get().await;
            acquired.set(true);
            conn.map(drop)
        }
    });

    // Give the waiter a chance to run while `first` is checked out.
    first.query_drop("SELECT pg_sleep(0.1)").await?;
    assert!(!acquired.get(), "second get() must wait for the permit");

    drop(first);
    waiter.await.expect("waiter task panicked")?;
    assert!(acquired.get());
    Ok(())
}

#[test]
fn pool_drop_outside_runtime() -> Result<(), Error> {
    let runtime = compio::runtime::Runtime::new()?;
    let pool = Pool::new(test_opts()?);
    let conn = runtime.block_on(pool.get())?;
    // Must not panic even though no runtime is current here.
    drop(conn);
    Ok(())
}
