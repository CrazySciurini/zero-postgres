use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, PoisonError};

use crate::error::Result;
use crate::opts::Opts;

use super::Conn;
use super::semaphore::{Permit, Semaphore};

pub struct Pool {
    opts: Opts,
    conns: Mutex<Vec<Conn>>,
    max_idle: usize,
    semaphore: Option<Arc<Semaphore>>,
}

impl Pool {
    pub fn new(opts: Opts) -> Self {
        let semaphore = opts
            .pool_max_concurrency
            .map(|n| Arc::new(Semaphore::new(n)));
        Self {
            conns: Mutex::new(Vec::new()),
            max_idle: opts.pool_max_idle_conn,
            opts,
            semaphore,
        }
    }

    pub fn get(self: &Arc<Self>) -> Result<PooledConn> {
        let permit = self.semaphore.as_ref().map(Semaphore::acquire);
        let conn = loop {
            let idle = self
                .conns
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop();
            match idle {
                Some(mut c) => {
                    if c.ping().is_ok() {
                        break c;
                    }
                    // Connection dead, try next one
                }
                None => break Conn::new(self.opts.clone())?,
            }
        };
        Ok(PooledConn {
            conn: ManuallyDrop::new(conn),
            pool: Arc::clone(self),
            permit,
        })
    }

    fn check_in(&self, mut conn: Conn) {
        if conn.is_broken() {
            return;
        }
        if conn.query_drop("DISCARD ALL").is_err() {
            return;
        }
        let mut conns = self.conns.lock().unwrap_or_else(PoisonError::into_inner);
        if conns.len() < self.max_idle {
            conns.push(conn);
        }
    }
}

pub struct PooledConn {
    pool: Arc<Pool>,
    conn: ManuallyDrop<Conn>,
    permit: Option<Permit>,
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
        self.pool.check_in(conn);
        // Release after check-in so the next permit holder reuses this connection.
        drop(self.permit.take());
    }
}
