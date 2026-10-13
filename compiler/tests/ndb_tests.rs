use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use nxl_compiler::ndb;

fn setup_project(dir: &Path) {
    fs::create_dir_all(dir.join("database").join("migrations")).unwrap();
    fs::create_dir_all(dir.join("database").join("seeders")).unwrap();
    fs::create_dir_all(dir.join("database").join("factories")).unwrap();
    fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"t\"\n\n[database]\ndriver = \"sqlite\"\npath = \"database/database.sqlite\"\n",
    )
    .unwrap();
}

fn write_migration(dir: &Path, file: &str, sql: &str) {
    fs::write(dir.join("database").join("migrations").join(file), sql).unwrap();
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("nxl_p14_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn migrate_creates_tables_and_tracks_batches() {
    let dir = tmpdir("migrate");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_users.sql",
        "-- up\nCREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\n-- down\nDROP TABLE users;\n",
    );
    let ran = ndb::migrate(&dir).unwrap();
    assert_eq!(ran, vec!["0001_users.sql"]);
    // second run is a no-op
    assert!(ndb::migrate(&dir).unwrap().is_empty());
    let status = ndb::migration_status(&dir).unwrap();
    assert_eq!(status.len(), 1);
    assert!(status[0].1.is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn failing_migration_rolls_back_and_does_not_record() {
    let dir = tmpdir("failing");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_bad.sql",
        "-- up\nCREATE TABLE a (id INTEGER);\nINSERT INTO nonexistent VALUES (1);\n-- down\nDROP TABLE a;\n",
    );
    let err = ndb::migrate(&dir).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("rolled back"), "{msg}");
    // table must not exist
    let conn = ndb::connect(&dir).unwrap();
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(exists, 0);
    // and the migration is not recorded
    let status = ndb::migration_status(&dir).unwrap();
    assert_eq!(status[0].1, None);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rollback_reverts_last_batch_only() {
    let dir = tmpdir("rollback");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE a (id INTEGER);\n-- down\nDROP TABLE a;\n",
    );
    write_migration(
        &dir,
        "0002_b.sql",
        "-- up\nCREATE TABLE b (id INTEGER);\n-- down\nDROP TABLE b;\n",
    );
    ndb::migrate(&dir).unwrap();
    // both in batch 1 (single run) -> rollback removes both
    let rolled = ndb::rollback(&dir).unwrap();
    assert_eq!(rolled.len(), 2);
    let conn = ndb::connect(&dir).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('a','b')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn refresh_drops_and_recreates() {
    let dir = tmpdir("refresh");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE a (id INTEGER);\n-- down\nDROP TABLE a;\n",
    );
    ndb::migrate(&dir).unwrap();
    let (rolled, ran) = ndb::refresh(&dir).unwrap();
    assert_eq!(rolled.len(), 1);
    assert_eq!(ran.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fresh_rebuilds_from_scratch() {
    let dir = tmpdir("fresh");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE a (id INTEGER);\n-- down\nDROP TABLE a;\n",
    );
    ndb::migrate(&dir).unwrap();
    let ran = ndb::fresh(&dir, false).unwrap();
    assert_eq!(ran.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn seeders_run_in_transaction() {
    let dir = tmpdir("seeders");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE t (id INTEGER, v TEXT);\n-- down\nDROP TABLE t;\n",
    );
    ndb::migrate(&dir).unwrap();
    fs::write(
        dir.join("database").join("seeders").join("001_seed.sql"),
        "INSERT INTO t VALUES (1, 'x'); INSERT INTO t VALUES (2, 'y');\n",
    )
    .unwrap();
    let ran = ndb::run_seeders(&dir).unwrap();
    assert_eq!(ran, vec!["001_seed.sql"]);
    let conn = ndb::connect(&dir).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn factory_executes_template_n_times() {
    let dir = tmpdir("factory");
    setup_project(&dir);
    write_migration(&dir, "0001_a.sql", "-- up\nCREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT);\n-- down\nDROP TABLE t;\n");
    ndb::migrate(&dir).unwrap();
    fs::write(
        dir.join("database").join("factories").join("row.sql"),
        "INSERT INTO t (v) VALUES ('f');\n",
    )
    .unwrap();
    let n = ndb::run_factory(&dir, "row", 5).unwrap();
    assert_eq!(n, 5);
    let conn = ndb::connect(&dir).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 5);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn query_builder_is_parameterized_and_safe() {
    let dir = tmpdir("query");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);\n-- down\nDROP TABLE t;\n",
    );
    ndb::migrate(&dir).unwrap();
    let conn = ndb::connect(&dir).unwrap();
    ndb::Query::insert(&conn, "t", &[("name", ndb::Value::Text("alice".into()))]).unwrap();
    ndb::Query::insert(
        &conn,
        "t",
        &[("name", ndb::Value::Text("' OR '1'='1".into()))],
    )
    .unwrap();

    // Classic injection payload stays data, not code.
    let rows = ndb::Query::table("t")
        .where_eq("name", ndb::Value::Text("' OR '1'='1".into()))
        .get(&conn)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "' OR '1'='1");

    let all = ndb::Query::table("t").count(&conn).unwrap();
    assert_eq!(all, 2);

    // Update + delete round trip
    let updated = ndb::Query::update(
        &conn,
        "t",
        &[("name", ndb::Value::Text("bob".into()))],
        "name",
        ndb::Value::Text("alice".into()),
    )
    .unwrap();
    assert_eq!(updated, 1);
    let deleted = ndb::Query::delete(&conn, "t", "name", ndb::Value::Text("bob".into())).unwrap();
    assert_eq!(deleted, 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn transaction_rolls_back_on_error() {
    let dir = tmpdir("tx");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_a.sql",
        "-- up\nCREATE TABLE t (id INTEGER UNIQUE);\n-- down\nDROP TABLE t;\n",
    );
    ndb::migrate(&dir).unwrap();
    let conn = ndb::connect(&dir).unwrap();
    let result = ndb::transaction(&conn, |c| {
        c.execute("INSERT INTO t VALUES (1)", [])
            .map_err(|e| ndb::NdbError::Sql(e.to_string()))?;
        c.execute("INSERT INTO t VALUES (1)", [])
            .map_err(|e| ndb::NdbError::Sql(e.to_string()))?;
        Ok(())
    });
    assert!(result.is_err());
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unsupported_driver_is_honest_error() {
    let dir = tmpdir("driver");
    setup_project(&dir);
    fs::write(
        dir.join("nxl.toml"),
        "[package]\nname = \"t\"\n\n[database]\ndriver = \"oracle\"\npath = \"x\"\n",
    )
    .unwrap();
    let err = ndb::db_create(&dir).unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("not supported"),
        "unknown driver must yield an honest unsupported error, got: {err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn known_driver_names_are_recognized() {
    for name in ["sqlite", "postgres", "postgresql", "pg", "mysql", "mariadb", "maria"] {
        assert!(
            ndb::Driver::from_name(name).is_some(),
            "driver '{name}' should be recognized"
        );
    }
    assert!(ndb::Driver::from_name("oracle").is_none());
    assert_eq!(ndb::Driver::from_name("POSTGRES").unwrap().name(), "postgres");
}

#[test]
fn make_helpers_create_stub_files() {
    let dir = tmpdir("make");
    setup_project(&dir);
    assert!(ndb::make_migration(&dir, "create_users").unwrap().exists());
    assert!(ndb::make_seeder(&dir, "users").unwrap().exists());
    assert!(ndb::make_factory(&dir, "user").unwrap().exists());
    assert!(ndb::make_model(&dir, "User").unwrap().exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn database_orm_helpers_work_on_sqlite() {
    let dir = tmpdir("orm");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_items.sql",
        "-- up\nCREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL, qty INTEGER NOT NULL);\n-- down\nDROP TABLE items;\n",
    );
    ndb::migrate(&dir).unwrap();

    let mut db = ndb::Database::open(&dir).unwrap();
    db.insert(
        "items",
        &[
            ("name", ndb::Value::Text("widget".into())),
            ("qty", ndb::Value::Int(3)),
        ],
    )
    .unwrap();
    db.insert(
        "items",
        &[
            ("name", ndb::Value::Text("gadget".into())),
            ("qty", ndb::Value::Int(7)),
        ],
    )
    .unwrap();

    let all = db.query(&ndb::Query::table("items")).unwrap();
    assert_eq!(all.len(), 2);

    let n = db
        .count(&ndb::Query::table("items").where_eq("name", ndb::Value::Text("widget".into())))
        .unwrap();
    assert_eq!(n, 1);

    let first = db
        .first(&ndb::Query::table("items").order_by("id", false))
        .unwrap()
        .unwrap();
    assert_eq!(first.get("name").map(String::as_str), Some("widget"));

    let updated = db
        .update(
            "items",
            &[("qty", ndb::Value::Int(9))],
            "name",
            ndb::Value::Text("widget".into()),
        )
        .unwrap();
    assert_eq!(updated, 1);

    let deleted = db
        .delete("items", "name", ndb::Value::Text("gadget".into()))
        .unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(db.count(&ndb::Query::table("items")).unwrap(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[derive(Debug, PartialEq)]
struct User {
    id: i64,
    name: String,
}

impl ndb::Model for User {
    const TABLE: &'static str = "users";
    fn columns() -> &'static [&'static str] {
        &["id", "name"]
    }
    fn values(&self) -> Vec<(&'static str, ndb::Value)> {
        vec![
            ("id", ndb::Value::Int(self.id)),
            ("name", ndb::Value::Text(self.name.clone())),
        ]
    }
    fn from_row(row: &BTreeMap<String, String>) -> ndb::NdbResult<Self> {
        Ok(User {
            id: ndb::row_i64(row, "id")?,
            name: ndb::row_text(row, "name")?.to_owned(),
        })
    }
}

#[test]
fn orm_model_crud_roundtrip() {
    let dir = tmpdir("model");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_users.sql",
        "-- up\nCREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\n-- down\nDROP TABLE users;\n",
    );
    ndb::migrate(&dir).unwrap();

    let mut db = ndb::Database::open(&dir).unwrap();
    db.model_insert(&User { id: 1, name: "ada".into() }).unwrap();
    db.model_insert(&User { id: 2, name: "bob".into() }).unwrap();
    assert_eq!(db.model_count::<User>().unwrap(), 2);

    let all: Vec<User> = db.model_all::<User>().unwrap();
    assert_eq!(all.len(), 2);

    let found = db
        .model_find::<User>("id", ndb::Value::Int(2))
        .unwrap()
        .unwrap();
    assert_eq!(found, User { id: 2, name: "bob".into() });
    assert!(db
        .model_find::<User>("id", ndb::Value::Int(99))
        .unwrap()
        .is_none());

    let deleted = db.model_delete::<User>("id", ndb::Value::Int(1)).unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(db.model_count::<User>().unwrap(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn connection_pool_reuses_and_bounds_connections() {
    let dir = tmpdir("pool");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_users.sql",
        "-- up\nCREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\n-- down\nDROP TABLE users;\n",
    );
    ndb::migrate(&dir).unwrap();

    let pool = ndb::ConnectionPool::new(&dir, 2).unwrap();
    assert_eq!(pool.max_size(), 2);
    assert_eq!(pool.live_count(), 0);

    {
        let mut c = pool.get().unwrap();
        c.insert("users", &[("name", ndb::Value::Text("a".into()))])
            .unwrap();
        assert_eq!(pool.live_count(), 1);
    }
    assert_eq!(pool.idle_count(), 1);

    {
        let mut c = pool.get().unwrap();
        assert_eq!(pool.live_count(), 1, "connection should be reused, not reopened");
        assert_eq!(c.count(&ndb::Query::table("users")).unwrap(), 1);
    }
    assert_eq!(pool.idle_count(), 1);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn migration_audit_log_tracks_apply_and_rollback() {
    let dir = tmpdir("audit");
    setup_project(&dir);
    write_migration(
        &dir,
        "0001_users.sql",
        "-- up\nCREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);\n-- down\nDROP TABLE users;\n",
    );
    ndb::migrate(&dir).unwrap();

    let audit = ndb::migration_audit(&dir).unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].name, "0001_users.sql");
    assert_eq!(audit[0].direction, "up");
    assert_eq!(audit[0].outcome, "ok");
    assert_eq!(audit[0].batch, 1);

    ndb::rollback(&dir).unwrap();
    let audit = ndb::migration_audit(&dir).unwrap();
    assert_eq!(audit.len(), 2);
    assert_eq!(audit[1].direction, "down");
    assert_eq!(audit[1].outcome, "ok");
    assert_eq!(audit[1].batch, 1);

    // A failing migration is still audited as an outcome, without being applied.
    write_migration(
        &dir,
        "0002_bad.sql",
        "-- up\nCREATE TABLE broken (id INTEGER);\nINSERT INTO nope VALUES (1);\n-- down\nDROP TABLE broken;\n",
    );
    assert!(ndb::migrate(&dir).is_err());
    let audit = ndb::migration_audit(&dir).unwrap();
    let last = audit.last().unwrap();
    assert_eq!(last.name, "0002_bad.sql");
    assert_eq!(last.outcome, "failed");
    assert_eq!(last.direction, "up");

    let _ = fs::remove_dir_all(&dir);
}
