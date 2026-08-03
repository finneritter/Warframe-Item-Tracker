// TS mirrors of the Rust DTOs in src-tauri/src/types.rs. serde serializes with
// the field names as written (snake_case), so these match 1:1.

export type Category = "warframe" | "weapon" | "set" | "mod" | "arcane";
export type Trend = "up" | "flat" | "down";

export interface CatalogRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  set_slug: string | null;
  ducats: number | null;
  is_vaulted: boolean;
  median_plat: number | null;
  trend: Trend | null;
  delta_7d: number | null;
  volume_7d: number | null; // 7-day trade volume (liquidity sort / thin-market hint)
  thumbnail_url: string | null;
  owned_qty: number;
  on_watchlist: boolean;
  buy_qty: number;
}

export interface InventoryRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  set_slug: string | null;
  qty: number;
  ducats: number | null;
  is_vaulted: boolean;
  median_plat: number | null;
  trend: Trend | null;
  delta_7d: number | null;
  volume_7d: number | null;
  thumbnail_url: string | null;
  last_modified_at: string;
  source: string; // 'manual' | 'wfm_import' | 'de_scan'; "" for collapsed set rows
  first_added_at: string; // RFC3339 when WFIT first recorded it; "" for collapsed sets
  value_plat: number | null; // rank-aware total value (mods/arcanes); else use median×qty
  realizable_plat: number | null; // liquidation-adjusted value (≤ market value)
  daily_volume: number | null;
  liquidity: number | null; // φ 0..1
  days_to_sell: number | null;
  confidence: "high" | "medium" | "low" | null;
  spark: number[]; // recent median series for the List-view sparkline (display-only)
  mod_rarity: string | null; // common|uncommon|rare|legendary (mods only)
  excluded: boolean; // value excluded from portfolio total (rarity on exclusion list)
}

export interface SaleRow {
  id: number;
  slug: string;
  display_name: string;
  category: Category;
  qty: number;
  plat_per_unit: number | null;
  market_median_at_sale_time: number | null;
  sold_at: string;
  notes: string | null;
  thumbnail_url: string | null;
}

export interface Summary {
  total_plat: number;
  realizable_plat: number;
  total_ducats: number;
  part_count: number;
  distinct_count: number;
  full_set_count: number;
  portfolio_7d: number | null;
  hot_count: number;
  sold_7d: number;
  at_target_count: number;
  last_synced: string | null;
}

export interface WatchRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  median_plat: number | null;
  trend: Trend | null;
  delta_7d: number | null;
  target_plat: number | null;
  is_vaulted: boolean;
  thumbnail_url: string | null;
  added_at: string;
}

export interface BuyRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  median_plat: number | null;
  trend: Trend | null;
  is_vaulted: boolean;
  buy_qty: number;
  thumbnail_url: string | null;
  added_at: string;
}

export interface SetPart {
  slug: string;
  part_name: string;
  /** owned_qty >= required — a ×2 part owned once is NOT owned. */
  owned: boolean;
  owned_qty: number;
  /** quantity_in_set — 2 for dual-weapon barrels/receivers. */
  required: number;
  median_plat: number | null;
}

export interface SetRow {
  set_slug: string;
  set_name: string;
  category: Category;
  /** Unit counts (sum of required quantities), not distinct part types. */
  total_parts: number;
  owned_parts: number;
  complete: boolean;
  parts: SetPart[];
  set_value: number | null;
  missing_value: number | null;
}

export interface DucatRow {
  slug: string;
  display_name: string;
  part_type: string;
  qty: number;
  median_plat: number | null;
  ducats: number;
  ducats_per_plat: number | null;
  verdict: "ducat" | "plat";
  is_vaulted: boolean;
  trend: Trend | null;
  thumbnail_url: string | null;
}

// ---- Arcanes / Vosfor ----
export interface ArcaneContribution {
  slug: string;
  display_name: string;
  prob: number;
  plat: number | null;
}
export interface CollectionEv {
  key: string;
  name: string;
  ev_plat_per_pull: number;
  plat_per_vosfor: number;
  legendary_pct: number;
  coverage: number;
  pool_size: number;
  top: ArcaneContribution[];
}
export interface ArcaneBreakdown {
  slug: string;
  display_name: string;
  rarity: string;
  plat: number | null;
  realizable: number;
  vosfor: number;
  prob: number;
  ev_contribution: number;
  thumbnail_url: string | null;
}
export interface OwnedArcane {
  slug: string;
  display_name: string;
  qty: number;
  rank0_copies: number;
  plat: number | null; // rank-0 (unranked) price — the sell reference
  maxed_plat: number | null; // muted info only
  vosfor: number; // per unranked copy
  sell_qty: number;
  sell_plat: number;
  dissolve_qty: number;
  vosfor_total: number; // dissolve_qty × vosfor
  dissolve_plat_equiv: number;
  collection: string | null;
  rarity: string | null;
  verdict: "sell" | "dissolve";
  trend: Trend | null;
  thumbnail_url: string | null;
}
export interface ArcaneSummary {
  total_vosfor: number; // recommended-dissolve Vosfor
  owned_count: number;
  sell_plat: number; // recommended-sell realizable plat
  best_collection: string | null;
  best_plat_per_200: number;
  plat_per_vosfor: number;
}
export interface ArcaneDashboard {
  collections: CollectionEv[];
  owned: OwnedArcane[];
  summary: ArcaneSummary;
}

export interface TrendRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  median_plat: number;
  delta: number; // % move over the selected timeframe
  z: number; // volatility-normalized move (std devs)
  range_pos: number; // 0..1 within lookback low..high
  range_low: number;
  range_high: number;
  volume: number; // avg daily volume
  owned_qty: number;
  on_watchlist: boolean;
  spark: number[];
  thumbnail_url: string | null;
}

export interface HeatRow {
  category: Category;
  avg_delta: number;
  count: number;
}

export interface TrendsData {
  index_change: number;
  advancing: number;
  declining: number;
  flat: number;
  index_spark: number[];
  liquid_count: number;
  total_priced: number;
  holdings_value: number;
  holdings_change: number;
  sell_signal_count: number;
  sell_signals: TrendRow[];
  buy_candidates: TrendRow[];
  unusual: TrendRow[];
  category_heat: HeatRow[];
}

export interface HistoryPoint {
  day: string;
  median: number | null;
  volume: number | null;
  open: number | null;
  high: number | null;
  low: number | null;
  close: number | null;
}

export interface ItemDetail {
  slug: string;
  display_name: string;
  part_type: string;
  category: Category;
  set_slug: string | null;
  ducats: number | null;
  median_plat: number | null;
  trend: Trend | null;
  delta_7d: number | null;
  volume_7d: number | null;
  thumbnail_url: string | null;
  owned_qty: number;
  on_watchlist: boolean;
  listed: boolean;
  realized_plat: number;
  sold_qty: number;
  max_rank: number | null;
  ranks: OwnedRank[];
  value_plat: number | null;
  realizable_plat: number | null;
  daily_volume: number | null;
  liquidity: number | null;
  days_to_sell: number | null;
  confidence: "high" | "medium" | "low" | null;
  history: HistoryPoint[];
}

export interface OwnedRank {
  rank: number;
  qty: number;
  median: number | null;
}

export interface ItemOrders {
  best_buy: number | null;
  best_sell: number | null;
  buyers: number;
  sellers: number;
}

export interface SellerOrder {
  ingame_name: string;
  reputation: number;
  status: "ingame" | "online" | "offline";
  platinum: number;
  quantity: number;
  rank: number | null;
}

/** One price level of the online bid ladder (demand depth), qty summed across
 *  buyers at that price. `rank` mirrors the seller table for mods/arcanes. */
export interface BuyOrder {
  platinum: number;
  quantity: number;
  rank: number | null;
}

export interface ItemSellers {
  display_name: string;
  max_rank: number | null;
  best_buy: number | null;
  buyers: number;
  sellers: number;
  orders: SellerOrder[];
  bids: BuyOrder[];
}

export interface WfmAccount {
  /** In-game name, spelled the way warframe.market spells it. Display only. */
  username: string | null;
  /** The warframe.market profile slug — what /v2/orders/user/<…> and /profile/<…>
   *  take. NOT a lowercase of `username` (collisions get an unguessable numeric
   *  suffix). Null on a pre-0022 install until the next connect/sync resolves it. */
  slug: string | null;
  status: string | null;
  connected: boolean;
  has_session: boolean;
  session_expires_at: string | null;
  session_expired: boolean;
}

// Result of the developer "simulate fake inventory" tool (1:1 with Rust SimSummary).
export interface SimSummary {
  items: number;
  mods: number;
  arcanes: number;
  resources: number;
  total_items: number; // sum of owned qty across all tradeable rows (the "15k" headline)
  platinum: number;
  credits: number;
  backup_path: string;
}

export interface ListingRow {
  order_id: string;
  slug: string;
  display_name: string;
  part_type: string;
  order_type: string;
  your_price: number | null;
  qty: number;
  visible: boolean;
  market_low: number | null;
  updated_at: string | null;
  is_vaulted: boolean;
  trend: Trend | null;
  thumbnail_url: string | null;
}

/** What a listings sync actually did (1:1 with Rust `SyncResult`). Distinguishes
 *  "you have no open orders" (fetched 0) from "we matched none of them" — the two
 *  used to look identical, i.e. like nothing happening at all. */
export interface SyncResult {
  fetched: number;
  mirrored: number;
  untracked: number;
}

export interface RecommendationRow {
  slug: string;
  display_name: string;
  part_type: string;
  category: string;
  thumbnail_url: string | null;
  rank: number | null;
  owned_qty: number;
  avg_daily_volume: number;
  suggested_price: number;
  median_plat: number | null;
  est_value: number;
  ducats_per_plat: number | null;
  trend: Trend | null;
}

export interface RepriceRow {
  order_id: string;
  slug: string;
  display_name: string;
  part_type: string;
  thumbnail_url: string | null;
  qty: number;
  visible: boolean;
  current_price: number | null;
  new_price: number;
}

export interface RepriceApply {
  order_id: string;
  platinum: number;
  quantity: number;
  visible: boolean;
}

// Game inventory import (memory-scan) — opt-in, consent-gated, Linux-only.
export interface GameScanStatus {
  supported: boolean;
  consented: boolean;
  warframe_running: boolean;
  auto_sync: boolean;
  last_scan_at: string | null;
}
export interface RankQty {
  rank: number;
  qty: number;
}
export interface ScanDiffRow {
  slug: string;
  display_name: string;
  part_type: string;
  status: "added" | "changed" | "removed";
  scan_qty: number;
  current_qty: number;
  source: string;
  ranks: RankQty[];
}
export interface ScanApply {
  slug: string;
  scan_qty: number;
  ranks: RankQty[];
}

// Account section (Profile · Codex · Resources · Arsenal) — mirrors src-tauri/src/types.rs
export interface IntrinsicRow {
  skill_key: string;
  label: string;
  rank: number;
}
export interface SyndicateRow {
  tag: string;
  label: string;
  standing: number;
  title: string | null;
}
export interface AccountProfile {
  has_data: boolean;
  scanned_at: string | null;
  mastery_rank: number;
  mr_into_next: number;
  mr_needed: number;
  equipped_glyph: string | null;
  equipped_glyph_name: string | null;
  created: string | null;
  credits: number;
  platinum: number;
  regal_aya: number;
  endo: number;
  trades_remaining: number;
  gifts_remaining: number;
  nodes_completed: number;
  nodes_total: number;
  total_missions: number;
  daily_focus: number;
  focus_xp: number;
  login_streak: number;
  guild_id: string | null;
  alignment: string | null;
  training_date: string | null;
  total_mastery_points: number;
  intrinsics: IntrinsicRow[];
  syndicates: SyndicateRow[];
}
export interface GearRow {
  unique_name: string;
  display_name: string;
  category: string;
  icon_url: string | null;
  slug: string | null;
  rank: number;
  max_rank: number;
  mastered: boolean;
  mastery_req: number | null;
}
export interface ResourceRow {
  unique_name: string;
  display_name: string;
  kind: string;
  icon_url: string | null;
  slug: string | null;
  count: number;
}
export interface CodexCategory {
  category: string;
  owned: number;
  total: number;
  mastered: number;
}
export interface LoreScanRow {
  display_name: string;
  scans: number;
}
export interface CodexData {
  has_data: boolean;
  categories: CodexCategory[];
  total_owned: number;
  total_items: number;
  total_mastered: number;
  total_mastery_points: number;
  lore_scans: LoreScanRow[];
}

// Worldstate (Rotation)
export interface Cycle {
  id: string;
  name: string;
  state: string;
  time_left: string | null;
  expiry: string | null;
}
export interface Fissure {
  tier: string;
  mission_type: string;
  node: string;
  enemy: string | null;
  expiry: string | null;
  eta: string | null;
  is_hard: boolean;
  is_storm: boolean;
}
export interface VendorItem {
  item: string;
  // Baro: ducats. Varzia: the wrapper reuses this key for the AYA cost.
  ducats: number | null;
  credits: number | null;
}
export interface Trader {
  active: boolean;
  activation: string | null;
  expiry: string | null;
  location: string | null;
  character: string | null;
  inventory: VendorItem[];
}
// Vendor stock enriched against the catalog (Vendors screen).
export interface VendorIntelRow {
  item: string;
  slug: string | null;
  thumbnail_url: string | null;
  median_plat: number | null;
  owned_qty: number;
  cost: number | null; // what the vendor charges, denominated in `currency`
  // "ducats" | "aya" | "regal_aya" | "steel_essence" — per ROW: Varzia mixes
  // aya (relics) and regal aya (frames/packs/cosmetics) in one stock list.
  currency: string;
  credits: number | null;
  cost_per_plat: number | null; // cost / median_plat (lower = better)
  good_deal: boolean;
  item_ref: string; // stable id for persisting manual checks
  tradeable: boolean; // resolved to a market slug → ownership auto-detect works
  checked: boolean; // owned (auto) or manually ticked
  check_source: "owned" | "manual" | null;
  rank: number | null; // syndicate rank gate (static vendors; null = ungated)
  bonus: string | null; // Eleanor's Coda rotation: preformatted "+45% Heat"; null elsewhere
}
// One vendor column on the Vendors board.
export interface VendorPanel {
  key: string; // "baro" | "varzia" | "steel_path"
  name: string;
  character: string | null;
  location: string | null;
  currency: string; // "ducats" | "aya" | "steel_essence"
  active: boolean;
  activation: string | null;
  expiry: string | null;
  rows: VendorIntelRow[];
}
// A wanted item available from a live reward source now (Rotation Overview).
export interface WantedNowRow {
  slug: string;
  display_name: string;
  source_label: string;
  eta: string | null;
}
// One owned stack of a relic at a refinement (storage is per refinement; the
// browser aggregates stacks into one row per relic identity).
export interface RelicStack {
  refinement: string;
  qty: number;
  source: string; // manual | de_scan
}
// One relic identity in the full-catalog relic browser — owned or not. EV is
// drop-based and squad-aware (best-of-N radshare). Powers the Relics screen.
export interface RelicBrowserRow {
  tier: string;
  relic_name: string;
  display_name: string;
  vaulted: boolean;
  aya: boolean; // in Varzia's current Prime Resurgence stock (buyable for Aya)
  protected: boolean;
  qty: number; // total owned across refinements (0 = catalog-only row)
  stacks: RelicStack[];
  ev_plat: number; // qty-weighted across owned stacks at squad size; Intact when unowned
  ducat_ev: number; // linear ducat EV per crack
  drops_owned: number; // slug-resolvable rewards owned ≥1…
  drops_total: number; // …of the slug-resolvable rewards (Forma excluded)
  drop_names: string[]; // reward names, for drops:/text search
  sets: CrackSet[]; // one-away sets this relic can finish
  wanted: boolean; // drops a watch/buy-list item
  best_reward: string | null;
  best_reward_plat: number | null;
  rare_reward: string | null; // the gold-tier (lowest-chance) drop; null on flat tables
  rare_plat: number | null; // its price, null when unpriced
  score: number; // burn priority (set > wanted > now > EV); protection applied UI-side
}
// A drop chance at one refinement (Requiem relics may list fewer than four).
export interface RefinementChance {
  refinement: string;
  chance: number; // percent
}
// One reward row in the relic drawer's drop table.
export interface RelicDetailDrop {
  reward_name: string;
  reward_slug: string | null; // null = untradeable (Forma, Requiem mods)
  chances: RefinementChance[];
  plat: number | null;
  ducats: number | null;
  owned_qty: number;
  wanted: boolean;
  set: boolean;
  set_slug: string | null;
  rare: boolean; // the gold-tier (lowest-chance) reward
}
// Per-refinement economics for the relic drawer: EV, radshare odds, refine-ROI.
export interface RelicRefinementInfo {
  refinement: string;
  owned_qty: number;
  ev_plat: number; // at the requested squad size
  ev_solo: number; // linear single-player EV
  ducat_ev: number;
  p_rare: number; // P(≥1 rare across the squad), 0–1
  trace_cost: number | null; // from Intact: 25 / 50 / 100; null for Intact
  ev_delta: number | null; // ev_plat − Intact ev_plat
  plat_per_100_traces: number | null;
}
// One reward read off the in-game reward-selection screen by the relic-OCR
// capture (mirrors src-tauri types.rs::CrackReward).
export interface CrackReward {
  reward_name: string;
  slug: string | null; // null = untradeable (Forma, Kuva)
  plat: number | null;
  ducats: number | null;
  ducats_per_plat: number | null; // high = Baro fodder
  owned_qty: number;
  wanted: boolean;
  set_slug: string | null; // completes a one-away set
  confidence: number; // OCR match confidence in [0.7, 1]
  best: boolean; // the recommended pick (see pick_reason)
  card_index: number; // 0-based on-screen card position, left to right
  unread: boolean; // card detected but not readable — render a "?" panel
  pick_reason: "wanted" | "set" | "price" | null; // why `best` is the pick
}
// Result of one reward-screen capture (mirrors types.rs::CrackCapture).
export interface CrackCapture {
  captured_at: string; // RFC3339
  rewards: CrackReward[];
  ocr_lines: string[]; // per-card OCR text (diagnostics)
  capture_ms: number;
  ocr_ms: number;
  error: string | null;
}
// Everything the relic drawer shows for one relic identity.
export interface RelicDetail {
  tier: string;
  relic_name: string;
  display_name: string;
  vaulted: boolean;
  aya: boolean; // in Varzia's current Resurgence stock
  protected: boolean;
  squad_size: number;
  stacks: RelicStack[];
  refinements: RelicRefinementInfo[]; // only refinements with a drop table
  drops: RelicDetailDrop[]; // highest-value first
}
// A relic that drops a given item — the item Drawer's reverse lookup.
export interface RelicSourceRow {
  tier: string;
  relic_name: string;
  display_name: string;
  vaulted: boolean;
  owned_qty: number;
  chance_intact: number | null; // percent
  chance_radiant: number | null;
}
// Live progress tick for "Update game data" (game-data-progress event). total 0 =
// indeterminate (sweeping bar); otherwise current/total is a fraction.
export interface GameDataProgress {
  step: number;
  steps: number;
  label: string;
  current: number;
  total: number;
}
// Result summary of the "Update game data" action (Settings → Data & cache).
export interface GameDataUpdate {
  catalog_new: number;
  catalog_total: number;
  vault_refreshed: boolean;
  sets_synced: number;
  relics_new: number;
  relics_total: number;
  relics_refreshed: boolean;
  manifest_total: number;
  manifest_refreshed: boolean;
}
// A set this relic helps finish (one part away) — a backlink target on the Sets screen.
export interface CrackSet {
  slug: string;
  name: string;
}
export interface SortieMission {
  node: string;
  mission_type: string;
  modifier: string | null;
  modifier_desc: string | null;
}
// One shape for the daily sortie AND the weekly archon hunt (no modifiers).
export interface Sortie {
  boss: string;
  faction: string;
  activation: string | null;
  expiry: string | null;
  missions: SortieMission[];
}
export interface SpReward {
  name: string;
  cost: number | null; // Steel Essence
}
export interface SteelPath {
  current_reward: SpReward | null;
  activation: string | null;
  expiry: string | null;
  rotation: SpReward[];
}
export interface Arbitration {
  node: string;
  mission_type: string;
  enemy: string | null;
  tier: string | null; // community S–D rating (browse.wf), null = unrated
  activation: string;
  expiry: string;
}
export interface ArbitrationBlock {
  current: Arbitration | null;
  upcoming: Arbitration[];
  /** Next few S/A-tier arbitrations from the whole schedule (can be days out). */
  notable: Arbitration[];
}
export interface NightwaveChallenge {
  id: string; // warframestat row id — check-off identity
  title: string;
  desc: string | null;
  reputation: number;
  is_daily: boolean;
  is_elite: boolean;
  expiry: string | null;
  checked: boolean;
  check_source: "scan" | "manual" | null;
}
export interface Nightwave {
  season: number | null;
  expiry: string | null; // season end
  challenges: NightwaveChallenge[]; // biggest standing first
}
export interface Invasion {
  node: string;
  attacker: string;
  defender: string;
  attacker_reward: string | null;
  defender_reward: string | null;
  completion: number; // attacker-side progress, 0–100
  eta: string | null;
}
// Startup health + DB backups
export interface StartupStatus {
  ok: boolean;
  error: string | null;
  db_path: string | null;
}
export interface BackupInfo {
  file_name: string;
  size_bytes: number;
  modified_at: string;
}

export interface PricingProgress {
  active: boolean;
  priced: number;
  total: number;
  // When prices last changed (launch drain / manual refresh / live heartbeat);
  // drives the topbar "live · Xs" indicator.
  last_price_sync: string | null;
}

// Desktop-notification + close-to-tray preferences (one JSON blob in app_settings).
export interface NotificationPrefs {
  master_enabled: boolean;
  close_to_tray: boolean;
  s_tier_arbitration: boolean;
  void_cascade: boolean;
  vendor_arrival: boolean;
  daily_reset: boolean;
  weekly_reset: boolean;
  // Daily background check for a new WFIT version (in-app notification;
  // deliberately not gated by master_enabled — it's not an OS toast).
  auto_check_updates: boolean;
}

/** App-update check result (mirrors types.rs::UpdateStatus). */
export interface UpdateStatus {
  current_version: string;
  latest_version: string | null;
  update_available: boolean;
  // True when this install can self-update in place (Windows installers,
  // Linux AppImage); false = deb/rpm/bare binary → point at GitHub instead.
  in_place: boolean;
  notes: string | null;
}

/** `update-download-progress` event payload (mirrors types.rs::UpdateProgress). */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** Cascade HUD overlay prefs (mirrors db::settings::OverlayPrefs). */
export interface OverlayPrefs {
  enabled: boolean;
  hotkey: string; // accelerator, e.g. "Alt+KeyC"
  duration_secs: number;
}

/** Relic-crack capture prefs (mirrors db::settings::RelicOcrPrefs). */
export interface RelicOcrPrefs {
  enabled: boolean;
  hotkey: string; // accelerator, e.g. "Alt+KeyT"
  duration_secs: number;
}

/** Cascade overlay status (mirrors worldstate::CascadeStatus). */
export interface CascadeStatus {
  active: boolean;
  tier: string | null;
  node: string | null;
  is_hard: boolean;
  expiry: string | null; // RFC3339 — "time left" when active
  omnia_reset: string | null; // RFC3339 — "time till reset" when inactive
}

/** One week of the Duviri Circuit (mirrors worldstate::CircuitWeek). */
export interface CircuitWeek {
  activation: string | null; // ISO — week start
  expiry: string | null; // ISO — next weekly reset
  incarnons: string[]; // EXC_HARD choices — plain weapon names ("Braton")
  frames: string[]; // EXC_NORMAL choices — warframe names
}

export interface Worldstate {
  cycles: Cycle[];
  fissures: Fissure[];
  baro: Trader | null;
  varzia: Trader | null;
  sortie: Sortie | null;
  archon_hunt: Sortie | null;
  steel_path: SteelPath | null;
  circuit: CircuitWeek | null; // DE-only; carried forward when DE is down
  nightwave: Nightwave | null;
  invasions: Invasion[];
  arbitration: ArbitrationBlock | null;
  fetched_at: string;
  source_timestamp: string | null; // warframestat.us snapshot time; null if absent
  // "de" = fissures cross-checked against DE's raw worldstate (the normal,
  // authoritative case); "warframestat" = wrapper-only fallback (DE unreachable).
  fissure_source: "de" | "warframestat";
}

// ---- rivens (separate API: v2 reference + v1 auction search) ----
export interface RivenWeapon {
  slug: string;
  name: string;
  riven_type: string; // rifle|pistol|shotgun|melee|zaw|kitgun|archgun
  group: string;
  disposition: number;
}

export interface RivenEstimate {
  point: number;
  low: number;
  high: number;
  confidence: "low" | "medium" | "high";
  comps_used: number;
  rationale: string;
}

export interface RivenDeal {
  kind: "great" | "fair" | "overpriced";
  delta_pct: number; // + above expected, - below
  expected: number;
}

export interface RivenAttribute {
  slug: string;
  name: string;
  unit: string | null; // percent|seconds|multiply|null
  exclusive_to: string[] | null; // null = any weapon
  positive_is_negative: boolean; // true = a "positive" roll is bad (e.g. recoil)
}

export interface RivenResultAttr {
  slug: string;
  name: string;
  value: number;
  positive: boolean;
  unit: string | null;
  grade: number | null; // % of god roll (positives only)
  wanted: boolean; // matches one of the user's desired stats
}

export interface RivenResult {
  id: string;
  riven_name: string;
  weapon_url_name: string;
  weapon_name: string;
  mastery_level: number;
  mod_rank: number;
  re_rolls: number;
  polarity: string;
  attributes: RivenResultAttr[];
  buyout_price: number | null;
  starting_price: number | null;
  top_bid: number | null;
  is_direct_sell: boolean;
  owner_name: string;
  owner_status: "ingame" | "online" | "offline" | string;
  owner_reputation: number;
  grade: number | null; // overall mean grade of gradeable positives
  match_tier: number; // 0 exact … 4 weapon-only
  matched_positives: number;
  created: string;
  updated: string;
  deal: RivenDeal | null;
}

export interface RivenPriceSummary {
  min: number | null;
  median: number | null;
  count: number;
}

export interface RivenSearchResponse {
  results: RivenResult[];
  summary: RivenPriceSummary;
  graded: boolean; // false when disposition unknown → grades shown as "—"
  estimate: RivenEstimate | null;
}

// Mirrors the Rust RivenQuery (snake_case fields, sent as the `query` arg).
export interface RivenQuery {
  weapon: string;
  positives: string[];
  negative: string | null;
  polarity: string | null;
  re_rolls_max: number | null;
  mastery_rank_max: number | null;
}

export interface RivenSavedSearch {
  id: number;
  label: string;
  weapon: string;
  positives: string[];
  negative: string | null;
  polarity: string | null;
  re_rolls_max: number | null;
  mastery_rank_max: number | null;
  // slug → value threshold (positive = min %, negative = max magnitude).
  min_values: Record<string, number>;
  // when true, the background watcher notifies on a matching auction.
  notify: boolean;
  created_at: string;
}

// One entry in the in-app notification center (mirrors Rust db::notifications::Notification).
export interface AppNotification {
  id: number;
  kind: string; // producer category, e.g. "riven"
  title: string;
  body: string;
  nav_screen: string | null; // screen to open on click
  nav_slug: string | null; // optional item slug to open in the Drawer
  payload: string | null; // producer JSON (e.g. { saved_search_id })
  created_at: string;
  read_at: string | null; // null = unread
}
