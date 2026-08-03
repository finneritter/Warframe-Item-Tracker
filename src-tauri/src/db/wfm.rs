use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::types::{ListingRow, WfmAccount};
use chrono::Utc;
use rusqlite::params;

/// The single account row (id = 1). `has_session` is filled by the caller from
/// the keychain — the JWT never lives in SQLite.
pub fn get_account(db: &Db) -> AppResult<WfmAccount> {
    db.with(|c| {
        let row = c
            .query_row(
                "SELECT username, slug, status FROM wfm_account WHERE id = 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, Option<String>>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .ok();
        Ok(match row {
            Some((username, slug, status)) => WfmAccount {
                connected: username.is_some(),
                username,
                slug,
                status,
                has_session: false,
                session_expires_at: None,
                session_expired: false,
            },
            None => WfmAccount {
                username: None,
                slug: None,
                status: None,
                connected: false,
                has_session: false,
                session_expires_at: None,
                session_expired: false,
            },
        })
    })
}

/// Store the resolved account. `username` is the in-game name for display;
/// `slug` is the warframe.market profile slug every API call is addressed by —
/// they are NOT interchangeable (see `domain::wfm_slug`), so both are required.
pub fn set_account(db: &Db, username: &str, slug: &str, status: Option<&str>) -> AppResult<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO wfm_account (id, username, slug, status) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                username = excluded.username,
                slug = excluded.slug,
                status = COALESCE(excluded.status, wfm_account.status)",
            params![username, slug, status],
        )?;
        Ok(())
    })
}

/// Persist the account's market presence (mirrors `wfm_set_status` after the API call).
pub fn set_status(db: &Db, status: &str) -> AppResult<()> {
    db.with(|c| {
        c.execute(
            "UPDATE wfm_account SET status = ?1 WHERE id = 1",
            params![status],
        )?;
        Ok(())
    })
}

pub fn clear_account(db: &Db) -> AppResult<()> {
    db.with(|c| {
        c.execute("DELETE FROM market_listings", [])?;
        c.execute("DELETE FROM wfm_account WHERE id = 1", [])?;
        Ok(())
    })
}

/// A row to write into the listings mirror.
#[derive(Debug, Clone)]
pub struct ListingMirror {
    pub order_id: String,
    pub slug: String,
    pub order_type: String,
    pub your_price: Option<i64>,
    pub qty: i64,
    pub visible: bool,
}

/// Replace the listings mirror wholesale (it reflects warframe.market's truth).
///
/// `fetched` is how many orders the API actually returned, which is what tells
/// "you have no orders" (fetched 0 — a genuine empty, wipe away) apart from "we
/// resolved none of them" (fetched N, mapped 0 — the catalog hasn't synced, or
/// every order is an item WFIT doesn't track). The second case used to DELETE the
/// whole mirror and report success. `sync_listings_impl` already short-circuits
/// it; this is the backstop so a future caller can't reintroduce it.
pub fn replace_listings(db: &Db, listings: &[ListingMirror], fetched: usize) -> AppResult<usize> {
    if listings.is_empty() && fetched > 0 {
        return Err(AppError::Invalid(format!(
            "refusing to clear the listings mirror: {fetched} orders fetched, 0 matched the catalog"
        )));
    }
    db.with_mut(|conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM market_listings", [])?;
        let now = Utc::now().to_rfc3339();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO market_listings
                    (order_id, slug, order_type, your_price, qty, visible, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for l in listings {
                stmt.execute(params![
                    l.order_id,
                    l.slug,
                    l.order_type,
                    l.your_price,
                    l.qty,
                    l.visible as i64,
                    now,
                ])?;
            }
        }
        tx.commit()?;
        Ok(listings.len())
    })
}

pub fn list_listings(db: &Db) -> AppResult<Vec<ListingRow>> {
    db.with(|c| {
        let mut stmt = c.prepare(
            "SELECT ml.order_id, ml.slug, ci.display_name, ci.part_type, ml.order_type,
                    ml.your_price, ml.qty, ml.visible, pc.median_plat, ml.updated_at,
                    ci.is_vaulted, pc.trend, ci.thumbnail_url
             FROM market_listings ml
             JOIN catalog_items ci ON ci.slug = ml.slug
             LEFT JOIN price_cache pc ON pc.slug = ml.slug
             WHERE ml.order_type = 'sell'
             ORDER BY ci.display_name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ListingRow {
                order_id: r.get(0)?,
                slug: r.get(1)?,
                display_name: r.get(2)?,
                part_type: r.get(3)?,
                order_type: r.get(4)?,
                your_price: r.get(5)?,
                qty: r.get(6)?,
                visible: r.get::<_, i64>(7)? != 0,
                market_low: r.get(8)?,
                updated_at: r.get(9)?,
                is_vaulted: r.get::<_, i64>(10)? != 0,
                trend: r.get(11)?,
                thumbnail_url: r.get(12)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

/// How many rows the mirror currently holds — the honest number to report when a
/// sync deliberately leaves the existing mirror in place.
pub fn count_listings(db: &Db) -> AppResult<usize> {
    db.read(|c| {
        let n: i64 = c.query_row("SELECT COUNT(*) FROM market_listings", [], |r| r.get(0))?;
        Ok(n as usize)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::{seed_item, test_db};

    fn seed(db: &Db) {
        seed_item(db, "a", "set", None);
        let rows = [ListingMirror {
            order_id: "o1".into(),
            slug: "a".into(),
            order_type: "sell".into(),
            your_price: Some(10),
            qty: 1,
            visible: true,
        }];
        replace_listings(db, &rows, 1).unwrap();
    }

    /// The regression guard: fetching orders but matching none of them must not be
    /// mistaken for "you have no orders".
    #[test]
    fn replace_listings_refuses_to_wipe_when_nothing_mapped() {
        let db = test_db("wfm");
        seed(&db);
        assert!(replace_listings(&db, &[], 5).is_err());
        assert_eq!(count_listings(&db).unwrap(), 1);
    }

    /// `fetched == 0` wipes — but only the CALLER can tell a genuine zero from an
    /// unauthenticated read that simply couldn't see invisible orders. That check
    /// lives in `commands::sync_listings_impl` (the `had_session` guard); this layer
    /// trusts the count it is handed, so don't reintroduce a bare `replace_listings`
    /// call on a public fetch.
    #[test]
    fn replace_listings_accepts_a_genuine_empty() {
        let db = test_db("wfm");
        seed(&db);
        assert_eq!(replace_listings(&db, &[], 0).unwrap(), 0);
        assert_eq!(count_listings(&db).unwrap(), 0);
    }

    #[test]
    fn account_roundtrip_keeps_name_and_slug_apart() {
        let db = test_db("wfm");
        set_account(&db, "Nadarejin", "nadarejin", Some("online")).unwrap();
        let acct = get_account(&db).unwrap();
        assert_eq!(acct.username.as_deref(), Some("Nadarejin"));
        assert_eq!(acct.slug.as_deref(), Some("nadarejin"));
        assert!(acct.connected);
    }
}
