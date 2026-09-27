-- Negative cache for statistics fetches that returned no usable data (a brand-new
-- item with no closed trades yet — empty 90-day series — or a non-2xx). Without
-- it such a slug never gets a price_cache row, so every staleness query picked it
-- again forever: the launch drain spun endlessly on the last few items (holding
-- the "syncing…" flag, which also starved the heartbeat) and the catalog tail
-- sorted them first on every tick. Rows expire, so the item is retried later.
CREATE TABLE IF NOT EXISTS price_miss (
  slug       TEXT PRIMARY KEY,
  checked_at TEXT NOT NULL,
  expires_at TEXT NOT NULL
);
