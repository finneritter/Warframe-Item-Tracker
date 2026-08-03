//! Developer-only: fill the DB with a believable random owned inventory so the
//! value-bearing screens (Dashboard, Inventory, Account, Sets, Arcanes) can be
//! exercised without a live game-client memory scan (the only real source, and
//! macOS can't scan at all). Isolated from the market/scan paths; gated behind
//! Developer mode in the UI. NOT a feature — purely a testing aid.
//!
//! `simulate` snapshots the DB first (recoverable), then REPLACES inventory +
//! account snapshot wholesale with random-but-reasonable data and sets the
//! warframe.market username to `random_user`. `clear` returns to an empty state.

use crate::db::{backup, catalog, Db};
use crate::error::{AppError, AppResult};
use crate::types::SimSummary;
use chrono::Utc;
use rusqlite::{params, Transaction};
use std::path::Path;

/// `fill` (a 1..=100 percentage) is how full the simulated account is — what
/// fraction of the tradeable catalog (and resource manifest) it owns. 100 ≈ a
/// maxed "lived-in" account that owns nearly everything; low values are sparse.
/// Take `pct`% of `total` (rounded up), at least 1 when anything exists.
fn scaled(total: i64, pct: i64) -> i64 {
    if total <= 0 {
        return 0;
    }
    (((total * pct) + 99) / 100).clamp(1, total)
}

/// Tiny xorshift64 PRNG seeded from the wall clock — enough for test fixtures,
/// and avoids pulling in the `rand` crate (the dep list is intentionally lean).
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or(1) as u64;
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Inclusive `lo..=hi`.
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        if hi <= lo {
            return lo;
        }
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
    /// True with probability `num/den`.
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.next_u64() % den < num
    }
    /// A prime set/part owned quantity. Usually one (you sell duplicates), but
    /// real accounts keep a few dupes and sometimes a pile of a heavily-farmed
    /// common part. Primes don't hoard the way mods do, so this stays modest.
    fn prime_qty(&mut self) -> i64 {
        match self.range(1, 100) {
            1..=75 => 1,
            76..=92 => self.range(2, 3),
            93..=98 => self.range(4, 8),
            _ => self.range(9, 20),
        }
    }

    /// A rank-0 mod stack — the bulk of a real account's item count. Most mods
    /// you hold a handful of; many commons pile into the dozens (kept for endo /
    /// ranking). Scaled by `fill` so a sparse account also has shallower hoards.
    fn mod_stack(&mut self, fill: i64) -> i64 {
        let base = match self.range(1, 100) {
            1..=60 => self.range(1, 4),
            61..=92 => self.range(5, 16),
            _ => self.range(25, 60),
        };
        (base * fill / 100).max(1)
    }

    /// A rank-0 arcane stack. You need 21 rank-0 copies to max one, so meaningful
    /// holds run deeper than primes. Scaled by `fill`.
    fn arcane_stack(&mut self, fill: i64) -> i64 {
        let base = match self.range(1, 100) {
            1..=60 => self.range(1, 8),
            _ => self.range(9, 21),
        };
        (base * fill / 100).max(1)
    }
}

/// Insert a rank-aware owned item (mod or arcane): a rank-0 stack of `base_qty`
/// plus, sometimes, a single maxed copy. Keeps `inventory_items.qty` equal to the
/// sum of its rank rows (the scan invariant). Returns the total owned qty written.
fn insert_rank_aware(
    tx: &Transaction,
    rng: &mut Rng,
    now: &str,
    slug: &str,
    max_rank: Option<i64>,
    base_qty: i64,
) -> AppResult<i64> {
    let max_rank = max_rank.unwrap_or(0).max(0);
    let mut ranks: Vec<(i64, i64)> = vec![(0, base_qty)];
    if max_rank > 0 && rng.chance(2, 5) {
        ranks.push((max_rank, rng.range(1, 2)));
    }
    let total: i64 = ranks.iter().map(|(_, q)| q).sum();
    tx.execute(
        "INSERT INTO inventory_items
            (slug, qty, first_added_at, last_modified_at, source, last_scan_qty)
         VALUES (?1, ?2, ?3, ?3, 'de_scan', ?2)",
        params![slug, total, now],
    )?;
    for (rank, qty) in ranks {
        tx.execute(
            "INSERT INTO inventory_ranks (slug, rank, qty) VALUES (?1, ?2, ?3)",
            params![slug, rank, qty],
        )?;
    }
    Ok(total)
}

/// Count tradeable catalog rows matching a fixed predicate (a compile-time
/// constant — never user input).
fn count_where(tx: &Transaction, predicate: &str) -> AppResult<i64> {
    let sql = format!("SELECT COUNT(*) FROM catalog_items WHERE is_tradeable = 1 AND {predicate}");
    Ok(tx.query_row(&sql, [], |r| r.get(0))?)
}

/// `SELECT slug, max_rank` for a random sample of catalog rows matching a fixed
/// category predicate. The predicate is a compile-time constant — never user input.
fn sample(tx: &Transaction, predicate: &str, limit: i64) -> AppResult<Vec<(String, Option<i64>)>> {
    let sql = format!(
        "SELECT slug, max_rank FROM catalog_items
         WHERE is_tradeable = 1 AND {predicate}
         ORDER BY RANDOM() LIMIT ?1"
    );
    let mut stmt = tx.prepare(&sql)?;
    let rows = stmt
        .query_map(params![limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Replace the current inventory + account snapshot with random test data sized
/// to `fill` (1..=100 % of the catalog owned). Backs the DB up first; returns a
/// summary for the toast.
pub fn simulate(db: &Db, db_path: &Path, fill: i64) -> AppResult<SimSummary> {
    let fill = fill.clamp(1, 100);
    // Guard: the catalog must be populated (and rank-backfilled) or there is
    // nothing to sample. Point the user at the existing refresh tools.
    if catalog::count(db)? == 0 || !catalog::has_any_max_rank(db)? {
        return Err(AppError::Invalid(
            "catalog is empty — run 'Update game data' (or 'Refresh catalog') first".into(),
        ));
    }

    // Recoverable: snapshot before we wipe anything.
    let backup_path = backup::snapshot(db, db_path, Some("pre-simulate"))?
        .display()
        .to_string();

    let mut rng = Rng::new();
    let platinum = rng.range(50, 2_500);
    let credits = rng.range(100_000, 8_000_000);

    let summary = db.with_mut(|conn| {
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();

        // Wipe everything we are about to replace (the "+ account snapshot"
        // tables too, so the Account screen shows only simulated data).
        for table in [
            "inventory_items",
            "inventory_ranks",
            "account_resources",
            "account_gear",
            "account_mastery",
            "account_lore_scans",
            "account_intrinsics",
            "account_syndicates",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }

        // --- Prime sets/parts: no ranks, just a (mostly single) owned qty. ---
        let prime_pred = "category IN ('warframe','weapon','set')";
        let n_prime = scaled(count_where(&tx, prime_pred)?, fill);
        let primes = sample(&tx, prime_pred, n_prime)?;
        for (slug, _) in &primes {
            let qty = rng.prime_qty();
            tx.execute(
                "INSERT INTO inventory_items
                    (slug, qty, first_added_at, last_modified_at, source, last_scan_qty)
                 VALUES (?1, ?2, ?3, ?3, 'de_scan', ?2)",
                params![slug, qty, now],
            )?;
        }

        // --- Mods: rank-aware and the bulk of the item count (deep duplicate
        // stacks, scaled by `fill`), so per-rank pricing + multi-copy haircut are
        // exercised hard. ---
        let n_mods_target = scaled(count_where(&tx, "category = 'mod'")?, fill);
        let mods = sample(&tx, "category = 'mod'", n_mods_target)?;
        let n_mods = mods.len() as i64;
        for (slug, max_rank) in &mods {
            let base = rng.mod_stack(fill);
            insert_rank_aware(&tx, &mut rng, &now, slug, *max_rank, base)?;
        }

        // --- Arcanes: rank-aware, deeper holds than primes (21 rank-0 = one max). ---
        let n_arc_target = scaled(count_where(&tx, "category = 'arcane'")?, fill);
        let arcanes = sample(&tx, "category = 'arcane'", n_arc_target)?;
        let n_arcanes = arcanes.len() as i64;
        for (slug, max_rank) in &arcanes {
            let base = rng.arcane_stack(fill);
            insert_rank_aware(&tx, &mut rng, &now, slug, *max_rank, base)?;
        }

        // --- Resources: realistic names/icons come from the bundled manifest. ---
        let res_total: i64 = tx.query_row(
            "SELECT COUNT(*) FROM item_manifest WHERE category = 'resource'",
            [],
            |r| r.get(0),
        )?;
        let n_res = scaled(res_total, fill);
        let resources: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT unique_name FROM item_manifest
                 WHERE category = 'resource' ORDER BY RANDOM() LIMIT ?1",
            )?;
            let out = stmt
                .query_map(params![n_res], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            out
        };
        for unique_name in &resources {
            tx.execute(
                "INSERT INTO account_resources (unique_name, kind, count)
                 VALUES (?1, 'resource', ?2)",
                params![unique_name, rng.range(100, 50_000)],
            )?;
        }

        // --- Account profile (single row) so the Account screen has data. ---
        tx.execute(
            "INSERT OR REPLACE INTO account_profile
                (id, scanned_at, mastery_rank, equipped_glyph, created, credits, platinum,
                 regal_aya, endo, trades_remaining, gifts_remaining, nodes_completed,
                 nodes_total, total_missions, daily_focus, focus_xp, login_streak)
             VALUES (1, ?1, ?2, NULL, '2018-03-24T00:00:00Z', ?3, ?4,
                 ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                now,
                rng.range(1, 34), // mastery_rank
                credits,
                platinum,
                rng.range(0, 30),         // regal_aya
                rng.range(0, 150_000),    // endo
                rng.range(0, 8),          // trades_remaining
                rng.range(0, 3),          // gifts_remaining
                rng.range(800, 1700),     // nodes_completed
                1700,                     // nodes_total
                rng.range(1_000, 40_000), // total_missions
                rng.range(0, 250_000),    // daily_focus
                rng.range(0, 5_000_000),  // focus_xp
                rng.range(1, 900),        // login_streak
            ],
        )?;

        // --- Profile name: `random_user`. The slug must be set too, or the first
        // sync would try to resolve it against the live API (commands::connected_slug).
        tx.execute(
            "INSERT INTO wfm_account (id, username, slug, status)
             VALUES (1, 'random_user', 'random-user', 'simulated')
             ON CONFLICT(id) DO UPDATE SET
                username = 'random_user', slug = 'random-user', status = 'simulated'",
            [],
        )?;

        // Headline "how many items does this account hold" — the sum of owned qty
        // across every tradeable row (what the user sees as "~15k items").
        let total_items: i64 = tx.query_row(
            "SELECT COALESCE(SUM(qty), 0) FROM inventory_items",
            [],
            |r| r.get(0),
        )?;

        tx.commit()?;
        Ok(SimSummary {
            items: primes.len() as i64,
            mods: n_mods,
            arcanes: n_arcanes,
            resources: resources.len() as i64,
            total_items,
            platinum,
            credits,
            backup_path,
        })
    })?;

    tracing::info!(
        items = summary.items,
        mods = summary.mods,
        arcanes = summary.arcanes,
        resources = summary.resources,
        total_items = summary.total_items,
        "simulated inventory written"
    );
    Ok(summary)
}

/// Return to an empty state: drop the simulated inventory + account snapshot and
/// the `random_user` name. Pairs with `simulate` (no full app wipe needed).
pub fn clear(db: &Db) -> AppResult<()> {
    db.with_mut(|conn| {
        let tx = conn.transaction()?;
        for table in [
            "inventory_items",
            "inventory_ranks",
            "account_resources",
            "account_gear",
            "account_mastery",
            "account_lore_scans",
            "account_intrinsics",
            "account_syndicates",
            "market_listings",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        tx.execute("DELETE FROM account_profile WHERE id = 1", [])?;
        tx.execute("DELETE FROM wfm_account WHERE id = 1", [])?;
        tx.commit()?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::{seed_item, test_db};

    fn seed_resource(db: &Db, unique_name: &str) {
        db.with(|c| {
            c.execute(
                "INSERT INTO item_manifest (unique_name, display_name, category)
                 VALUES (?1, ?1, 'resource')",
                params![unique_name],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn set_max_rank(db: &Db, slug: &str, max_rank: i64) {
        db.with(|c| {
            c.execute(
                "UPDATE catalog_items SET max_rank = ?2 WHERE slug = ?1",
                params![slug, max_rank],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn simulate_then_clear_round_trips() {
        let db = test_db("simulate");
        let path = std::env::temp_dir().join(format!("wfit-sim-{}.sqlite", std::process::id()));

        // A minimal catalog spanning the sampled categories + one ranked mod.
        seed_item(&db, "saryn_prime_chassis", "warframe", Some(40));
        seed_item(&db, "soma_prime_barrel", "weapon", Some(20));
        seed_item(&db, "saryn_prime_set", "set", Some(120));
        seed_item(&db, "serration", "mod", Some(8));
        set_max_rank(&db, "serration", 10);
        seed_item(&db, "energize", "arcane", Some(60));
        set_max_rank(&db, "energize", 5);
        seed_resource(&db, "/Lotus/Types/Items/Ferrite");

        let s = simulate(&db, &path, 100).unwrap();
        assert!(s.items >= 1 && s.mods >= 1 && s.arcanes >= 1 && s.resources >= 1);
        assert!(s.platinum > 0 && s.credits > 0);
        // total_items is the sum of owned qty, and must be >= the distinct row
        // count (deep stacks make it strictly larger on a real-sized catalog).
        assert!(s.total_items >= s.items + s.mods + s.arcanes);

        db.read(|c| {
            // inventory_items.qty must equal the sum of its rank rows (scan invariant).
            let bad: i64 = c.query_row(
                "SELECT COUNT(*) FROM inventory_items i
                 WHERE EXISTS (SELECT 1 FROM inventory_ranks r WHERE r.slug = i.slug)
                   AND i.qty <> (SELECT COALESCE(SUM(qty),0) FROM inventory_ranks r WHERE r.slug = i.slug)",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(bad, 0, "qty must equal sum of rank rows");
            let user: Option<String> = c.query_row(
                "SELECT username FROM wfm_account WHERE id = 1",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(user.as_deref(), Some("random_user"));
            let prof: i64 =
                c.query_row("SELECT COUNT(*) FROM account_profile", [], |r| r.get(0))?;
            assert_eq!(prof, 1);
            Ok(())
        })
        .unwrap();

        clear(&db).unwrap();
        db.read(|c| {
            for table in ["inventory_items", "inventory_ranks", "account_resources"] {
                let n: i64 =
                    c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?;
                assert_eq!(n, 0, "{table} not cleared");
            }
            let prof: i64 =
                c.query_row("SELECT COUNT(*) FROM account_profile", [], |r| r.get(0))?;
            assert_eq!(prof, 0, "profile not cleared");
            Ok(())
        })
        .unwrap();

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn scaled_covers_the_range() {
        assert_eq!(scaled(0, 100), 0); // nothing to own
        assert_eq!(scaled(200, 100), 200); // full account owns everything
        assert_eq!(scaled(200, 50), 100); // half
        assert_eq!(scaled(200, 1), 2); // ceil of 2.0
        assert_eq!(scaled(10, 1), 1); // always at least one when some exist
    }

    #[test]
    fn stacks_collapse_to_one_at_minimum_fill() {
        // At fill=1 every base stack (<= ~60) scales to 0 and floors to 1, so a
        // sparse account holds shallow stacks regardless of the rarity roll.
        let mut rng = Rng::new();
        for _ in 0..200 {
            assert_eq!(rng.mod_stack(1), 1);
            assert_eq!(rng.arcane_stack(1), 1);
            assert!(rng.mod_stack(100) >= 1);
            assert!(rng.arcane_stack(100) >= 1);
            assert!(rng.prime_qty() >= 1);
        }
    }

    #[test]
    fn full_fill_lands_near_a_real_account_size() {
        // Seed a catalog matching the live tradeable counts (warframe 200, weapon
        // 607, set 231, mod 1388, arcane 170) so total item count is realistic.
        let db = test_db("simulate-size");
        let path = std::env::temp_dir().join(format!("wfit-size-{}.sqlite", std::process::id()));
        let mut n = 0;
        let mut seed = |cat: &str, count: usize, max_rank: Option<i64>| {
            for _ in 0..count {
                n += 1;
                seed_item(&db, &format!("{cat}_{n}"), cat, Some(10));
                if let Some(mr) = max_rank {
                    set_max_rank(&db, &format!("{cat}_{n}"), mr);
                }
            }
        };
        seed("warframe", 200, None);
        seed("weapon", 607, None);
        seed("set", 231, None);
        seed("mod", 1388, Some(10));
        seed("arcane", 170, Some(5));

        let s = simulate(&db, &path, 100).unwrap();
        // ~15k is the target; RNG + the rarity tiers spread it, so assert a wide
        // but realistic band (and that it's an order of magnitude past row count).
        eprintln!("full-fill total_items = {}", s.total_items);
        assert!(
            (9_000..=24_000).contains(&s.total_items),
            "total_items {} outside realistic band",
            s.total_items
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn simulate_errors_on_empty_catalog() {
        let db = test_db("simulate-empty");
        let path = std::env::temp_dir().join("wfit-sim-empty.sqlite");
        assert!(simulate(&db, &path, 50).is_err());
    }
}
