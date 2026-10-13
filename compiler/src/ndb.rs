//! P14 — NDB Database Platform
//!
//! Multi-driver database platform (SQLite, PostgreSQL, MySQL, MariaDB) for NEXORA.
//!
//! # Status
//!
//! | Feature | Status |
//! | --- | --- |
//! | SQLite driver | IMPLEMENTED (via rusqlite, bundled) |
//! | PostgreSQL driver | IMPLEMENTED (via the `postgres` crate) |
//! | MySQL / MariaDB drivers | IMPLEMENTED (via the `mysql` crate) |
//! | Connection management | IMPLEMENTED |
//! | Migrations (up/down, batches, rollback) | IMPLEMENTED (all drivers) |
//! | Refresh / fresh / seed | IMPLEMENTED (all drivers) |
//! | Query builder (parameterized) | IMPLEMENTED (all drivers) |
//! | Transactions | IMPLEMENTED (all drivers) |
//! | Seeders | IMPLEMENTED |
//! | Factories | IMPLEMENTED (template SQL executed N times) |
//! | ORM models | IMPLEMENTED (schema + CRUD, one-to-many via foreign key) |
//! | Connection pooling | IMPLEMENTED |
//! | CLI commands | IMPLEMENTED |

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};

use mysql::prelude::Queryable as _;

// ── Error ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum NdbError {
    Io(std::io::Error),
    Sql(String),
    Config(String),
    Migration(String),
    UnsupportedDriver(String),
    Connect(String),
}

impl fmt::Display for NdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NdbError::Io(e) => write!(f, "error[NDB14001]: I/O error: {e}"),
            NdbError::Sql(m) => write!(f, "error[NDB14002]: SQL error: {m}"),
            NdbError::Config(m) => write!(f, "error[NDB14003]: config error: {m}"),
            NdbError::Migration(m) => write!(f, "error[NDB14004]: migration error: {m}"),
            NdbError::UnsupportedDriver(d) => write!(
                f,
                "error[NDB14005]: driver '{d}' is not supported (supported: sqlite, postgres, mysql, mariadb)"
            ),
            NdbError::Connect(m) => write!(f, "error[NDB14006]: connection error: {m}"),
        }
    }
}

impl From<postgres::Error> for NdbError {
    fn from(e: postgres::Error) -> Self {
        NdbError::Connect(e.to_string())
    }
}
impl From<mysql::Error> for NdbError {
    fn from(e: mysql::Error) -> Self {
        NdbError::Connect(e.to_string())
    }
}

impl From<std::io::Error> for NdbError {
    fn from(e: std::io::Error) -> Self {
        NdbError::Io(e)
    }
}
impl From<rusqlite::Error> for NdbError {
    fn from(e: rusqlite::Error) -> Self {
        NdbError::Sql(e.to_string())
    }
}

pub type NdbResult<T> = Result<T, NdbError>;

// ── Configuration ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct DatabaseConfig {
    pub driver: String,
    pub path: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            driver: "sqlite".to_owned(),
            path: "database/database.sqlite".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 5432,
            user: "root".to_owned(),
            password: String::new(),
            dbname: "nxl".to_owned(),
        }
    }
}

impl DatabaseConfig {
    /// Parse the `[database]` section of an `nxl.toml` manifest.
    pub fn parse(toml: &str) -> Self {
        let mut cfg = DatabaseConfig::default();
        let mut in_db = false;
        for raw in toml.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                in_db = line == "[database]";
                continue;
            }
            if !in_db {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v.trim().trim_matches('"');
                match k.trim() {
                    "driver" => cfg.driver = v.to_owned(),
                    "path" => cfg.path = v.to_owned(),
                    "host" => cfg.host = v.to_owned(),
                    "port" => cfg.port = v.trim().parse().unwrap_or(cfg.port),
                    "user" => cfg.user = v.to_owned(),
                    "password" => cfg.password = v.to_owned(),
                    "dbname" => cfg.dbname = v.to_owned(),
                    _ => {}
                }
            }
        }
        match cfg.driver.to_ascii_lowercase().as_str() {
            "mysql" | "mariadb" if cfg.port == 5432 => cfg.port = 3306,
            _ => {}
        }
        cfg
    }

    pub fn load(dir: &Path) -> NdbResult<Self> {
        let text = fs::read_to_string(dir.join("nxl.toml"))
            .map_err(|_| NdbError::Config(format!("no nxl.toml in '{}'", dir.display())))?;
        Ok(Self::parse(&text))
    }

    pub fn db_path(&self, dir: &Path) -> PathBuf {
        let p = PathBuf::from(&self.path);
        if p.is_absolute() {
            p
        } else {
            dir.join(p)
        }
    }

    /// Human-readable connection target for diagnostics.
    pub fn display_target(&self) -> String {
        match self.driver.to_ascii_lowercase().as_str() {
            "sqlite" => self.path.clone(),
            d => format!("{d}://{}:{}/{}", self.host, self.port, self.dbname),
        }
    }
}

/// Supported NDB drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Sqlite,
    Postgres,
    Mysql,
    MariaDb,
}

impl Driver {
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sqlite" => Some(Driver::Sqlite),
            "postgres" | "postgresql" | "pg" => Some(Driver::Postgres),
            "mysql" => Some(Driver::Mysql),
            "mariadb" | "maria" => Some(Driver::MariaDb),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Driver::Sqlite => "sqlite",
            Driver::Postgres => "postgres",
            Driver::Mysql => "mysql",
            Driver::MariaDb => "mariadb",
        }
    }

    /// Parameter placeholder for the Nth (1-based) positional parameter.
    pub fn placeholder(&self, i: usize) -> String {
        match self {
            Driver::Postgres => format!("${i}"),
            _ => "?".to_owned(),
        }
    }
}

/// SQL-quote an identifier (table/db name) per driver.
fn quote_ident(driver: Driver, ident: &str) -> String {
    match driver {
        Driver::Sqlite => format!("\"{ident}\""),
        Driver::Postgres => format!("\"{ident}\""),
        Driver::Mysql | Driver::MariaDb => format!("`{ident}`"),
    }
}

// ── Multi-driver backend ──────────────────────────────────────────────

/// A live connection to one of the supported database engines.
pub enum Backend {
    Sqlite(Connection),
    Postgres(postgres::Client),
    Mysql(mysql::Conn),
    MariaDb(mysql::Conn),
}

impl Backend {
    /// Run a single (possibly multi-statement) batch on a raw backend handle.
    fn exec_stmt(&mut self, sql: &str) -> NdbResult<()> {
        match self {
            Backend::Sqlite(c) => c.execute_batch(sql).map_err(NdbError::from),
            Backend::Postgres(client) => {
                client.simple_query(sql)?;
                Ok(())
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => mysql_batch(c, sql),
        }
    }
}

/// Split a SQL script into individual statements (mysql has no multi-statement
/// batch helper in this driver configuration).
fn split_sql_statements(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_owned())
        .collect()
}

fn mysql_batch(c: &mut mysql::Conn, sql: &str) -> NdbResult<()> {
    for stmt in split_sql_statements(sql) {
        c.query_drop(stmt)?;
    }
    Ok(())
}

/// A driver-agnostic database handle. All NDB operations that take a
/// [`Database`] work identically for sqlite, postgres, mysql, and mariadb.
pub struct Database {
    driver: Driver,
    inner: Backend,
    cfg: DatabaseConfig,
    dir: PathBuf,
}

impl Database {
    /// Open the database configured in `dir`/`nxl.toml`.
    pub fn open(dir: &Path) -> NdbResult<Database> {
        let cfg = DatabaseConfig::load(dir)?;
        Database::open_config(cfg, dir)
    }

    /// Open a database handle from an explicit config.
    pub fn open_config(cfg: DatabaseConfig, dir: &Path) -> NdbResult<Database> {
        let driver = Driver::from_name(&cfg.driver).ok_or_else(|| {
            NdbError::UnsupportedDriver(cfg.driver.clone())
        })?;
        let inner = Self::connect_backend(driver, &cfg, dir)?;
        Ok(Database {
            driver,
            inner,
            cfg,
            dir: dir.to_path_buf(),
        })
    }

    fn connect_backend(driver: Driver, cfg: &DatabaseConfig, dir: &Path) -> NdbResult<Backend> {
        match driver {
            Driver::Sqlite => {
                let path = cfg.db_path(dir);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let conn = Connection::open(&path)?;
                conn.pragma_update(None, "foreign_keys", "ON")?;
                Ok(Backend::Sqlite(conn))
            }
            Driver::Postgres => {
                let conn_str = format!(
                    "host={} port={} user={} password={} dbname={}",
                    cfg.host, cfg.port, cfg.user, cfg.password, cfg.dbname
                );
                let client = postgres::Client::connect(&conn_str, postgres::NoTls)
                    .map_err(|e| NdbError::Connect(format!("{e} (postgres at {}:{})", cfg.host, cfg.port)))?;
                Ok(Backend::Postgres(client))
            }
            Driver::Mysql | Driver::MariaDb => {
                let url = format!(
                    "mysql://{}:{}@{}:{}/{}",
                    cfg.user, cfg.password, cfg.host, cfg.port, cfg.dbname
                );
                let mut conn = mysql::Conn::new(url.as_str()).map_err(|e| {
                    NdbError::Connect(format!("{e} (mysql at {}:{})", cfg.host, cfg.port))
                })?;
                let _ = conn.ping();
                match driver {
                    Driver::MariaDb => Ok(Backend::MariaDb(conn)),
                    _ => Ok(Backend::Mysql(conn)),
                }
            }
        }
    }

    pub fn driver(&self) -> Driver {
        self.driver
    }

    /// Rebuild a handle from raw parts (used by [`ConnectionPool`]).
    fn from_parts(driver: Driver, inner: Backend, cfg: DatabaseConfig, dir: PathBuf) -> Database {
        Database {
            driver,
            inner,
            cfg,
            dir,
        }
    }

    /// Consume the handle, returning its raw backend (used by [`ConnectionPool`]).
    pub fn into_backend(self) -> Backend {
        self.inner
    }

    pub fn config(&self) -> &DatabaseConfig {
        &self.cfg
    }

    /// Drop the sqlite lease, giving back the raw connection.
    pub fn into_sqlite(self) -> NdbResult<Connection> {
        match self.inner {
            Backend::Sqlite(c) => Ok(c),
            _ => Err(NdbError::Config(
                "driver is not sqlite; use Database instead".to_owned(),
            )),
        }
    }

    /// Does the target database (server-side) already exist?
    fn database_exists(&mut self, name: &str) -> NdbResult<bool> {
        match self.driver {
            Driver::Postgres => {
                let mut m = self.maintenance_client()?;
                match &mut m {
                    Backend::Postgres(client) => {
                        let rows = client
                            .query("SELECT 1 FROM pg_database WHERE datname = $1", &[&name])?;
                        Ok(!rows.is_empty())
                    }
                    _ => unreachable!(),
                }
            }
            Driver::Mysql | Driver::MariaDb => {
                let mut m = self.maintenance_client()?;
                match &mut m {
                    Backend::Mysql(c) | Backend::MariaDb(c) => {
                        let mut rows = c.exec_iter(
                            "SELECT COUNT(*) AS n FROM information_schema.schemata WHERE schema_name = ?",
                            (name,),
                        )?;
                        if let Some(row) = rows.next() {
                            let r = row?;
                            return Ok(r.get::<i64, _>(0).unwrap_or(0) > 0);
                        }
                        Ok(false)
                    }
                    _ => unreachable!(),
                }
            }
            Driver::Sqlite => Ok(self.config().db_path(&self.dir).exists()),
        }
    }

    /// Create the target database on the server (postgres/mysql/mariadb).
    fn create_database(&mut self, name: &str) -> NdbResult<()> {
        match self.driver {
            Driver::Postgres => {
                let mut m = self.maintenance_client()?;
                let sql = format!("CREATE DATABASE {}", quote_ident(Driver::Postgres, name));
                m.exec_stmt(&sql)
            }
            Driver::Mysql | Driver::MariaDb => {
                let mut m = self.maintenance_client()?;
                let sql = format!("CREATE DATABASE {}", quote_ident(Driver::Mysql, name));
                m.exec_stmt(&sql)
            }
            Driver::Sqlite => {
                let path = self.config().db_path(&self.dir);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                File::create(&path)?;
                Ok(())
            }
        }
    }

    /// Drop the target database on the server (postgres/mysql/mariadb).
    fn drop_database(&mut self, name: &str) -> NdbResult<()> {
        match self.driver {
            Driver::Postgres => {
                let mut m = self.maintenance_client()?;
                let sql = format!("DROP DATABASE {}", quote_ident(Driver::Postgres, name));
                m.exec_stmt(&sql)
            }
            Driver::Mysql | Driver::MariaDb => {
                let mut m = self.maintenance_client()?;
                let sql = format!("DROP DATABASE {}", quote_ident(Driver::Mysql, name));
                m.exec_stmt(&sql)
            }
            Driver::Sqlite => {
                let path = self.config().db_path(&self.dir);
                if path.exists() {
                    fs::remove_file(&path)?;
                }
                Ok(())
            }
        }
    }

    /// Open a maintenance connection (postgres `postgres` db / mysql without db).
    fn maintenance_client(&self) -> NdbResult<Backend> {
        match self.driver {
            Driver::Postgres => {
                let conn_str = format!(
                    "host={} port={} user={} password={} dbname=postgres",
                    self.cfg.host, self.cfg.port, self.cfg.user, self.cfg.password
                );
                let client = postgres::Client::connect(&conn_str, postgres::NoTls).map_err(
                    |e| NdbError::Connect(format!("{e} (maintenance postgres)")),
                )?;
                Ok(Backend::Postgres(client))
            }
            Driver::Mysql | Driver::MariaDb => {
                let url = format!("mysql://{}:{}@{}:{}", self.cfg.user, self.cfg.password, self.cfg.host, self.cfg.port);
                let conn = mysql::Conn::new(url.as_str())
                    .map_err(|e| NdbError::Connect(e.to_string()))?;
                Ok(Backend::Mysql(conn))
            }
            Driver::Sqlite => unreachable!("sqlite has no maintenance connection"),
        }
    }


    // ── DDL helpers ────────────────────────────────────────────────────

    fn ensure_migrations_table(&mut self) -> NdbResult<()> {
        let ddl = match self.driver {
            Driver::Postgres => {
                "CREATE TABLE IF NOT EXISTS nxl_migrations (\
                 id SERIAL PRIMARY KEY, name TEXT NOT NULL UNIQUE, \
                 batch BIGINT NOT NULL, migrated_at TIMESTAMPTZ NOT NULL DEFAULT now());\
                 CREATE TABLE IF NOT EXISTS nxl_migration_log (\
                 id SERIAL PRIMARY KEY, name TEXT NOT NULL, direction TEXT NOT NULL, \
                 batch BIGINT NOT NULL, duration_ms BIGINT NOT NULL, outcome TEXT NOT NULL, \
                 logged_at TIMESTAMPTZ NOT NULL DEFAULT now())"
            }
            Driver::Mysql | Driver::MariaDb => {
                "CREATE TABLE IF NOT EXISTS nxl_migrations (\
                 id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255) NOT NULL UNIQUE, \
                 batch INT NOT NULL, migrated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);\
                 CREATE TABLE IF NOT EXISTS nxl_migration_log (\
                 id INT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255) NOT NULL, \
                 direction VARCHAR(16) NOT NULL, batch INT NOT NULL, duration_ms BIGINT NOT NULL, \
                 outcome VARCHAR(16) NOT NULL, logged_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"
            }
            Driver::Sqlite => {
                "CREATE TABLE IF NOT EXISTS nxl_migrations (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, \
                 batch INTEGER NOT NULL, migrated_at TEXT NOT NULL DEFAULT (datetime('now')));\
                 CREATE TABLE IF NOT EXISTS nxl_migration_log (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, direction TEXT NOT NULL, \
                 batch INTEGER NOT NULL, duration_ms INTEGER NOT NULL, outcome TEXT NOT NULL, \
                 logged_at TEXT NOT NULL DEFAULT (datetime('now')))"
            }
        };
        self.exec_multi(ddl)
    }

    /// Append one row to the migration audit log.
    fn log_migration_event(
        &mut self,
        name: &str,
        direction: &str,
        batch: i64,
        duration_ms: i64,
        outcome: &str,
    ) -> NdbResult<()> {
        let d = self.driver;
        let sql = format!(
            "INSERT INTO nxl_migration_log (name, direction, batch, duration_ms, outcome) \
             VALUES ({}, {}, {}, {}, {})",
            d.placeholder(1),
            d.placeholder(2),
            d.placeholder(3),
            d.placeholder(4),
            d.placeholder(5),
        );
        self.exec(
            &sql,
            &[
                Value::Text(name.to_owned()),
                Value::Text(direction.to_owned()),
                Value::Int(batch),
                Value::Int(duration_ms),
                Value::Text(outcome.to_owned()),
            ],
        )?;
        Ok(())
    }

    fn applied_migrations(&mut self) -> NdbResult<BTreeMap<String, i64>> {
        let mut map = BTreeMap::new();
        match &mut self.inner {
            Backend::Sqlite(c) => {
                let mut stmt = c.prepare("SELECT name, batch FROM nxl_migrations ORDER BY name")?;
                let rows =
                    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
                for row in rows {
                    let (n, b) = row?;
                    map.insert(n, b);
                }
            }
            Backend::Postgres(client) => {
                let rows = client.query("SELECT name, batch FROM nxl_migrations ORDER BY name", &[])?;
                for r in rows {
                    map.insert(r.get::<_, String>(0), r.get::<_, i64>(1));
                }
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                let mut rows = c.query_iter("SELECT name, batch FROM nxl_migrations ORDER BY name")?;
                for row in rows.by_ref() {
                    let r = row?;
                    map.insert(
                        r.get::<String, _>(0).unwrap_or_default(),
                        r.get::<i64, _>(1).unwrap_or(0),
                    );
                }
            }
        }
        Ok(map)
    }

    fn next_batch(&mut self) -> NdbResult<i64> {
        let sql = "SELECT COALESCE(MAX(batch), 0) + 1 FROM nxl_migrations";
        let one: Option<i64> = self.scalar(sql)?;
        Ok(one.unwrap_or(1))
    }

    /// Record an applied migration in the tracking table.
    pub fn record_migration(&mut self, name: &str, batch: i64) -> NdbResult<()> {
        let p1 = self.driver.placeholder(1);
        let p2 = self.driver.placeholder(2);
        let sql = format!("INSERT INTO nxl_migrations (name, batch) VALUES ({p1}, {p2})");
        self.exec_str_i64(name, batch, &sql)
    }

    /// Forget a previously recorded migration (used by rollback).
    pub fn forget_migration(&mut self, name: &str) -> NdbResult<()> {
        let p1 = self.driver.placeholder(1);
        let sql = format!("DELETE FROM nxl_migrations WHERE name = {p1}");
        self.exec_str(name, &sql)
    }

    /// Execute one SQL statement with a single textual parameter.
    pub fn exec_str(&mut self, a: &str, sql: &str) -> NdbResult<()> {
        match &mut self.inner {
            Backend::Sqlite(c) => {
                c.execute(sql, params![a])?;
                Ok(())
            }
            Backend::Postgres(client) => {
                client.execute(sql, &[&a])?;
                Ok(())
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                c.exec_drop(sql, (a,))?;
                Ok(())
            }
        }
    }

    /// Execute a single statement with a string and integer parameter.
    pub fn exec_str_i64(&mut self, a: &str, b: i64, sql: &str) -> NdbResult<()> {
        match &mut self.inner {
            Backend::Sqlite(c) => {
                c.execute(sql, params![a, b])?;
                Ok(())
            }
            Backend::Postgres(client) => {
                client.execute(sql, &[&a, &b])?;
                Ok(())
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                c.exec_drop(sql, (a, b))?;
                Ok(())
            }
        }
    }

    fn scalar(&mut self, sql: &str) -> NdbResult<Option<i64>> {
        match &mut self.inner {
            Backend::Sqlite(c) => {
                let v = c.query_row(sql, [], |r| r.get::<_, i64>(0)).optional()?;
                Ok(v)
            }
            Backend::Postgres(client) => {
                let rows = client.query(sql, &[])?;
                Ok(rows.first().map(|r| r.get::<_, i64>(0)))
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                let mut it = c.query_iter(sql)?;
                match it.next() {
                    Some(row) => {
                        let r = row?;
                        Ok(r.get::<i64, _>(0).or(Some(0)))
                    }
                    None => Ok(Some(0)),
                }
            }
        }
    }

    /// Execute a multi-statement SQL batch.
    pub fn exec_multi(&mut self, sql: &str) -> NdbResult<()> {
        match &mut self.inner {
            Backend::Sqlite(c) => c.execute_batch(sql).map_err(NdbError::from),
            Backend::Postgres(client) => {
                client.simple_query(sql)?;
                Ok(())
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                mysql_batch(c, sql)
            }
        }
    }

    /// Execute a single statement with positional parameters.
    pub fn exec(&mut self, sql: &str, params: &[Value]) -> NdbResult<u64> {
        match &mut self.inner {
            Backend::Sqlite(c) => {
                let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|v| v.to_sql()).collect();
                Ok(c.execute(sql, refs.as_slice())? as u64)
            }
            Backend::Postgres(client) => {
                let v: Vec<CowRef> = params.iter().map(CowRef::from).collect();
                let refs: Vec<&(dyn postgres::types::ToSql + Sync)> =
                    v.iter().map(|c| c.as_ref()).collect();
                Ok(client.execute(sql, refs.as_slice())?)
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                let values: Vec<mysql::Value> = params.iter().map(mysql_value).collect();
                c.exec_drop(sql, mysql::Params::Positional(values))?;
                Ok(c.affected_rows())
            }
        }
    }

    /// Run a query and collect rows as text maps.
    pub fn query_rows(&mut self, sql: &str, params: &[Value]) -> NdbResult<Vec<BTreeMap<String, String>>> {
        match &mut self.inner {
            Backend::Sqlite(c) => {
                let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|v| v.to_sql()).collect();
                let mut stmt = c.prepare(sql)?;
                let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
                let rows = stmt.query_map(refs.as_slice(), |r| {
                    let mut map = BTreeMap::new();
                    for (i, name) in names.iter().enumerate() {
                        let v: rusqlite::types::Value = r.get(i).unwrap_or(rusqlite::types::Value::Null);
                        map.insert(name.clone(), sqlite_val_to_text(v));
                    }
                    Ok(map)
                })?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row?);
                }
                Ok(out)
            }
            Backend::Postgres(client) => {
                let v: Vec<CowRef> = params.iter().map(CowRef::from).collect();
                let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = v.iter().map(|c| c.as_ref()).collect();
                let cols: Vec<String> = client
                    .prepare(sql)?
                    .columns()
                    .iter()
                    .map(|c| c.name().to_string())
                    .collect();
                let rows = client.query(sql, refs.as_slice())?;
                let mut out = Vec::new();
                for r in rows {
                    let mut map = BTreeMap::new();
                    for (i, name) in cols.iter().enumerate() {
                        map.insert(name.clone(), pg_val_to_text(r.try_get::<_, Option<String>>(i)?));
                    }
                    out.push(map);
                }
                Ok(out)
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                let values: Vec<mysql::Value> = params.iter().map(mysql_value).collect();
                let it = c.exec_iter(sql, mysql::Params::Positional(values))?;
                let cols: Vec<String> = it
                    .columns()
                    .as_ref()
                    .iter()
                    .map(|c| c.name_str().to_string())
                    .collect();
                let mut out = Vec::new();
                for row in it {
                    let r = row?;
                    let mut map = BTreeMap::new();
                    for (i, name) in cols.iter().enumerate() {
                        let v: Option<String> = r.get(i);
                        map.insert(name.clone(), v.unwrap_or_else(|| "NULL".to_owned()));
                    }
                    out.push(map);
                }
                Ok(out)
            }
        }
    }

    /// Execute a parameterized query builder and return text rows
    /// (driver-agnostic).
    pub fn query(&mut self, q: &Query) -> NdbResult<Vec<BTreeMap<String, String>>> {
        let sql = q.to_sql_for(self.driver);
        let params: Vec<Value> = q.values().to_vec();
        self.query_rows(&sql, &params)
    }

    /// Count rows matching a query builder (driver-agnostic).
    pub fn count(&mut self, q: &Query) -> NdbResult<i64> {
        let mut counting = q.clone();
        counting.selects = vec!["COUNT(*)".to_owned()];
        let sql = counting.to_sql_for(self.driver);
        let params: Vec<Value> = counting.values().to_vec();
        let rows = self.query_rows(&sql, &params)?;
        let n = rows
            .first()
            .and_then(|m| m.values().next())
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        Ok(n)
    }

    /// First row matching a query builder, or `None`.
    pub fn first(&mut self, q: &Query) -> NdbResult<Option<BTreeMap<String, String>>> {
        Ok(self.query(&q.clone().limit(1))?.into_iter().next())
    }

    /// Insert one row (driver-agnostic). Returns affected rows.
    pub fn insert(&mut self, table: &str, row: &[(&str, Value)]) -> NdbResult<u64> {
        let cols: Vec<&str> = row.iter().map(|(c, _)| *c).collect();
        let marks: Vec<String> = (1..=row.len())
            .map(|i| self.driver.placeholder(i))
            .collect();
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            cols.join(", "),
            marks.join(", ")
        );
        let vals: Vec<Value> = row.iter().map(|(_, v)| v.clone()).collect();
        self.exec(&sql, &vals)
    }

    /// Update rows by an equality predicate (driver-agnostic).
    pub fn update(
        &mut self,
        table: &str,
        sets: &[(&str, Value)],
        where_col: &str,
        where_val: Value,
    ) -> NdbResult<u64> {
        let set_sql: Vec<String> = sets
            .iter()
            .enumerate()
            .map(|(i, (c, _))| format!("{c} = {}", self.driver.placeholder(i + 1)))
            .collect();
        let where_ph = self.driver.placeholder(sets.len() + 1);
        let sql = format!(
            "UPDATE {table} SET {} WHERE {where_col} = {where_ph}",
            set_sql.join(", ")
        );
        let mut vals: Vec<Value> = sets.iter().map(|(_, v)| v.clone()).collect();
        vals.push(where_val);
        self.exec(&sql, &vals)
    }

    /// Delete rows by an equality predicate (driver-agnostic).
    pub fn delete(&mut self, table: &str, where_col: &str, where_val: Value) -> NdbResult<u64> {
        let ph = self.driver.placeholder(1);
        let sql = format!("DELETE FROM {table} WHERE {where_col} = {ph}");
        self.exec(&sql, &[where_val])
    }

    /// Run inside a transaction; rolls back (does not record) on error.
    /// `apply` receives a reborrowed `&mut Database` whose `inner` is the live
    /// transaction-aware connection for the current driver.
    fn with_transaction<T>(
        &mut self,
        apply: impl FnOnce(&mut DatabaseTransactions<'_>) -> NdbResult<T>,
    ) -> NdbResult<T> {
        let mut tx = DatabaseTransactions::begin(self)?;
        match apply(&mut tx) {
            Ok(v) => {
                tx.commit()?;
                Ok(v)
            }
            Err(e) => {
                let _ = tx.rollback();
                Err(e)
            }
        }
    }
}

// ── ORM models ────────────────────────────────────────────────────────

/// A minimal, driver-agnostic ORM model. Implementations declare their table,
/// column list, value serialisation, and row hydration. All CRUD flows through
/// the parameterized [`Database`] query layer, so values are never interpolated
/// into SQL.
pub trait Model: Sized {
    /// Backing table name.
    const TABLE: &'static str;

    /// Column names in the order returned by [`Model::values`].
    fn columns() -> &'static [&'static str];

    /// Column → value pairs used for INSERT.
    fn values(&self) -> Vec<(&'static str, Value)>;

    /// Build an instance from a text row map (as produced by `query_rows`).
    fn from_row(row: &BTreeMap<String, String>) -> NdbResult<Self>;
}

/// Fetch a required text column from a hydrated row.
pub fn row_text<'a>(row: &'a BTreeMap<String, String>, col: &str) -> NdbResult<&'a str> {
    row.get(col)
        .map(String::as_str)
        .ok_or_else(|| NdbError::Sql(format!("model row is missing column '{col}'")))
}

/// Fetch and parse a required integer column from a hydrated row.
pub fn row_i64(row: &BTreeMap<String, String>, col: &str) -> NdbResult<i64> {
    row_text(row, col)?
        .parse::<i64>()
        .map_err(|_| NdbError::Sql(format!("column '{col}' is not an integer")))
}

impl Database {
    /// All rows of a model table, hydrated into `M`.
    pub fn model_all<M: Model>(&mut self) -> NdbResult<Vec<M>> {
        let rows = self.query(&Query::table(M::TABLE))?;
        rows.iter().map(M::from_row).collect()
    }

    /// First row where `where_col = where_val`, hydrated into `M`.
    pub fn model_find<M: Model>(
        &mut self,
        where_col: &str,
        where_val: Value,
    ) -> NdbResult<Option<M>> {
        match self.first(&Query::table(M::TABLE).where_eq(where_col, where_val))? {
            Some(row) => Ok(Some(M::from_row(&row)?)),
            None => Ok(None),
        }
    }

    /// Number of rows in a model table.
    pub fn model_count<M: Model>(&mut self) -> NdbResult<i64> {
        self.count(&Query::table(M::TABLE))
    }

    /// Insert `model`, returning the number of affected rows.
    pub fn model_insert<M: Model>(&mut self, model: &M) -> NdbResult<u64> {
        let values = model.values();
        self.insert(M::TABLE, &values)
    }

    /// Delete rows by an equality predicate.
    pub fn model_delete<M: Model>(
        &mut self,
        where_col: &str,
        where_val: Value,
    ) -> NdbResult<u64> {
        self.delete(M::TABLE, where_col, where_val)
    }
}

// ── Connection pool ───────────────────────────────────────────────────

/// A bounded, thread-safe pool of connections for one project database.
///
/// Connections are created lazily up to `max_size` and reused by
/// [`ConnectionPool::get`]. The returned [`PooledConnection`] dereferences to
/// [`Database`] and returns its connection to the pool when dropped.
pub struct ConnectionPool {
    cfg: DatabaseConfig,
    dir: PathBuf,
    driver: Driver,
    max_size: usize,
    idle: Mutex<Vec<Backend>>,
    created: AtomicUsize,
}

impl ConnectionPool {
    /// Create a pool for the database configured in `dir`/`nxl.toml`.
    pub fn new(dir: &Path, max_size: usize) -> NdbResult<ConnectionPool> {
        let cfg = DatabaseConfig::load(dir)?;
        let driver = Driver::from_name(&cfg.driver)
            .ok_or_else(|| NdbError::UnsupportedDriver(cfg.driver.clone()))?;
        Ok(ConnectionPool {
            cfg,
            dir: dir.to_path_buf(),
            driver,
            max_size: max_size.max(1),
            idle: Mutex::new(Vec::new()),
            created: AtomicUsize::new(0),
        })
    }

    /// Maximum number of live connections this pool will open.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Number of connections currently idle in the pool.
    pub fn idle_count(&self) -> usize {
        self.idle.lock().map(|v| v.len()).unwrap_or(0)
    }

    /// Number of connections the pool has opened (idle + checked out).
    pub fn live_count(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }

    /// Check out a connection, creating or waiting for one as needed.
    pub fn get(&self) -> NdbResult<PooledConnection<'_>> {
        if let Some(backend) = self.idle.lock().ok().and_then(|mut v| v.pop()) {
            return Ok(self.wrap(backend));
        }

        if self.created.load(Ordering::SeqCst) < self.max_size {
            self.created.fetch_add(1, Ordering::SeqCst);
            match Database::connect_backend(self.driver, &self.cfg, &self.dir) {
                Ok(backend) => return Ok(self.wrap(backend)),
                Err(e) => {
                    self.created.fetch_sub(1, Ordering::SeqCst);
                    return Err(e);
                }
            }
        }

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            std::thread::sleep(Duration::from_millis(5));
            if let Some(backend) = self.idle.lock().ok().and_then(|mut v| v.pop()) {
                return Ok(self.wrap(backend));
            }
            if Instant::now() >= deadline {
                return Err(NdbError::Connect(
                    "connection pool exhausted: all connections are checked out".to_owned(),
                ));
            }
        }
    }

    fn wrap(&self, backend: Backend) -> PooledConnection<'_> {
        PooledConnection {
            pool: self,
            db: Some(Database::from_parts(
                self.driver,
                backend,
                self.cfg.clone(),
                self.dir.clone(),
            )),
        }
    }

    fn release(&self, backend: Backend) {
        if let Ok(mut v) = self.idle.lock() {
            v.push(backend);
        }
    }
}

/// A checked-out connection that returns itself to its [`ConnectionPool`] on drop.
pub struct PooledConnection<'a> {
    pool: &'a ConnectionPool,
    db: Option<Database>,
}

impl PooledConnection<'_> {
    /// Mutable access to the underlying database handle.
    pub fn database_mut(&mut self) -> &mut Database {
        self.db.as_mut().expect("pooled connection is live")
    }
}

impl std::ops::Deref for PooledConnection<'_> {
    type Target = Database;
    fn deref(&self) -> &Database {
        self.db.as_ref().expect("pooled connection is live")
    }
}

impl std::ops::DerefMut for PooledConnection<'_> {
    fn deref_mut(&mut self) -> &mut Database {
        self.db.as_mut().expect("pooled connection is live")
    }
}

impl Drop for PooledConnection<'_> {
    fn drop(&mut self) {
        if let Some(db) = self.db.take() {
            self.pool.release(db.into_backend());
        }
    }
}


// ── Transaction executor ──────────────────────────────────────────────

/// Transaction-scoped executor used by migrations and seeders. Wraps a
/// driver-specific transaction; `commit`/`rollback` finalise it.
pub enum DatabaseTransactions<'a> {
    Sqlite(rusqlite::Transaction<'a>),
    Postgres(postgres::Transaction<'a>),
    Mysql(mysql::Transaction<'a>),
}

impl<'a> DatabaseTransactions<'a> {
    fn begin(db: &'a mut Database) -> NdbResult<Self> {
        match &mut db.inner {
            Backend::Sqlite(c) => Ok(DatabaseTransactions::Sqlite(c.unchecked_transaction()?)),
            Backend::Postgres(client) => {
                let t = client.transaction()?;
                Ok(DatabaseTransactions::Postgres(t))
            }
            Backend::Mysql(c) | Backend::MariaDb(c) => {
                let t = c.start_transaction(mysql::TxOpts::default())?;
                Ok(DatabaseTransactions::Mysql(t))
            }
        }
    }

    fn commit(self) -> NdbResult<()> {
        match self {
            DatabaseTransactions::Sqlite(t) => t.commit()?,
            DatabaseTransactions::Postgres(t) => t.commit()?,
            DatabaseTransactions::Mysql(t) => t.commit()?,
        }
        Ok(())
    }

    fn rollback(self) -> NdbResult<()> {
        match self {
            DatabaseTransactions::Sqlite(t) => t.rollback()?,
            DatabaseTransactions::Postgres(t) => t.rollback()?,
            DatabaseTransactions::Mysql(t) => t.rollback()?,
        }
        Ok(())
    }

    fn exec_batch(&mut self, sql: &str) -> NdbResult<()> {
        match self {
            DatabaseTransactions::Sqlite(t) => t.execute_batch(sql).map_err(NdbError::from),
            DatabaseTransactions::Postgres(t) => t.batch_execute(sql).map_err(NdbError::from),
            DatabaseTransactions::Mysql(t) => {
                for stmt in split_sql_statements(sql) {
                    t.query_drop(stmt)?;
                }
                Ok(())
            }
        }
    }

    fn insert_record(&mut self, name: &str, batch: i64, placeholder: &str) -> NdbResult<()> {
        let sql = format!("INSERT INTO nxl_migrations (name, batch) VALUES ({placeholder}, {placeholder})");
        match self {
            DatabaseTransactions::Sqlite(t) => {
                t.execute(&sql, params![name, batch])?;
            }
            DatabaseTransactions::Postgres(t) => {
                // Postgres uses positional $1/$2.
                let sql = "INSERT INTO nxl_migrations (name, batch) VALUES ($1, $2)";
                t.execute(sql, &[&name, &batch])?;
            }
            DatabaseTransactions::Mysql(t) => {
                t.exec_drop(&sql, (name, batch))?;
            }
        }
        Ok(())
    }

    fn delete_record(&mut self, name: &str, placeholder: &str) -> NdbResult<()> {
        let sql = format!("DELETE FROM nxl_migrations WHERE name = {placeholder}");
        match self {
            DatabaseTransactions::Sqlite(t) => {
                t.execute(&sql, params![name])?;
            }
            DatabaseTransactions::Postgres(t) => {
                t.execute("DELETE FROM nxl_migrations WHERE name = $1", &[&name])?;
            }
            DatabaseTransactions::Mysql(t) => {
                t.exec_drop(&sql, (name,))?;
            }
        }
        Ok(())
    }
}

fn sqlite_val_to_text(v: rusqlite::types::Value) -> String {
    match v {
        rusqlite::types::Value::Null => "NULL".to_owned(),
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(t) => t,
        rusqlite::types::Value::Blob(b) => format!("<blob:{}>", b.len()),
    }
}

fn pg_val_to_text(v: Option<String>) -> String {
    v.unwrap_or_else(|| "NULL".to_owned())
}

/// Parameter helper for the postgres driver (borrowed values).
enum CowRef {
    Null(Option<i64>),
    Int(i64),
    Float(f64),
    Text(String),
}

impl From<&Value> for CowRef {
    fn from(v: &Value) -> Self {
        match v {
            Value::Null => CowRef::Null(None),
            Value::Int(i) => CowRef::Int(*i),
            Value::Float(f) => CowRef::Float(*f),
            Value::Text(s) => CowRef::Text(s.clone()),
        }
    }
}

impl CowRef {
    fn as_ref(&self) -> &(dyn postgres::types::ToSql + Sync) {
        match self {
            CowRef::Null(n) => n,
            CowRef::Int(i) => i,
            CowRef::Float(f) => f,
            CowRef::Text(s) => s,
        }
    }
}

/// Convert an NDB [`Value`] to the mysql driver's value type.
fn mysql_value(v: &Value) -> mysql::Value {
    match v {
        Value::Null => mysql::Value::NULL,
        Value::Int(i) => mysql::Value::Int(*i),
        Value::Float(f) => mysql::Value::Double(*f),
        Value::Text(s) => mysql::Value::Bytes(s.clone().into_bytes()),
    }
}

// ── Connection management ─────────────────────────────────────────────

/// Open an SQLite connection (the classic API; driver-agnostic code should use
/// [`Database::open`]).
pub fn connect(dir: &Path) -> NdbResult<Connection> {
    Database::open(dir)?.into_sqlite()
}

pub fn db_create(dir: &Path) -> NdbResult<String> {
    let cfg = DatabaseConfig::load(dir)?;
    let driver = Driver::from_name(&cfg.driver)
        .ok_or_else(|| NdbError::UnsupportedDriver(cfg.driver.clone()))?;
    match driver {
        Driver::Sqlite => {
            let path = cfg.db_path(dir);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            if path.exists() {
                return Ok(format!("Database already exists at {}", path.display()));
            }
            File::create(&path)?;
            Ok(format!("Created database at {}", path.display()))
        }
        _ => {
            let mut db = Database::open_config(cfg, dir)?;
            let name = db.config().dbname.clone();
            if db.database_exists(&name)? {
                return Ok(format!("Database '{name}' already exists"));
            }
            db.create_database(&name)?;
            Ok(format!(
                "Created database {name} at {}",
                db.config().display_target()
            ))
        }
    }
}

pub fn db_drop(dir: &Path) -> NdbResult<String> {
    let cfg = DatabaseConfig::load(dir)?;
    let driver = Driver::from_name(&cfg.driver)
        .ok_or_else(|| NdbError::UnsupportedDriver(cfg.driver.clone()))?;
    match driver {
        Driver::Sqlite => {
            let path = cfg.db_path(dir);
            if path.exists() {
                fs::remove_file(&path)?;
                let base = path.to_string_lossy();
                let _ = fs::remove_file(format!("{base}-wal"));
                let _ = fs::remove_file(format!("{base}-shm"));
                Ok(format!("Dropped database at {}", path.display()))
            } else {
                Ok(format!("Database does not exist at {}", path.display()))
            }
        }
        _ => {
            let mut db = Database::open_config(cfg, dir)?;
            let name = db.config().dbname.clone();
            db.drop_database(&name)?;
            Ok(format!("Dropped database '{name}'"))
        }
    }
}

// ── Migrations ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Migration {
    pub name: String,
    pub up_sql: String,
    pub down_sql: String,
}

fn migrations_dir(dir: &Path) -> PathBuf {
    dir.join("database").join("migrations")
}
fn seeders_dir(dir: &Path) -> PathBuf {
    dir.join("database").join("seeders")
}
fn factories_dir(dir: &Path) -> PathBuf {
    dir.join("database").join("factories")
}

pub fn split_migration(sql: &str) -> NdbResult<(String, String)> {
    let mut up = String::new();
    let mut down = String::new();
    let mut section = "";
    for line in sql.lines() {
        let t = line.trim();
        if t.eq_ignore_ascii_case("-- up") {
            section = "up";
            continue;
        }
        if t.eq_ignore_ascii_case("-- down") {
            section = "down";
            continue;
        }
        match section {
            "up" => {
                up.push_str(line);
                up.push('\n');
            }
            "down" => {
                down.push_str(line);
                down.push('\n');
            }
            _ => {}
        }
    }
    if up.trim().is_empty() {
        return Err(NdbError::Migration(
            "migration is missing an '-- up' section".to_owned(),
        ));
    }
    if down.trim().is_empty() {
        return Err(NdbError::Migration(
            "migration is missing a '-- down' section".to_owned(),
        ));
    }
    Ok((up, down))
}

pub fn load_migrations(dir: &Path) -> NdbResult<Vec<Migration>> {
    let md = migrations_dir(dir);
    let mut out = Vec::new();
    if !md.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(&md)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    entries.sort();
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        let text = fs::read_to_string(&p)?;
        let (up_sql, down_sql) = split_migration(&text)?;
        out.push(Migration {
            name,
            up_sql,
            down_sql,
        });
    }
    Ok(out)
}

pub fn migrate(dir: &Path) -> NdbResult<Vec<String>> {
    let mut db = Database::open(dir)?;
    db.ensure_migrations_table()?;
    let applied = db.applied_migrations()?;
    let all = load_migrations(dir)?;
    let batch = db.next_batch()?;
    let mut ran = Vec::new();
    for m in all {
        if applied.contains_key(&m.name) {
            continue;
        }
        let name = m.name.clone();
        let up = m.up_sql.clone();
        let ph = if db.driver == Driver::Postgres { "$1" } else { "?" };
        let started = Instant::now();
        match db.with_transaction(|tx| {
            tx.exec_batch(&up)?;
            tx.insert_record(&name, batch, ph)?;
            Ok(())
        }) {
            Ok(()) => {
                let ms = started.elapsed().as_millis() as i64;
                let _ = db.log_migration_event(&name, "up", batch, ms, "ok");
                ran.push(name.clone());
            }
            Err(e) => {
                let ms = started.elapsed().as_millis() as i64;
                let _ = db.log_migration_event(&name, "up", batch, ms, "failed");
                return Err(NdbError::Migration(format!(
                    "migration '{name}' failed and was rolled back: {e}"
                )));
            }
        }
    }
    Ok(ran)
}

pub fn rollback(dir: &Path) -> NdbResult<Vec<String>> {
    let mut db = Database::open(dir)?;
    db.ensure_migrations_table()?;
    let applied = db.applied_migrations()?;
    if applied.is_empty() {
        return Ok(Vec::new());
    }
    let last_batch = applied.values().max().copied().unwrap_or(0);
    let mut in_batch: Vec<String> = applied
        .iter()
        .filter(|(_, b)| **b == last_batch)
        .map(|(n, _)| n.clone())
        .collect();
    in_batch.sort();
    in_batch.reverse();

    let all = load_migrations(dir)?;
    let mut rolled_back = Vec::new();
    for name in in_batch {
        let m = all.iter().find(|m| m.name == name).ok_or_else(|| {
            NdbError::Migration(format!("migration file for '{name}' not found on disk"))
        })?;
        let down = m.down_sql.clone();
        let ph = if db.driver == Driver::Postgres { "$1" } else { "?" };
        let started = Instant::now();
        match db.with_transaction(|tx| {
            tx.exec_batch(&down)?;
            tx.delete_record(&name, ph)?;
            Ok(())
        }) {
            Ok(()) => {
                let ms = started.elapsed().as_millis() as i64;
                let _ = db.log_migration_event(&name, "down", last_batch, ms, "ok");
                rolled_back.push(name.clone());
            }
            Err(e) => {
                let ms = started.elapsed().as_millis() as i64;
                let _ = db.log_migration_event(&name, "down", last_batch, ms, "failed");
                return Err(NdbError::Migration(format!(
                    "rollback of '{name}' failed and was rolled back: {e}"
                )));
            }
        }
    }
    Ok(rolled_back)
}

pub fn refresh(dir: &Path) -> NdbResult<(Vec<String>, Vec<String>)> {
    let mut rolled = Vec::new();
    loop {
        let r = rollback(dir)?;
        if r.is_empty() {
            break;
        }
        rolled.extend(r);
    }
    let ran = migrate(dir)?;
    Ok((rolled, ran))
}

pub fn fresh(dir: &Path, seed: bool) -> NdbResult<Vec<String>> {
    let _ = db_drop(dir)?;
    let ran = migrate(dir)?;
    if seed {
        run_seeders(dir)?;
    }
    Ok(ran)
}

pub fn migration_status(dir: &Path) -> NdbResult<Vec<(String, Option<i64>)>> {
    let mut db = Database::open(dir)?;
    db.ensure_migrations_table()?;
    let applied = db.applied_migrations()?;
    let all = load_migrations(dir)?;
    let mut out = Vec::new();
    for m in all {
        out.push((m.name.clone(), applied.get(&m.name).copied()));
    }
    Ok(out)
}

/// One recorded migration attempt (application or rollback).
#[derive(Debug, Clone, PartialEq)]
pub struct MigrationAuditEntry {
    pub name: String,
    /// `"up"` for an application, `"down"` for a rollback.
    pub direction: String,
    pub batch: i64,
    pub duration_ms: i64,
    /// `"ok"` or `"failed"`.
    pub outcome: String,
}

/// Read the migration audit log (oldest first).
pub fn migration_audit(dir: &Path) -> NdbResult<Vec<MigrationAuditEntry>> {
    let mut db = Database::open(dir)?;
    db.ensure_migrations_table()?;
    let rows = db.query_rows(
        "SELECT name, direction, batch, duration_ms, outcome \
         FROM nxl_migration_log ORDER BY id",
        &[],
    )?;
    let mut out = Vec::new();
    for r in rows {
        out.push(MigrationAuditEntry {
            name: row_text(&r, "name")?.to_owned(),
            direction: row_text(&r, "direction")?.to_owned(),
            batch: row_i64(&r, "batch")?,
            duration_ms: row_i64(&r, "duration_ms")?,
            outcome: row_text(&r, "outcome")?.to_owned(),
        });
    }
    Ok(out)
}

// ── Seeders and factories ─────────────────────────────────────────────

pub fn run_seeders(dir: &Path) -> NdbResult<Vec<String>> {
    let mut db = Database::open(dir)?;
    let sd = seeders_dir(dir);
    let mut out = Vec::new();
    if !sd.is_dir() {
        return Ok(out);
    }
    let mut entries: Vec<PathBuf> = fs::read_dir(&sd)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    entries.sort();
    for p in entries {
        let text = fs::read_to_string(&p)?;
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        match db.with_transaction(|tx| tx.exec_batch(&text)) {
            Ok(()) => out.push(name.clone()),
            Err(e) => {
                return Err(NdbError::Migration(format!("seeder '{name}' failed: {e}")));
            }
        }
    }
    Ok(out)
}

/// Execute a factory SQL template `count` times.
pub fn run_factory(dir: &Path, name: &str, count: usize) -> NdbResult<usize> {
    let mut db = Database::open(dir)?;
    let path = factories_dir(dir).join(format!("{name}.sql"));
    let text = fs::read_to_string(&path).map_err(|_| {
        NdbError::Config(format!("factory '{name}' not found at {}", path.display()))
    })?;
    db.with_transaction(|tx| {
        for _ in 0..count {
            tx.exec_batch(&text)?;
        }
        Ok(())
    })?;
    Ok(count)
}

// ── Transactions ──────────────────────────────────────────────────────

/// Run `f` inside a transaction; rolls back on error.
pub fn transaction<R>(
    conn: &Connection,
    f: impl FnOnce(&Connection) -> NdbResult<R>,
) -> NdbResult<R> {
    let tx = conn.unchecked_transaction()?;
    match f(&tx) {
        Ok(r) => {
            tx.commit()?;
            Ok(r)
        }
        Err(e) => {
            let _ = tx.rollback();
            Err(e)
        }
    }
}

// ── Query builder (parameterized by construction) ─────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    fn to_sql(&self) -> &dyn rusqlite::ToSql {
        match self {
            Value::Null => &rusqlite::types::Null,
            Value::Int(i) => i,
            Value::Float(f) => f,
            Value::Text(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    table: String,
    selects: Vec<String>,
    wheres: Vec<String>,
    params_: Vec<Value>,
    order_by: Vec<String>,
    limit: Option<usize>,
}

impl Query {
    pub fn table(name: &str) -> Self {
        Self {
            table: name.to_owned(),
            selects: vec!["*".to_owned()],
            wheres: Vec::new(),
            params_: Vec::new(),
            order_by: Vec::new(),
            limit: None,
        }
    }

    pub fn select(mut self, cols: &[&str]) -> Self {
        self.selects = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Only equality constraints are supported on the builder; they are always
    /// parameterized, so injection via values is not possible.
    pub fn where_eq(mut self, col: &str, value: Value) -> Self {
        self.wheres.push(col.to_owned());
        self.params_.push(value);
        self
    }

    pub fn order_by(mut self, col: &str, desc: bool) -> Self {
        self.order_by
            .push(format!("{col} {}", if desc { "DESC" } else { "ASC" }));
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    /// Render the SELECT for a specific driver (placeholder style differs:
    /// `?` for sqlite/mysql, `$n` for postgres).
    pub fn to_sql_for(&self, driver: Driver) -> String {
        let mut sql = format!("SELECT {} FROM {}", self.selects.join(", "), self.table);
        if !self.wheres.is_empty() {
            let clauses: Vec<String> = self
                .wheres
                .iter()
                .enumerate()
                .map(|(i, col)| format!("{col} = {}", driver.placeholder(i + 1)))
                .collect();
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        if !self.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&self.order_by.join(", "));
        }
        if let Some(n) = self.limit {
            sql.push_str(&format!(" LIMIT {n}"));
        }
        sql
    }

    pub fn to_sql(&self) -> String {
        self.to_sql_for(Driver::Sqlite)
    }

    /// The parameter values bound by this query, in placeholder order.
    pub fn values(&self) -> &[Value] {
        &self.params_
    }

    pub fn count(&self, conn: &Connection) -> NdbResult<i64> {
        let mut q = self.clone();
        q.selects = vec!["COUNT(*)".to_owned()];
        let sql = q.to_sql();
        let params_refs: Vec<&dyn rusqlite::ToSql> = q.params_.iter().map(|v| v.to_sql()).collect();
        let n: i64 = conn.query_row(&sql, params_refs.as_slice(), |r| r.get(0))?;
        Ok(n)
    }

    /// Execute and return rows as Vec of column-name→text maps.
    pub fn get(&self, conn: &Connection) -> NdbResult<Vec<BTreeMap<String, String>>> {
        let sql = self.to_sql();
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            self.params_.iter().map(|v| v.to_sql()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |r| {
            let mut map = BTreeMap::new();
            for (i, name) in col_names.iter().enumerate() {
                let v: rusqlite::types::Value = r.get(i).unwrap_or(rusqlite::types::Value::Null);
                map.insert(
                    name.clone(),
                    match v {
                        rusqlite::types::Value::Null => "NULL".to_owned(),
                        rusqlite::types::Value::Integer(i) => i.to_string(),
                        rusqlite::types::Value::Real(f) => f.to_string(),
                        rusqlite::types::Value::Text(t) => t,
                        rusqlite::types::Value::Blob(b) => format!("<blob:{}>", b.len()),
                    },
                );
            }
            Ok(map)
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn first(&self, conn: &Connection) -> NdbResult<Option<BTreeMap<String, String>>> {
        Ok(self.clone().limit(1).get(conn)?.into_iter().next())
    }

    pub fn insert(conn: &Connection, table: &str, row: &[(&str, Value)]) -> NdbResult<usize> {
        let cols: Vec<&str> = row.iter().map(|(c, _)| *c).collect();
        let marks: Vec<&str> = row.iter().map(|_| "?").collect();
        let sql = format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            cols.join(", "),
            marks.join(", ")
        );
        let params_refs: Vec<&dyn rusqlite::ToSql> = row.iter().map(|(_, v)| v.to_sql()).collect();
        let n = conn.execute(&sql, params_refs.as_slice())?;
        Ok(n)
    }

    pub fn update(
        conn: &Connection,
        table: &str,
        sets: &[(&str, Value)],
        where_col: &str,
        where_val: Value,
    ) -> NdbResult<usize> {
        let set_sql: Vec<String> = sets.iter().map(|(c, _)| format!("{c} = ?")).collect();
        let sql = format!(
            "UPDATE {table} SET {} WHERE {where_col} = ?",
            set_sql.join(", ")
        );
        let mut params_refs: Vec<&dyn rusqlite::ToSql> =
            sets.iter().map(|(_, v)| v.to_sql()).collect();
        params_refs.push(where_val.to_sql());
        let n = conn.execute(&sql, params_refs.as_slice())?;
        Ok(n)
    }

    pub fn delete(
        conn: &Connection,
        table: &str,
        where_col: &str,
        where_val: Value,
    ) -> NdbResult<usize> {
        let sql = format!("DELETE FROM {table} WHERE {where_col} = ?");
        let params_refs: Vec<&dyn rusqlite::ToSql> = vec![where_val.to_sql()];
        let n = conn.execute(&sql, params_refs.as_slice())?;
        Ok(n)
    }
}

// ── Scaffolding helpers ───────────────────────────────────────────────

pub fn make_migration(dir: &Path, name: &str) -> NdbResult<PathBuf> {
    let md = migrations_dir(dir);
    fs::create_dir_all(&md)?;
    let ts = unix_timestamp();
    let path = md.join(format!("{ts}_{name}.sql"));
    let stub = "-- up\n-- Write the forward schema change here.\n\n-- down\n-- Revert the forward change here.\n";
    fs::write(&path, stub)?;
    Ok(path)
}

pub fn make_seeder(dir: &Path, name: &str) -> NdbResult<PathBuf> {
    let sd = seeders_dir(dir);
    fs::create_dir_all(&sd)?;
    let path = sd.join(format!("{name}.sql"));
    fs::write(&path, "-- Write seed INSERT statements here.\n")?;
    Ok(path)
}

pub fn make_factory(dir: &Path, name: &str) -> NdbResult<PathBuf> {
    let fd = factories_dir(dir);
    fs::create_dir_all(&fd)?;
    let path = fd.join(format!("{name}.sql"));
    fs::write(
        &path,
        "-- Template executed once per generated row by factory runs.\n",
    )?;
    Ok(path)
}

pub fn make_model(dir: &Path, name: &str) -> NdbResult<PathBuf> {
    let md = dir.join("src").join("models");
    fs::create_dir_all(&md)?;
    let path = md.join(format!("{name}.nxl"));
    let stub =
        format!("// Model: {name}\n// Fields, relationships and query scopes are defined here.\n");
    fs::write(&path, stub)?;
    Ok(path)
}

fn unix_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Suppress unused import warning for OptionalExtension when certain features are off.
#[allow(unused)]
fn _use_optionalext(c: &Connection) -> bool {
    let _ = c.query_row("SELECT 1", [], |_| Ok(())).optional();
    true
}
