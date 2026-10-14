//! Asynchronous connection pool.

use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{Error, Result};
use crate::opts::Opts;

use super::Conn;

pub struct Pool {
    opts: Opts,
    conns: ArrayQueue<Conn>,
    semaphore: Option<Arc<Semaphore>>,
}

impl Pool {
    pub fn new(opts: Opts) -> Self {
        let semaphore = opts
            .pool_max_concurrency
            .map(|n| Arc::new(Semaphore::new(n)));
        Self {
            conns: ArrayQueue::new(opts.pool_max_idle_conn),
            opts,
            semaphore,
        }
    }

    pub async fn get(self: &Arc<Self>) -> Result<PooledConn> {
        let permit = if let Some(sem) = &self.semaphore {
            Some(
                Arc::clone(sem)
                    .acquire_owned()
                    .await
                    .map_err(|_unhelpful_err| {
                        Error::LibraryBug("pool semaphore is closed".into())
                    })?,
            )
        } else {
            None
        };
        let conn = loop {
            match self.conns.pop() {
                Some(mut c) => {
                    if c.ping().await.is_ok() {
                        break c;
                    }
                    // Connection dead, try next one
                }
                None => break Conn::new(self.opts.clone()).await?,
            }
        };
        Ok(PooledConn {
            conn: ManuallyDrop::new(conn),
            pool: Arc::clone(self),
            permit,
        })
    }

    fn check_in(self: &Arc<Self>, mut conn: Conn, permit: Option<OwnedSemaphorePermit>) {
        if conn.is_broken() {
            return;
        }
        // Without a runtime (e.g. during shutdown) the connection cannot be
        // reset, so it is dropped instead of returned to the pool.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let pool = Arc::clone(self);
        handle.spawn(async move {
            if conn.query_drop("DISCARD ALL").await.is_ok() {
                let _ = pool.conns.push(conn);
            }
            drop(permit);
        });
    }
}

pub struct PooledConn {
    pool: Arc<Pool>,
    conn: ManuallyDrop<Conn>,
    permit: Option<OwnedSemaphorePermit>,
}

impl Deref for PooledConn {
    type Target = Conn;
    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl DerefMut for PooledConn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.conn
    }
}

impl Drop for PooledConn {
    fn drop(&mut self) {
        // SAFETY: conn is never accessed after this
        let conn = unsafe { ManuallyDrop::take(&mut self.conn) };
        self.pool.check_in(conn, self.permit.take());
    }
}
