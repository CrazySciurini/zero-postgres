#![allow(clippy::panic_in_result_fn, clippy::shadow_unrelated)]

use zero_postgres::Error;
use zero_postgres::tokio::Conn;

async fn get_conn() -> Result<Conn, Error> {
    let mut db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/postgres".to_string());
    if !db_url.contains("tls_mode=") {
        if db_url.contains('?') {
            db_url.push_str("&tls_mode=disable");
        } else {
            db_url.push_str("?tls_mode=disable");
        }
    }
    Conn::new(db_url.as_str()).await
}

/// Statements the server rejects, with the SQLSTATE each must report.
/// Covers errors raised during Parse (42P01, 42703) and during Execute (22012).
const REJECTED: [(&str, &str); 3] = [
    ("SELECT $1::int FROM missing_table", "42P01"),
    ("SELECT missing_column, $1::int", "42703"),
    ("SELECT 1 / $1::int", "22012"),
];

#[tokio::test]
async fn server_errors_leave_connection_usable() -> Result<(), Error> {
    let mut conn = get_conn().await?;

    for (sql, sqlstate) in REJECTED {
        let err = conn.exec_drop(sql, (0_i32,)).await.expect_err(sql);
        assert_eq!(err.sqlstate(), Some(sqlstate), "{sql}: {err}");
        let rows: Vec<(i32,)> = conn.exec_collect("SELECT $1::int", (7_i32,)).await?;
        assert_eq!(rows, [(7,)]);
    }

    let err = conn
        .query_drop("SELECT * FROM missing_table")
        .await
        .expect_err("simple query should fail");
    assert_eq!(err.sqlstate(), Some("42P01"), "{err}");
    conn.query_drop("SELECT 1").await?;
    Ok(())
}
