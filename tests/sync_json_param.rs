//! Tests that string parameters bind to JSON and JSONB.

#![allow(clippy::panic_in_result_fn)]

use std::env;
use zero_postgres::Error;
use zero_postgres::sync::Conn;

fn get_conn() -> Result<Conn, Error> {
    let mut db_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/postgres".to_string());
    if !db_url.contains("tls_mode=") {
        if db_url.contains('?') {
            db_url.push_str("&tls_mode=disable");
        } else {
            db_url.push_str("?tls_mode=disable");
        }
    }
    Conn::new(db_url.as_str())
}

#[test]
fn str_binds_to_jsonb() -> Result<(), Error> {
    let mut conn = get_conn()?;
    let stmt = conn.prepare("SELECT $1::jsonb ->> 'a'")?;
    let row: Option<(String,)> = conn.exec_first(&stmt, (r#"{"a": "b"}"#,))?;
    assert_eq!(row, Some(("b".to_string(),)));
    Ok(())
}

#[test]
fn str_binds_to_json() -> Result<(), Error> {
    let mut conn = get_conn()?;
    let stmt = conn.prepare("SELECT $1::json ->> 'a'")?;
    let row: Option<(String,)> = conn.exec_first(&stmt, (r#"{"a": "b"}"#,))?;
    assert_eq!(row, Some(("b".to_string(),)));
    Ok(())
}
