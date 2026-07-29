//! Game-inventory-import state + merge. The single-row `game_scan_state` holds
//! consent + last-scan bookkeeping; `merge_from_scan` is the only writer into
//! `inventory_items` from a scan (provenance `source = 'de_scan'`).
//!
//! The game session (accountId/nonce) is NEVER persisted here — only the account
//! id, to detect "a different account was scanned".

use crate::db::Db;
use crate::error::AppResult;
use crate::gamescan::ScanItem;
use crate::types::{RankQty, ScanApply, ScanDiffRow};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub const SOURCE_DE_SCAN: &str = "de_scan";

/// Raw persisted feature state (consent + last-scan bookkeeping).
#[derive(Debug, Clone, Default)]
pub struct ScanState {
    pub consent_at: Option<String>,
    pub last_scan_at: Option<String>,
    #[allow(dead_code)] // surfaced in B2 to warn on a different-account scan
    pub last_account_id: Option<String>,
    pub auto_sync: bool,
}

fn ensure_row(c: &Connection) -> AppResult<()> {
    c.execute("INSERT OR IGNORE INTO game_scan_state (id) VALUES (1)", [])?;
    Ok(())
}

pub fn get_state(db: &Db) -> AppResult<ScanState> {
    db.with(|c| {
        ensure_row(c)?;
        let st = c.query_row(
            "SELECT consent_at, last_scan_at, last_account_id, auto_sync
             FROM game_scan_state WHERE id = 1",
            [],
            |r| {
                Ok(ScanState {
                    consent_at: r.get(0)?,
                    last_scan_at: r.get(1)?,
                    last_account_id: r.get(2)?,
                    auto_sync: r.get::<_, i64>(3)? != 0,
                })
            },
        )?;
        Ok(st)
    })
}

pub fn is_consented(db: &Db) -> AppResult<bool> {
    Ok(get_state(db)?.consent_at.is_some())
}

pub fn set_consent(db: &Db) -> AppResult<()> {
    db.with(|c| {
        ensure_row(c)?;
        c.execute(
            "UPDATE game_scan_state SET consent_at = ?1 WHERE id = 1",
            params![Utc::now().to_rfc3339()],
        )?;
        Ok(())
    })
}

pub fn clear_consent(db: &Db) -> AppResult<()> {
    db.with(|c| {
        ensure_row(c)?;
        c.execute(
            "UPDATE game_scan_state SET consent_at = NULL WHERE id = 1",
            [],
        )?;
        Ok(())
    })
}

pub fn record_scan(db: &Db, account_id: Option<&str>) -> AppResult<()> {
    db.with(|c| {
        ensure_row(c)?;
        c.execute(
            "UPDATE game_scan_state SET last_scan_at = ?1, last_account_id = ?2 WHERE id = 1",
            params![Utc::now().to_rfc3339(), account_id],
        )?;
        Ok(())
    })
}

/// `game_ref` (DE uniqueName) → catalog slug. The join the scan mapping rides on.
/// A `game_ref` can carry two catalog rows, so the ORDER BY makes the last write
/// win the same slug the Arsenal picks (see `catalog::PICK_SLUG_FOR_GAME_REF`) —
/// without it the mapping was whichever row the scan happened to read last.
pub fn game_ref_to_slug(db: &Db) -> AppResult<HashMap<String, String>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT game_ref, slug FROM catalog_items WHERE game_ref IS NOT NULL
              ORDER BY (substr(slug, -4) = '_set') ASC, slug DESC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut m = HashMap::new();
        for r in rows {
            let (gref, slug) = r?;
            m.insert(gref, slug);
        }
        Ok(m)
    })
}

/// Current inventory as slug → (qty, source). Used to diff a scan against reality.
fn current_inventory(c: &Connection) -> AppResult<HashMap<String, (i64, String)>> {
    let mut stmt = c.prepare("SELECT slug, qty, source FROM inventory_items")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            (r.get::<_, i64>(1)?, r.get::<_, String>(2)?),
        ))
    })?;
    let mut m = HashMap::new();
    for r in rows {
        let (slug, v) = r?;
        m.insert(slug, v);
    }
    Ok(m)
}

/// Compute the reviewable diff of a resolved scan against current inventory (§5):
/// - present in scan, absent in inventory     → 'added'
/// - present in both with a different quantity → 'changed'
/// - equal quantity                            → no row (nothing to do)
/// - a prior `de_scan` row absent from this scan → 'removed' (scan_qty 0).
///   `manual` / `wfm_import` rows are NEVER reported as removed — only de_scan rows.
pub fn diff(db: &Db, scan: &[ScanItem]) -> AppResult<Vec<ScanDiffRow>> {
    // Aggregate the per-(slug,rank) scan into a per-slug total + rank breakdown.
    let mut totals: HashMap<String, i64> = HashMap::new();
    let mut breakdown: HashMap<String, Vec<RankQty>> = HashMap::new();
    for s in scan {
        *totals.entry(s.slug.clone()).or_insert(0) += s.qty;
        if let Some(rank) = s.rank {
            breakdown
                .entry(s.slug.clone())
                .or_default()
                .push(RankQty { rank, qty: s.qty });
        }
    }

    db.with(|c| {
        let current = current_inventory(c)?;
        let stored_ranks = current_ranks(c)?;
        let mut out: Vec<ScanDiffRow> = Vec::new();

        // added / changed
        for (slug, &scan_qty) in &totals {
            let (current_qty, source) = current
                .get(slug)
                .map(|(q, src)| (*q, src.clone()))
                .unwrap_or((0, String::new()));

            // The scan's rank breakdown, normalized for comparison + the row.
            let mut ranks = breakdown.get(slug).cloned().unwrap_or_default();
            ranks.sort_by_key(|r| r.rank);
            let scan_bd: Vec<(i64, i64)> = ranks.iter().map(|r| (r.rank, r.qty)).collect();
            // A row is "changed" if its total OR its rank breakdown differs from what's
            // stored — so already-imported mods/arcanes still get their per-rank detail
            // written even when the total is unchanged.
            let bd_changed = stored_ranks.get(slug).map(Vec::as_slice).unwrap_or(&[]) != scan_bd;

            let status = if !current.contains_key(slug) {
                "added"
            } else if current_qty != scan_qty || bd_changed {
                "changed"
            } else {
                continue; // unchanged
            };
            let (display_name, part_type) = catalog_label(c, slug)?;
            out.push(ScanDiffRow {
                slug: slug.clone(),
                display_name,
                part_type,
                status: status.to_string(),
                scan_qty,
                current_qty,
                source,
                ranks,
            });
        }

        // removed: de_scan rows that this scan no longer reports
        for (slug, (qty, source)) in &current {
            if source == SOURCE_DE_SCAN && !totals.contains_key(slug.as_str()) {
                let (display_name, part_type) = catalog_label(c, slug)?;
                out.push(ScanDiffRow {
                    slug: slug.clone(),
                    display_name,
                    part_type,
                    status: "removed".to_string(),
                    scan_qty: 0,
                    current_qty: *qty,
                    source: source.clone(),
                    ranks: Vec::new(),
                });
            }
        }

        out.sort_by(|a, b| {
            a.status
                .cmp(&b.status)
                .then_with(|| a.display_name.cmp(&b.display_name))
        });
        Ok(out)
    })
}

/// Stored rank breakdown per slug (sorted by rank) — to detect breakdown-only changes.
fn current_ranks(c: &Connection) -> AppResult<HashMap<String, Vec<(i64, i64)>>> {
    let mut stmt = c.prepare("SELECT slug, rank, qty FROM inventory_ranks ORDER BY slug, rank")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    let mut m: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    for r in rows {
        let (slug, rank, qty) = r?;
        m.entry(slug).or_default().push((rank, qty));
    }
    Ok(m)
}

fn catalog_label(c: &Connection, slug: &str) -> AppResult<(String, String)> {
    let row = c
        .query_row(
            "SELECT display_name, part_type FROM catalog_items WHERE slug = ?1",
            params![slug],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .unwrap_or_else(|_| (slug.to_string(), String::new()));
    Ok(row)
}

/// Transactionally merge user-confirmed scan rows into inventory (§5):
/// the scan is authoritative — set `qty = scan_qty`, flip `source` to 'de_scan',
/// record `last_scan_qty`. `scan_qty == 0` deletes the row (a confirmed removal).
/// `first_added_at` is preserved on update. Returns the number of rows touched.
pub fn merge_from_scan(db: &Db, rows: &[ScanApply]) -> AppResult<usize> {
    db.with_mut(|conn| {
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        let mut n = 0usize;
        for row in rows {
            // The rank breakdown is rebuilt from scratch for this slug each time.
            tx.execute(
                "DELETE FROM inventory_ranks WHERE slug = ?1",
                params![row.slug],
            )?;
            if row.scan_qty <= 0 {
                tx.execute(
                    "DELETE FROM inventory_items WHERE slug = ?1",
                    params![row.slug],
                )?;
            } else {
                tx.execute(
                    "INSERT INTO inventory_items
                        (slug, qty, first_added_at, last_modified_at, source, last_scan_qty)
                     VALUES (?1, ?2, ?3, ?3, 'de_scan', ?2)
                     ON CONFLICT(slug) DO UPDATE SET
                        qty = ?2,
                        last_modified_at = ?3,
                        source = 'de_scan',
                        last_scan_qty = ?2",
                    params![row.slug, row.scan_qty, now],
                )?;
                for rq in &row.ranks {
                    if rq.qty > 0 {
                        tx.execute(
                            "INSERT INTO inventory_ranks (slug, rank, qty) VALUES (?1, ?2, ?3)
                             ON CONFLICT(slug, rank) DO UPDATE SET qty = excluded.qty",
                            params![row.slug, rq.rank, rq.qty],
                        )?;
                    }
                }
            }
            n += 1;
        }
        tx.commit()?;
        Ok(n)
    })
}
