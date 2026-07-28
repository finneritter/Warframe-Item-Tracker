use crate::db::Db;
use crate::domain::mod_rarity;
use crate::error::AppResult;
use crate::types::CatalogRow;
use chrono::Utc;
use rusqlite::params;
use std::collections::HashMap;

/// A catalog row ready to upsert (Pass A: skeleton + ducats from /v2/items).
#[derive(Debug, Clone)]
pub struct CatalogUpsert {
    pub slug: String,
    pub wfm_id: Option<String>,
    pub display_name: String,
    pub part_type: String,
    pub category: String, // 'warframe'|'weapon'|'set'|'mod'|'arcane'
    pub set_slug: Option<String>,
    pub ducats: Option<i64>,
    pub game_ref: Option<String>, // DE internal `uniqueName` path (joins to game inventory)
    pub max_rank: Option<i64>,    // rank ceiling (mods/arcanes); null for prime parts
    pub is_vaulted: bool,
    pub is_tradeable: bool,
    pub thumbnail_url: Option<String>,
}

pub fn count(db: &Db) -> AppResult<i64> {
    db.read(|c| {
        let n: i64 = c.query_row("SELECT COUNT(*) FROM catalog_items", [], |r| r.get(0))?;
        Ok(n)
    })
}

/// Count catalog rows still missing the `game_ref` join key. Non-zero after the
/// 0003 migration until a catalog refetch backfills them (the API supplies it).
pub fn missing_game_ref_count(db: &Db) -> AppResult<i64> {
    db.read(|c| {
        let n: i64 = c.query_row(
            "SELECT COUNT(*) FROM catalog_items WHERE game_ref IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    })
}

/// True once any catalog row carries `max_rank` — i.e. a post-0004 refresh has run.
/// Prime parts legitimately have null max_rank, so we can't check "all"; "any"
/// non-null means the backfill happened. Used to trigger that one-time refetch.
pub fn has_any_max_rank(db: &Db) -> AppResult<bool> {
    db.read(|c| {
        let n: i64 = c.query_row(
            "SELECT COUNT(*) FROM catalog_items WHERE max_rank IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    })
}

/// Upsert the catalog in one transaction. Preserves existing ducats / thumbnails
/// when a refresh somehow omits them (COALESCE), but always refreshes the rest.
pub fn upsert_many(db: &Db, items: &[CatalogUpsert]) -> AppResult<usize> {
    db.with_mut(|conn| {
        let tx = conn.transaction()?;
        let now = Utc::now().to_rfc3339();
        {
            let mut stmt = tx.prepare(
                "INSERT INTO catalog_items
                    (slug, wfm_id, display_name, part_type, category, set_slug,
                     ducats, game_ref, max_rank, is_vaulted, is_tradeable, thumbnail_url,
                     mod_rarity, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(slug) DO UPDATE SET
                    wfm_id        = COALESCE(excluded.wfm_id, catalog_items.wfm_id),
                    display_name  = excluded.display_name,
                    part_type     = excluded.part_type,
                    category      = excluded.category,
                    set_slug      = excluded.set_slug,
                    ducats        = COALESCE(excluded.ducats, catalog_items.ducats),
                    game_ref      = COALESCE(excluded.game_ref, catalog_items.game_ref),
                    max_rank      = COALESCE(excluded.max_rank, catalog_items.max_rank),
                    -- is_vaulted is owned by db::vault::apply (warframe-items), not the
                    -- catalog API (which has no vault data) — don't clobber it on refresh.
                    is_tradeable  = excluded.is_tradeable,
                    thumbnail_url = COALESCE(excluded.thumbnail_url, catalog_items.thumbnail_url),
                    mod_rarity    = COALESCE(excluded.mod_rarity, catalog_items.mod_rarity),
                    updated_at    = excluded.updated_at",
            )?;
            for it in items {
                // Mods only: bundled rarity keyed on game_ref (uniqueName).
                let rarity = if it.category == "mod" {
                    it.game_ref.as_deref().and_then(mod_rarity::rarity_for)
                } else {
                    None
                };
                stmt.execute(params![
                    it.slug,
                    it.wfm_id,
                    it.display_name,
                    it.part_type,
                    it.category,
                    it.set_slug,
                    it.ducats,
                    it.game_ref,
                    it.max_rank,
                    it.is_vaulted as i64,
                    it.is_tradeable as i64,
                    it.thumbnail_url,
                    rarity,
                    now,
                ])?;
            }
        }
        // The upsert rewrites set_slug from the slug heuristic every time, so the
        // repair has to ride along with it rather than run once.
        repair_set_slugs_tx(&tx)?;
        tx.commit()?;
        Ok(items.len())
    })
}

/// Repair `set_slug`s the slug heuristic gets wrong. `derive_set_slug` assumes a
/// set is always named `<stem>_prime_set`, which breaks two ways:
///   * Kavasa Prime's parts belong to `kavasa_prime_kubrow_collar_set`, so band /
///     buckle / collar blueprint pointed at a `kavasa_prime_set` that doesn't exist.
///   * `gotva_prime` is a whole weapon, not a part — it has no set at all.
///
/// Both produced phantom rows on the Sets screen. `set_membership` (filled by the
/// /set pass) is authoritative, so prefer it; a part pointing at a non-existent set
/// with no membership row loses its set_slug. Idempotent.
pub fn repair_set_slugs(db: &Db) -> AppResult<usize> {
    db.with_mut(|conn| {
        let tx = conn.transaction()?;
        let n = repair_set_slugs_tx(&tx)?;
        tx.commit()?;
        Ok(n)
    })
}

fn repair_set_slugs_tx(tx: &rusqlite::Transaction<'_>) -> rusqlite::Result<usize> {
    // Only rows whose set_slug names no real set item are touched — a correct
    // derivation is left alone, and mod sets (Vigilante &c., never derived) stay
    // out of the Sets screen.
    const ORPHANED: &str = "set_slug IS NOT NULL
         AND NOT EXISTS (SELECT 1 FROM catalog_items s
                          WHERE s.slug = catalog_items.set_slug AND s.category = 'set')";
    let repointed = tx.execute(
        &format!(
            "UPDATE catalog_items
                SET set_slug = (SELECT sm.set_slug FROM set_membership sm
                                 WHERE sm.part_slug = catalog_items.slug)
              WHERE {ORPHANED}
                AND EXISTS (SELECT 1 FROM set_membership sm
                             WHERE sm.part_slug = catalog_items.slug)"
        ),
        [],
    )?;
    let cleared = tx.execute(
        &format!("UPDATE catalog_items SET set_slug = NULL WHERE {ORPHANED}"),
        [],
    )?;
    Ok(repointed + cleared)
}

/// Current bundled mod-rarity dataset version. Bump alongside
/// `domain/data/mod_rarity.tsv` to force a one-time re-backfill on next launch.
const MOD_RARITY_VER: &str = "1";

/// Populate `catalog_items.mod_rarity` for existing mods from the bundled map
/// (keyed on game_ref). Runs once per dataset version — the ongoing upsert keeps
/// new mods current, this just fills rows that predate the column. Idempotent.
pub fn backfill_mod_rarity(db: &Db) -> AppResult<usize> {
    use crate::db::settings;
    if settings::get(db, settings::KEY_MOD_RARITY_VER)?.as_deref() == Some(MOD_RARITY_VER) {
        return Ok(0);
    }
    let n = db.with_mut(|conn| {
        let tx = conn.transaction()?;
        let pairs: Vec<(String, String)> = {
            let mut stmt = tx.prepare(
                "SELECT slug, game_ref FROM catalog_items
                 WHERE category = 'mod' AND game_ref IS NOT NULL",
            )?;
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut updated = 0usize;
        for (slug, game_ref) in &pairs {
            if let Some(rarity) = mod_rarity::rarity_for(game_ref) {
                tx.execute(
                    "UPDATE catalog_items SET mod_rarity = ?1 WHERE slug = ?2",
                    params![rarity, slug],
                )?;
                updated += 1;
            }
        }
        tx.commit()?;
        Ok(updated)
    })?;
    settings::set(db, settings::KEY_MOD_RARITY_VER, MOD_RARITY_VER)?;
    Ok(n)
}

/// The warframe.market item id for a slug (for posting orders, which key by id).
pub fn wfm_id_for(db: &Db, slug: &str) -> AppResult<Option<String>> {
    db.read(|c| {
        let id = c
            .query_row(
                "SELECT wfm_id FROM catalog_items WHERE slug = ?1",
                params![slug],
                |r| r.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        Ok(id)
    })
}

/// Build the warframe.market id -> slug map (for resolving setParts ids in Pass B).
pub fn id_slug_map(db: &Db) -> AppResult<HashMap<String, String>> {
    db.read(|c| {
        let mut stmt =
            c.prepare("SELECT wfm_id, slug FROM catalog_items WHERE wfm_id IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = HashMap::new();
        for r in rows {
            let (id, slug) = r?;
            map.insert(id, slug);
        }
        Ok(map)
    })
}

/// Canonical key for matching a free-text item name (vendor stock, worldstate
/// reward strings) against catalog `display_name`s: lowercase, drop everything but
/// alphanumerics and single spaces. "Ash Prime Blade" / "ash  prime  blade!" → "ash prime blade".
/// Shared by vendor intel (F2), wanted-now reward matching (F3), and relics (F1).
pub fn normalize_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_space = false;
        } else if !prev_space && !out.is_empty() {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim_end().to_string()
}

/// normalized display_name → slug, for resolving free-text names to catalog items.
/// On a name collision the first slug seen wins (rare; e.g. a part and set sharing a
/// name). Built once per caller.
pub fn name_slug_map(db: &Db) -> AppResult<HashMap<String, String>> {
    db.read(|c| {
        let mut stmt = c.prepare("SELECT display_name, slug FROM catalog_items")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut map = HashMap::new();
        for r in rows {
            let (name, slug) = r?;
            map.entry(normalize_name(&name)).or_insert(slug);
        }
        Ok(map)
    })
}

const CATALOG_SELECT: &str = "SELECT
        ci.slug, ci.display_name, ci.part_type, ci.category, ci.set_slug,
        ci.ducats, ci.is_vaulted, pc.median_plat, pc.trend, pc.delta_7d,
        pc.volume_7d, ci.thumbnail_url,
        CASE WHEN ci.category = 'set' THEN (
            SELECT COALESCE(MIN(COALESCE(mi.qty, 0)), 0)
            FROM catalog_items m
            LEFT JOIN inventory_items mi ON mi.slug = m.slug
            WHERE m.set_slug = ci.slug
        ) ELSE COALESCE(ii.qty, 0) END AS owned_qty,
        CASE WHEN w.slug IS NOT NULL THEN 1 ELSE 0 END AS on_watchlist,
        COALESCE(bl.buy_qty, 0) AS buy_qty
     FROM catalog_items ci
     LEFT JOIN price_cache pc ON pc.slug = ci.slug
     LEFT JOIN inventory_items ii ON ii.slug = ci.slug
     LEFT JOIN watchlist w ON w.slug = ci.slug
     LEFT JOIN buy_list bl ON bl.slug = ci.slug";

fn map_catalog_row(r: &rusqlite::Row) -> rusqlite::Result<CatalogRow> {
    Ok(CatalogRow {
        slug: r.get(0)?,
        display_name: r.get(1)?,
        part_type: r.get(2)?,
        category: r.get(3)?,
        set_slug: r.get(4)?,
        ducats: r.get(5)?,
        is_vaulted: r.get::<_, i64>(6)? != 0,
        median_plat: r.get(7)?,
        trend: r.get(8)?,
        delta_7d: r.get(9)?,
        volume_7d: r.get(10)?,
        thumbnail_url: r.get(11)?,
        owned_qty: r.get(12)?,
        on_watchlist: r.get::<_, i64>(13)? != 0,
        buy_qty: r.get(14)?,
    })
}

/// List the catalog, optionally filtered to one category. Used by the Add Items modal.
pub fn list(db: &Db, category: Option<&str>) -> AppResult<Vec<CatalogRow>> {
    db.read(|c| {
        let mut out = Vec::new();
        match category {
            Some(cat) => {
                let sql =
                    format!("{CATALOG_SELECT} WHERE ci.category = ?1 ORDER BY ci.display_name ASC");
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt.query_map(params![cat], map_catalog_row)?;
                for r in rows {
                    out.push(r?);
                }
            }
            None => {
                let sql = format!("{CATALOG_SELECT} ORDER BY ci.display_name ASC");
                let mut stmt = c.prepare(&sql)?;
                let rows = stmt.query_map([], map_catalog_row)?;
                for r in rows {
                    out.push(r?);
                }
            }
        }
        Ok(out)
    })
}

/// Search the catalog by display name (case-insensitive substring).
pub fn search(db: &Db, q: &str, limit: i64) -> AppResult<Vec<CatalogRow>> {
    db.read(|c| {
        let like = format!("%{}%", q.replace('%', "\\%").replace('_', "\\_"));
        let sql = format!(
            "{CATALOG_SELECT} WHERE ci.display_name LIKE ?1 ESCAPE '\\'
             ORDER BY ci.display_name ASC LIMIT ?2"
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(params![like, limit], map_catalog_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

/// One catalog row by slug (for the Drawer when an item isn't owned).
pub fn get(db: &Db, slug: &str) -> AppResult<Option<CatalogRow>> {
    db.read(|c| {
        let sql = format!("{CATALOG_SELECT} WHERE ci.slug = ?1");
        let row = c.query_row(&sql, params![slug], map_catalog_row).ok();
        Ok(row)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::testutil::test_db;

    fn up(slug: &str, name: &str, category: &str) -> CatalogUpsert {
        CatalogUpsert {
            slug: slug.into(),
            display_name: name.into(),
            part_type: "Part".into(),
            category: category.into(),
            set_slug: None,
            ducats: Some(45),
            game_ref: None,
            max_rank: None,
            is_vaulted: false,
            is_tradeable: true,
            thumbnail_url: None,
            wfm_id: None,
        }
    }

    fn part_of(slug: &str, set_slug: &str) -> CatalogUpsert {
        CatalogUpsert {
            set_slug: Some(set_slug.into()),
            ..up(slug, slug, "weapon")
        }
    }

    fn set_slug_of(db: &crate::db::Db, slug: &str) -> Option<String> {
        db.read(|c| {
            Ok(c.query_row(
                "SELECT set_slug FROM catalog_items WHERE slug = ?1",
                params![slug],
                |r| r.get::<_, Option<String>>(0),
            )?)
        })
        .unwrap()
    }

    /// Kavasa Prime's parts derive `kavasa_prime_set`, but the real set item is
    /// `kavasa_prime_kubrow_collar_set`; `gotva_prime` is a whole weapon with no
    /// set. Both must stop pointing at a set that doesn't exist.
    #[test]
    fn upsert_repairs_phantom_set_slugs() {
        let db = test_db("catalog-setslug-repair");
        upsert_many(
            &db,
            &[
                up("kavasa_prime_kubrow_collar_set", "Kavasa Prime Set", "set"),
                up("saryn_prime_set", "Saryn Prime Set", "set"),
            ],
        )
        .unwrap();
        db.with(|c| {
            c.execute(
                "INSERT INTO set_membership (set_slug, part_slug, quantity_in_set)
                 VALUES ('kavasa_prime_kubrow_collar_set', 'kavasa_prime_band', 1)",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        upsert_many(
            &db,
            &[
                part_of("kavasa_prime_band", "kavasa_prime_set"),
                part_of("gotva_prime", "gotva_prime_set"),
                part_of("saryn_prime_chassis", "saryn_prime_set"),
            ],
        )
        .unwrap();

        assert_eq!(
            set_slug_of(&db, "kavasa_prime_band").as_deref(),
            Some("kavasa_prime_kubrow_collar_set"),
            "membership is authoritative when the derived set is missing"
        );
        assert_eq!(
            set_slug_of(&db, "gotva_prime"),
            None,
            "no set item and no membership row -> not a part of anything"
        );
        assert_eq!(
            set_slug_of(&db, "saryn_prime_chassis").as_deref(),
            Some("saryn_prime_set"),
            "a correct derivation must be left alone"
        );
    }

    #[test]
    fn upsert_inserts_then_updates_in_place() {
        let db = test_db("catalog-upsert");
        let n = upsert_many(
            &db,
            &[up("saryn_prime_chassis", "Saryn Prime Chassis", "warframe")],
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(count(&db).unwrap(), 1);

        // Re-upserting the same slug must update, not duplicate.
        upsert_many(
            &db,
            &[up(
                "saryn_prime_chassis",
                "Saryn Prime Chassis!",
                "warframe",
            )],
        )
        .unwrap();
        assert_eq!(count(&db).unwrap(), 1);
        let row = get(&db, "saryn_prime_chassis").unwrap().unwrap();
        assert_eq!(row.display_name, "Saryn Prime Chassis!");
    }

    #[test]
    fn search_is_case_insensitive_and_bounded() {
        let db = test_db("catalog-search");
        upsert_many(
            &db,
            &[
                up("saryn_prime_chassis", "Saryn Prime Chassis", "warframe"),
                up("saryn_prime_systems", "Saryn Prime Systems", "warframe"),
                up("mesa_prime_chassis", "Mesa Prime Chassis", "warframe"),
            ],
        )
        .unwrap();
        let hits = search(&db, "saryn", 10).unwrap();
        assert_eq!(hits.len(), 2);
        let hits = search(&db, "SARYN", 1).unwrap();
        assert_eq!(hits.len(), 1, "limit must bound the result");
        assert!(search(&db, "zzz_nothing", 10).unwrap().is_empty());
    }
}
