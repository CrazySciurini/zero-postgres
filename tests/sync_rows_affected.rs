//! Tests that drop methods report the row count from the server's command tag.

#![allow(clippy::panic_in_result_fn)]

use std::env;
use zero_postgres::Error;
use zero_postgres::sync::Conn;

fn get_conn() -> Result<Conn, Error> {
    let mut db_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/postgres".to_string());
    if !db_url.contains("sslmode=") {
        if db_url.contains('?') {
            db_url.push_str("&sslmode=disable");
        } else {
            db_url.push_str("?sslmode=disable");
        }
    }
    Conn::new(db_url.as_str())
}

#[test]
fn drop_methods_report_rows_affected() -> Result<(), Error> {
    let mut conn = get_conn()?;
    conn.query_drop("CREATE TEMP TABLE rows_affected_test (id INT PRIMARY KEY, value INT)")?;

    let inserted = conn.query_drop("INSERT INTO rows_affected_test VALUES (1, 10), (2, 20)")?;
    assert_eq!(inserted, Some(2));

    let updated = conn.exec_drop("UPDATE rows_affected_test SET value = value + $1", (1_i32,))?;
    assert_eq!(updated, Some(2));

    // Updates id 2 and inserts id 3.
    let merged = conn.exec_drop(
        "MERGE INTO rows_affected_test AS t \
         USING (VALUES (2, $1::int), (3, $1::int)) AS s (id, value) ON t.id = s.id \
         WHEN MATCHED THEN UPDATE SET value = s.value \
         WHEN NOT MATCHED THEN INSERT (id, value) VALUES (s.id, s.value)",
        (99_i32,),
    )?;
    assert_eq!(merged, Some(2));

    let deleted = conn.query_drop("DELETE FROM rows_affected_test WHERE value = 99")?;
    assert_eq!(deleted, Some(2));

    let created = conn.query_drop("CREATE TEMP TABLE rows_affected_other (id INT)")?;
    assert_eq!(created, None);
    Ok(())
}
