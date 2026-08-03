# WFIT — warframe.market Account Sign-In (Listings Import)

**Status:** Proposal (2026-05-30) — **shipped, and since diverged. Read §0 first;** the rest of this
document is the original design record and is stale in the places §0 names.

---

## 0. Current state (2026-08-03) — what actually shipped

Three things below are no longer true of the code:

- **The API surface is v2, not v1.** Orders are `GET /v2/orders/user/<slug>`, the session check is
  `GET /v2/me`, the profile is `GET /v2/user/<slug>`. Every `/v1/profile/…` path in this document is
  dead. (Statistics remain v1 — see `DATA_SOURCING_MASTER_PLAN.md`.)
- **A user is addressed by their profile SLUG, not their in-game name.** The endpoints are
  case-sensitive and 404 on the name (`Nadarejin` → 404; `nadarejin` → 200). The slug is *not*
  derivable: colliding names get an unguessable numeric suffix (`Deepsea_` → `deepsea-0265`) while the
  obvious guess is a **different real account** (`deepsea` is `-DeepSea-`). So Tier 1 is no longer
  "type your username" — `commands.rs::resolve_wfm_identity` resolves a candidate slug
  (`domain::wfm_slug`) and accepts it **only** when the profile's own `ingameName` matches what the
  user typed; a pasted profile URL is the authoritative escape hatch, and `/v2/me` short-circuits the
  whole thing when a session exists. The resolved slug is persisted (`wfm_account.slug`, migration
  0022) and every order call goes through it. Never re-derive it from the name.
- **The listings → inventory import is gone.** `wfm_fetch_listings` / `wfm_apply_import` / `ImportRow`
  / `ImportApply` / `mark_imported` and the review sheet in §4 and §5 no longer exist; `last_import_at`
  is a vestigial column. Real inventory comes from the game memory scan (`GAME_INVENTORY_IMPORT.md`).
  What remains is a **read-only mirror**: `wfm_sync_listings` → `market_listings`, returning a
  `SyncResult { fetched, mirrored, untracked }` so "you have no open orders" is distinguishable from
  "we matched none of them" — a sync that maps zero of N fetched orders refuses to clear the mirror.

Still accurate: the tier model itself (Tier 1 public / Tier 2 pasted JWT / Tier 3 never), the JWT
living in the OS keychain, and the principle that listings ≠ inventory.

---

**Original proposal follows.** · **Date:** 2026-05-30 · **Verified against the warframe.market v1 API surface 2026-05-30.**

> **What this is:** an *optional* convenience feature that lets a user connect their warframe.market
> account to import their **active orders (listings)** into WFIT. It is **not** in-game inventory sync.
>
> **The distinction matters and must stay visible in the UI:** warframe.market knows what you've
> **posted for sale/buy**, not what you **own**. A part you have ten of but haven't listed is invisible
> to warframe.market. So this feature seeds/suggests inventory from *listings*, clearly labeled as such —
> it never claims to be your real inventory.
>
> **Why not real inventory sync (here):** there is no official Digital Extremes inventory API. The only routes
> are unofficial (the private mobile/Companion endpoint, with session tokens scraped from game memory) and
> carry a documented ban risk. **Superseded for users who opt in** — that AlecaFrame-parity path is now
> specced in `GAME_INVENTORY_IMPORT.md` (opt-in, consent-gated, Linux-only, off by default). This
> *listings* feature stays entirely within warframe.market's public + authenticated API and remains the
> zero-risk default recommendation; the game scan is the power-user lane.

This refines `archive/DESKTOP_REWRITE_PRD.md` (commands + schema) and reuses the rate limiter / client from
`DATA_SOURCING_MASTER_PLAN.md`. It adds **one new external capability** (authenticated WFM calls) and
treats it as strictly optional — the app is fully functional without ever signing in.

---

## 1. Three connection tiers (pick the lowest that does the job)

| Tier | Mechanism | Credentials handled | Gets you | Recommendation |
|---|---|---|---|---|
| **1 — Username only** | `GET /v1/profile/{username}/orders` (public, no auth) | none — just a username | Your **visible** sell/buy orders | **Ship this first.** Zero credential risk, ~90% of the value. |
| **2 — JWT paste** | User copies their `JWT` cookie from the browser; app stores it | a session token (no password) | Visible **+ invisible** orders, own profile | Good v1.1. No password ever touches the app. |
| **3 — Email/password login** | App POSTs `/v1/auth/signin` itself (CSRF + Cloudflare) | full password, briefly | Same as Tier 2, smoother UX | Defer. Most friction + most risk for least marginal gain. |

The cheapest tier that satisfies the user wins. Tier 1 needs **no auth at all** — a user's orders are a
public endpoint — so the MVP is genuinely just "type your WFM username, review what we found, import."

---

## 2. Auth mechanics (Tiers 2–3)

warframe.market auth is **v1** (the catalog is v2; auth + orders live on v1 — consistent with the app's
existing mixed-version usage).

- **Token shape:** a **JWT**. Accepted two ways:
  - Cookie: `Cookie: JWT=<token>`
  - Header: `Authorization: JWT <token>`  ← prefer this for a desktop client.
- **Tier 2 (paste):** the user grabs the `JWT` cookie value from `warframe.market` in their browser
  (DevTools → Application/Storage → Cookies). The app validates it with one authenticated call and stores it.
- **Tier 3 (full login):** `POST /v1/auth/signin` with a JSON body (`email`, `password`,
  `auth_type: "header"`). **Caveats:** the endpoint sits behind **Cloudflare** and expects a CSRF token
  obtained from a prior page load, so a naive POST can get challenged. This is the fragile path — only
  build it if Tier 1/2 prove insufficient, and expect to maintain it.
- **Validation:** before trusting a stored token, make one cheap authenticated request; if it 401s, mark
  the session expired and fall back to the signed-out state. Never block app launch on WFM auth.

**Token storage:** OS keychain via Tauri (`keyring` crate / secure store) — **never** in `wfit.db`,
never in plaintext, never logged. The DB stays a pure cache of public game data; the session is a secret.

---

## 3. Endpoints used

| Endpoint | Auth | Purpose |
|---|---|---|
| `GET /v1/profile/{username}/orders` | none (more complete with auth) | The user's sell/buy orders → the import source |
| `POST /v1/auth/signin` | — | Tier 3 only: exchange email/password for a JWT |
| `GET /v1/profile/{username}` (or `/settings/accounts`) | JWT | Confirm who's signed in; show linked-platform info (informational) |

All requests keep the existing headers (`User-Agent: wfit-desktop/0.1`, `Language: en`,
`Platform: pc`, `Accept: application/json`) and the **same global 400 ms throttle** — no separate
rate-limit pool.

---

## 4. What the import actually does

1. Fetch the user's **sell** orders (buy orders are optional — usually not relevant to an inventory tracker).
2. For each order, map `order.item.slug` → an existing `catalog_items.slug`. Drop anything that isn't a
   tracked prime part (the user may list mods, rivens, etc. — out of scope).
3. Present a **reviewable import sheet**, not a silent write: each matched part shows its listed quantity
   and asking price, with a checkbox and an editable qty. The user confirms before anything touches inventory.
4. On confirm, upsert into `inventory_items` — **merge, don't clobber**. Surface conflicts ("you have 3
   manually, your listing says 5") and let the user choose; never overwrite manual counts blindly.

**Honest-gap reminders to bake into the UI copy:**
- Listings ≠ inventory. Unlisted parts won't appear.
- Invisible orders require Tier 2/3 (auth); Tier 1 sees visible orders only.
- The listed price is *your* ask, not the market median — keep it separate from the §3 statistics price.

---

## 5. New schema (small)

```sql
-- Optional: remember the connected account + last import (token is NOT here — keychain only)
CREATE TABLE wfm_account (
  id            INTEGER PRIMARY KEY CHECK (id = 1),   -- single row, single user
  username      TEXT,
  last_import_at TEXT
);
```

No new columns on `inventory_items`. If you want to mark provenance, an optional
`inventory_items.source TEXT` (`'manual'` | `'wfm_import'`) makes conflict UX and undo easier — but it's
not required for v1.

---

## 6. New commands (`commands.rs`)

| Command | Behavior |
|---|---|
| `wfm_connect(username)` | Tier 1: store username, no auth. Returns success + whether orders are visible. |
| `wfm_set_session(jwt)` | Tier 2: validate the pasted JWT, store in keychain on success, persist username. |
| `wfm_signin(email, password)` | Tier 3 (deferred): full login → JWT → keychain. |
| `wfm_signout()` | Clear keychain token + `wfm_account` row. |
| `wfm_fetch_listings()` | Fetch `/profile/{username}/orders` (auth header if present), map slugs → catalog, return a **preview** `Vec<ImportRow>` — does **not** write. |
| `wfm_apply_import(rows)` | Transactional upsert of the user-confirmed rows into `inventory_items` (merge semantics). |

`wfm_fetch_listings` is read-only and previews; `wfm_apply_import` is the only writer, kept transactional
like `add_to_inventory` / `record_sale`.

---

## 7. Safety, scope, and ToS posture

- **Read-only.** WFIT never *creates, edits, or deletes* warframe.market orders. It only reads the
  user's own orders. (Order management is explicitly out of scope.)
- **Stays on the public/authenticated WFM API** — the sanctioned surface. No game-memory scanning, no
  private DE mobile endpoint, no scraping. This is the safe lane.
- **Respect the throttle** (400 ms global min-gap, ~2.5 req/s) — Cloudflare will block bursts.
- **Token hygiene:** keychain only; validate-then-use; expire gracefully; offer a clear "Disconnect."
- **Fully optional & removable:** signed-out is the default and a first-class state. The feature can be
  cut entirely without touching the core data layer.

---

## 8. Suggested build order

1. **Tier 1 end-to-end:** `wfm_connect` → `wfm_fetch_listings` (public) → import-preview UI →
   `wfm_apply_import`. This alone delivers the feature.
2. **Tier 2:** add `wfm_set_session` + keychain storage + auth header on the orders call (picks up
   invisible orders).
3. **Polish:** provenance tag, conflict resolution UX, "last imported" timestamp in Settings.
4. **Tier 3 (only if wanted):** in-app email/password login. Budget time for the CSRF/Cloudflare dance.

**Definition of done (v1):** Settings → "Connect warframe.market" → enter username → review found
listings → import selected → inventory updates, with manual counts preserved and everything labeled as
*listings, not inventory*.

---

## 9. One-line summary for future sessions

> Optional WFM account connect that imports **listings (orders), not inventory** — there's no DE
> inventory API. Tier 1 = public `GET /v1/profile/{username}/orders` (no auth, ship first); Tier 2 = pasted
> **JWT** (`Authorization: JWT <token>`, keychain-stored) for invisible orders; Tier 3 = full
> `/v1/auth/signin` (CSRF + Cloudflare, deferred). Read-only, reuses the 400 ms throttle, never clobbers
> manual inventory, token lives in the OS keychain — never in the DB.
