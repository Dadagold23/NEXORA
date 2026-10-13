# NDB — Database (P14)

Support four drivers: `sqlite` (bundled `rusqlite`), `postgres`, `mysql`, and `mariadb` (via the `postgres`/`mysql` crates). An unknown driver returns an explicit `error[NDB14005] ... not supported`.

`nxl.toml` (sqlite):
```toml
[database]
driver = "sqlite"
path = "database/database.sqlite"
```
Server drivers read `host`, `port`, `user`, `password`, and `dbname` from the same `[database]` section.

Migrations: `database/migrations/<name>.sql` with `-- up` / `-- down` sections. Tracked in `nxl_migrations` (name, batch, migrated_at). Each migration and seeder runs in its own transaction; failures roll back and are not recorded.

Commands:
```bash
nxl db:create       # create database file
nxl db:drop         # delete it
nxl db:migrate      # pending migrations → single batch
nxl db:rollback     # last batch only
nxl db:refresh      # rollback all, re-migrate
nxl db:fresh        # drop, migrate ([--seed])
nxl db:seed         # database/seeders/*.sql in order
nxl db:status       # show migration status (applied/pending)
nxl db:audit        # show the migration audit log
nxl make:migration  create_users   # database/migrations/<ts>_create_users.sql
nxl make:seeder     users
nxl make:factory    user
nxl make:model      User           # src/models/User.nxl stub
```

Query builder: `Query::table("t").select(&[..]).where_eq(..).order_by(..).limit(n)` renders per-driver placeholders (`?` or `$n`). Driver-agnostic execution: `Database::{query, count, first, insert, update, delete}`. The sqlite helpers `Query::{get, first, count, insert, update, delete}(&Connection)` remain for backward compatibility. All values are parameterised. Transitive helper: `ndb::transaction(conn, |tx| ...)` (sqlite) or `Database::with_transaction`.

ORM models: implement the `Model` trait (`TABLE`, `columns`, `values`, `from_row`) and use `Database::{model_all, model_find, model_count, model_insert, model_delete}`. `row_text` / `row_i64` hydrate typed fields.

Connection pooling: `ConnectionPool::new(&dir, max)` lazily opens up to `max` connections and reuses them; `pool.get()` returns a `PooledConnection` that dereferences to `Database` and returns to the pool on drop.

Factories: `run_factory(dir, "user", 50)` executes `database/factories/user.sql` 50 times in one transaction.

Audit log: every apply/rollback attempt is recorded in `nxl_migration_log` (name, direction, batch, duration_ms, outcome) and readable via `ndb::migration_audit(&dir)` or `nxl db:audit`.
