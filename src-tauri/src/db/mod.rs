use crate::error::{AppError, AppResult};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use std::path::Path;
use std::sync::Arc;

pub mod account;
pub mod arcanes;
pub mod backup;
pub mod buylist;
pub mod catalog;
pub mod coda;
pub mod gamescan;
pub mod inventory;
pub mod meta;
pub mod nightwave_acts;
pub mod notifications;
pub mod prices;
pub mod recommend;
pub mod relic_data;
pub mod relics;
pub mod rivens;
pub mod sales;
pub mod sets;
pub mod settings;
pub mod simulate;
pub mod trends;
pub mod vault;
pub mod vendor;
pub mod vendor_checkoff;
pub mod wanted;
pub mod watchlist;
pub mod wfm;

/// The schema version the MIGRATIONS list produces (`PRAGMA user_version` after
/// `to_latest`). Bump in lockstep when appending a migration — the pre-migration
/// backup gate and its test both pin it.
pub const SCHEMA_VERSION: i64 = 22;

// Append future migrations here; never edit a shipped one.
static MIGRATIONS: Lazy<Migrations<'static>> = Lazy::new(|| {
    Migrations::new(vec![
        M::up(include_str!("../../migrations/0001_init.sql")),
        M::up(include_str!("../../migrations/0002_ohlc.sql")),
        M::up(include_str!("../../migrations/0003_game_import.sql")),
        M::up(include_str!("../../migrations/0004_ranks.sql")),
        M::up(include_str!("../../migrations/0005_orders.sql")),
        M::up(include_str!("../../migrations/0006_buy_orders.sql")),
        M::up(include_str!("../../migrations/0007_mod_rarity.sql")),
        M::up(include_str!("../../migrations/0008_vault_status.sql")),
        M::up(include_str!("../../migrations/0009_perf_indexes.sql")),
        M::up(include_str!("../../migrations/0010_order_fetch_meta.sql")),
        M::up(include_str!("../../migrations/0011_owned_relics.sql")),
        M::up(include_str!("../../migrations/0012_relic_data.sql")),
        M::up(include_str!("../../migrations/0013_account.sql")),
        M::up(include_str!("../../migrations/0014_rivens.sql")),
        M::up(include_str!(
            "../../migrations/0015_riven_search_thresholds.sql"
        )),
        M::up(include_str!("../../migrations/0016_app_notifications.sql")),
        M::up(include_str!("../../migrations/0017_vendor_checkoff.sql")),
        M::up(include_str!("../../migrations/0018_relic_prefs.sql")),
        M::up(include_str!(
            "../../migrations/0019_nightwave_completions.sql"
        )),
        M::up(include_str!("../../migrations/0020_coda_rotation.sql")),
        M::up(include_str!(
            "../../migrations/0021_account_gear_mastery_class.sql"
        )),
        M::up(include_str!("../../migrations/0022_wfm_account_slug.sql")),
    ])
});

/// Read-only connection pool size. WAL lets these read concurrently with the
/// single writer, so a long market sync no longer blocks UI reads.
const READ_POOL_SIZE: u32 = 4;

/// Performance/correctness pragmas applied to every connection (writer + readers).
/// `journal_mode`/`synchronous` are set once on the writer (WAL is persisted DB
/// state); these are per-connection and must be re-applied to each pool member.
fn tune(conn: &Connection) -> rusqlite::Result<()> {
    // Wait (don't error with SQLITE_BUSY) when briefly contending the writer's commit.
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "cache_size", -65536)?; // 64 MB page cache
    conn.pragma_update(None, "mmap_size", 268_435_456_i64)?; // 256 MB memory-mapped I/O
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

#[derive(Clone)]
pub struct Db {
    /// The single writer. WAL allows only one writer; serializing through this
    /// mutex keeps writes (and the few read paths that still use `with`) correct.
    inner: Arc<Mutex<Connection>>,
    /// Read-only connections for hot UI read paths (`read`). Isolated from the
    /// writer so a sync holding `inner` doesn't freeze reads.
    readers: Pool<SqliteConnectionManager>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        tune(&conn)?;

        // Existing DB about to be migrated → snapshot it first, so a botched
        // migration can never be the thing that loses the inventory/sales data.
        // A failed snapshot is logged, not fatal: a backup hiccup must not
        // block startup on a healthy DB.
        let user_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let mut pre_migration: Option<std::path::PathBuf> = None;
        if user_version > 0 && user_version < SCHEMA_VERSION {
            match backup::snapshot_conn(&conn, &backup::backups_dir(path), Some("pre-migration")) {
                Ok(p) => {
                    tracing::info!(path = %p.display(), "pre-migration snapshot saved");
                    pre_migration = Some(p);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "pre-migration snapshot failed; migrating anyway")
                }
            }
        }
        MIGRATIONS
            .to_latest(&mut conn)
            .map_err(|e| match &pre_migration {
                Some(p) => AppError::Other(format!(
                    "migration failed: {e}. A pre-migration snapshot was saved to {}",
                    p.display()
                )),
                None => AppError::Migration(e),
            })?;

        // Build the read pool AFTER migrations so the schema is in place. Each
        // reader is `query_only` so an accidental write routed to `read` errors
        // loudly instead of corrupting state.
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            c.pragma_update(None, "synchronous", "NORMAL")?;
            tune(c)?;
            c.pragma_update(None, "query_only", "ON")?;
            Ok(())
        });
        let readers = Pool::builder()
            .max_size(READ_POOL_SIZE)
            .build(manager)
            .map_err(|e| AppError::Other(format!("read pool init: {e}")))?;

        Ok(Self {
            inner: Arc::new(Mutex::new(conn)),
            readers,
        })
    }

    /// Run a closure on the writer connection (shared read/write lock). Use for
    /// writes and the few legacy read paths that have not moved to `read`.
    pub fn with<R>(&self, f: impl FnOnce(&Connection) -> AppResult<R>) -> AppResult<R> {
        #[cfg(feature = "dev-dashboard")]
        {
            let t0 = std::time::Instant::now();
            let conn = self.inner.lock();
            let waited = t0.elapsed();
            let t1 = std::time::Instant::now();
            let r = f(&conn);
            crate::devtools::metrics::record_db_writer(waited, t1.elapsed());
            r
        }
        #[cfg(not(feature = "dev-dashboard"))]
        {
            let conn = self.inner.lock();
            f(&conn)
        }
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(&mut Connection) -> AppResult<R>) -> AppResult<R> {
        #[cfg(feature = "dev-dashboard")]
        {
            let t0 = std::time::Instant::now();
            let mut conn = self.inner.lock();
            let waited = t0.elapsed();
            // Dev-only artificial writer contention, if a fault is armed.
            crate::devtools::faults::db_hold();
            let t1 = std::time::Instant::now();
            let r = f(&mut conn);
            crate::devtools::metrics::record_db_writer(waited, t1.elapsed());
            r
        }
        #[cfg(not(feature = "dev-dashboard"))]
        {
            let mut conn = self.inner.lock();
            f(&mut conn)
        }
    }

    /// Run a read-only closure on a pooled connection — never blocks on the
    /// writer mutex, so UI reads stay responsive during a market sync. The
    /// connection is `query_only`; do not attempt writes here.
    pub fn read<R>(&self, f: impl FnOnce(&Connection) -> AppResult<R>) -> AppResult<R> {
        #[cfg(feature = "dev-dashboard")]
        {
            let t0 = std::time::Instant::now();
            let conn = self
                .readers
                .get()
                .map_err(|e| AppError::Other(format!("read pool: {e}")))?;
            let waited = t0.elapsed();
            let t1 = std::time::Instant::now();
            let r = f(&conn);
            crate::devtools::metrics::record_db_read(waited, t1.elapsed());
            r
        }
        #[cfg(not(feature = "dev-dashboard"))]
        {
            let conn = self
                .readers
                .get()
                .map_err(|e| AppError::Other(format!("read pool: {e}")))?;
            f(&conn)
        }
    }
}

/// Test fixture: a fully-migrated Db on a fresh temp file, with a helper to
/// seed catalog rows (most CRUD paths require the slug to exist there).
#[cfg(test)]
pub(crate) mod testutil {
    use super::Db;

    pub fn test_db(tag: &str) -> Db {
        let path = std::env::temp_dir().join(format!(
            "wfit-test-{tag}-{}-{}.sqlite",
            std::process::id(),
            std::thread::current()
                .name()
                .unwrap_or("t")
                .replace("::", "-")
        ));
        let _ = std::fs::remove_file(&path);
        Db::open(&path).unwrap()
    }

    pub fn seed_item(db: &Db, slug: &str, category: &str, median: Option<i64>) {
        db.with(|c| {
            c.execute(
                "INSERT INTO catalog_items (slug, display_name, part_type, category)
                 VALUES (?1, ?1, 'Part', ?2)",
                rusqlite::params![slug, category],
            )?;
            if let Some(m) = median {
                c.execute(
                    "INSERT INTO price_cache (slug, median_plat, trend, fetched_at, expires_at)
                     VALUES (?1, ?2, 'flat', '2026-01-01', '2099-01-01')",
                    rusqlite::params![slug, m],
                )?;
            }
            Ok(())
        })
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    // The headline Phase-1 guarantee: a read does NOT block while a write holds the
    // writer connection. A `with_mut` closure sits on the lock for 600ms; a `read`
    // on the pool must still return promptly (WAL = concurrent readers).
    #[test]
    fn read_does_not_block_on_a_held_write() {
        let dir = std::env::temp_dir().join(format!("wfit-readpool-{}", std::process::id()));
        let _ = std::fs::remove_file(&dir);
        let db = Db::open(&dir).unwrap();
        db.with_mut(|c| {
            c.execute("CREATE TABLE t (x INTEGER)", [])
                .map_err(Into::into)
        })
        .unwrap();

        let writer = db.clone();
        let handle = std::thread::spawn(move || {
            writer
                .with_mut(|c| {
                    c.execute("INSERT INTO t VALUES (1)", [])?;
                    std::thread::sleep(Duration::from_millis(600)); // hold the writer lock
                    Ok(())
                })
                .unwrap();
        });
        std::thread::sleep(Duration::from_millis(50)); // ensure the write lock is held

        let start = Instant::now();
        let n: i64 = db
            .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM t", [], |r| r.get(0))?))
            .unwrap();
        let elapsed = start.elapsed();
        handle.join().unwrap();

        assert!(
            elapsed < Duration::from_millis(300),
            "read blocked on the writer for {elapsed:?}"
        );
        assert!(n >= 0);
        let _ = std::fs::remove_file(&dir);
    }

    // SCHEMA_VERSION must track the migration list — the pre-migration backup
    // gate compares user_version against it. Forgetting the bump would silently
    // skip the snapshot for the new migration.
    #[test]
    fn schema_version_matches_migrations() {
        let db = testutil::test_db("schemaver");
        let v: i64 = db
            .with(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(
            v, SCHEMA_VERSION,
            "bump SCHEMA_VERSION with the migration list"
        );
    }

    // Upgrade-path guard: applying every migration in sequence must succeed on a
    // fresh file (the install path) AND when re-opened later (the upgrade path,
    // i.e. what the two real machines do on every launch) — with user data
    // surviving the re-open. Catches a future 0011+ that breaks on existing DBs.
    #[test]
    fn migrations_apply_and_reapply_with_data_intact() {
        let path = std::env::temp_dir().join(format!("wfit-migrate-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);

        // Fresh install: full chain applies, schema is present.
        {
            let db = Db::open(&path).unwrap();
            db.with(|c| {
                let n: i64 = c.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |r| r.get(0),
                )?;
                assert!(n > 10, "expected the full schema, got {n} tables");
                // Seed a user-data row (inventory is NOT a rebuildable cache).
                c.execute_batch(
                    "INSERT INTO catalog_items (slug, display_name, part_type, category)
                     VALUES ('probe_item', 'Probe Item', 'Part', 'mod');
                     INSERT INTO inventory_items (slug, qty, first_added_at, last_modified_at, source)
                     VALUES ('probe_item', 3, '2026-01-01', '2026-01-01', 'manual');",
                )?;
                Ok(())
            })
            .unwrap();
        }

        // Re-open (the upgrade path): migrations re-run idempotently, data intact.
        {
            let db = Db::open(&path).unwrap();
            let qty: i64 = db
                .read(|c| {
                    Ok(c.query_row(
                        "SELECT qty FROM inventory_items WHERE slug='probe_item'",
                        [],
                        |r| r.get(0),
                    )?)
                })
                .unwrap();
            assert_eq!(qty, 3, "user data must survive a re-open/migration run");
        }
        let _ = std::fs::remove_file(&path);
    }
}
