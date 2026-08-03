// Thin invoke() wrappers around the Rust command surface. All domain transforms
// live in Rust; these just type the boundary.
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import type {
  AccountProfile,
  AppNotification,
  ArcaneBreakdown,
  ArcaneDashboard,
  BackupInfo,
  BuyRow,
  CascadeStatus,
  CatalogRow,
  CodexData,
  CrackCapture,
  DucatRow,
  GameDataUpdate,
  GameScanStatus,
  GearRow,
  InventoryRow,
  ItemDetail,
  ItemOrders,
  ItemSellers,
  ListingRow,
  NotificationPrefs,
  OverlayPrefs,
  PricingProgress,
  RecommendationRow,
  RelicBrowserRow,
  RelicDetail,
  RelicOcrPrefs,
  RelicSourceRow,
  RepriceApply,
  RepriceRow,
  ResourceRow,
  RivenAttribute,
  RivenQuery,
  RivenSavedSearch,
  RivenSearchResponse,
  RivenWeapon,
  SaleRow,
  ScanApply,
  ScanDiffRow,
  SetRow,
  SimSummary,
  StartupStatus,
  Summary,
  SyncResult,
  TrendsData,
  UpdateStatus,
  VendorPanel,
  WantedNowRow,
  WatchRow,
  WfmAccount,
  Worldstate,
} from "./types";

// app
export const appVersion = () => getVersion();
// self-update (Settings › About). install exits the process on Windows.
export const checkAppUpdate = () => invoke<UpdateStatus>("check_app_update");
export const installAppUpdate = () => invoke<void>("install_app_update");
export const restartApp = () => invoke<void>("restart_app");

// startup / recovery (work without AppState — usable on the recovery screen)
export const startupStatus = () => invoke<StartupStatus>("startup_status");
export const recoveryBackupDb = () => invoke<string>("recovery_backup_db");
export const recoveryResetDb = () => invoke<void>("recovery_reset_db");

// backups
export const backupNow = () => invoke<string>("backup_now");
export const listBackups = () => invoke<BackupInfo[]>("list_backups");
export const openBackupsDir = () => invoke<void>("open_backups_dir");

// dev dashboard (only running when built with --features dev-dashboard)
export const devDashboardUrl = () => invoke<string | null>("dev_dashboard_url");
export const openDevDashboard = () => invoke<void>("open_dev_dashboard");

// catalog
export const catalogCount = () => invoke<number>("catalog_count");
export const catalogRefresh = () => invoke<number>("catalog_refresh");
export const updateGameData = () => invoke<GameDataUpdate>("update_game_data");
export const rebuildCache = () => invoke<number>("rebuild_cache");
export const wipeApp = () => invoke<void>("wipe_app");
export const setsRefresh = () => invoke<number>("sets_refresh");

// developer — simulate a fake owned inventory for local testing. `fill` is how
// full the account is (1–100 % of the catalog owned).
export const simulateInventory = (fill: number) =>
  invoke<SimSummary>("simulate_inventory", { fill });
export const clearSimulatedInventory = () => invoke<void>("clear_simulated_inventory");
export const getCatalog = (category?: string) =>
  invoke<CatalogRow[]>("get_catalog", { category: category ?? null });
export const getCatalogItem = (slug: string) =>
  invoke<CatalogRow | null>("get_catalog_item", { slug });
export const searchCatalog = (q: string, limit?: number) =>
  invoke<CatalogRow[]>("search_catalog", { q, limit: limit ?? null });

// inventory
export const getInventory = () => invoke<InventoryRow[]>("get_inventory");
export const addToInventory = (slug: string, qty?: number) =>
  invoke<number>("add_to_inventory", { slug, qty: qty ?? null });
export const setQty = (slug: string, qty: number) => invoke<number>("set_qty", { slug, qty });
export const removeItem = (slug: string) => invoke<void>("remove_item", { slug });
export const getSummary = () => invoke<Summary>("get_summary");

// sales
export const recordSale = (slug: string, qty?: number, platPerUnit?: number, notes?: string) =>
  invoke<number>("record_sale", {
    slug,
    qty: qty ?? null,
    platPerUnit: platPerUnit ?? null,
    notes: notes ?? null,
  });
export const undoSale = (id: number) => invoke<void>("undo_sale", { id });
export const getSales = (limit?: number) =>
  invoke<SaleRow[]>("get_sales", { limit: limit ?? null });

// watchlist
export const getWatchlist = () => invoke<WatchRow[]>("get_watchlist");
export const addWatch = (slug: string, target?: number) =>
  invoke<void>("add_watch", { slug, target: target ?? null });
export const removeWatch = (slug: string) => invoke<void>("remove_watch", { slug });
export const setTarget = (slug: string, target?: number) =>
  invoke<void>("set_target", { slug, target: target ?? null });

// buy list
export const getBuyList = () => invoke<BuyRow[]>("get_buy_list");
export const addToBuyList = (slug: string, qty?: number) =>
  invoke<void>("add_to_buy_list", { slug, qty: qty ?? null });
export const setBuyQty = (slug: string, qty: number) => invoke<void>("set_buy_qty", { slug, qty });
export const removeBuy = (slug: string) => invoke<void>("remove_buy", { slug });
export const purchaseBuy = (slug: string) => invoke<number>("purchase_buy", { slug });
export const getBudget = () => invoke<number | null>("get_budget");
export const setBudget = (value: number) => invoke<void>("set_budget", { value });

export const getExcludedRarities = () => invoke<string[]>("get_excluded_rarities");
export const setExcludedRarities = (rarities: string[]) =>
  invoke<void>("set_excluded_rarities", { rarities });
export const getExcludedMinPlat = () => invoke<number>("get_excluded_min_plat");
export const setExcludedMinPlat = (value: number) =>
  invoke<void>("set_excluded_min_plat", { value });
export const getExcludedMinPlatByCat = () =>
  invoke<Record<string, number>>("get_excluded_min_plat_by_cat");
export const setExcludedMinPlatByCat = (thresholds: Record<string, number>) =>
  invoke<void>("set_excluded_min_plat_by_cat", { thresholds });
export const getRecMinPrice = () => invoke<number>("get_rec_min_price");
export const setRecMinPrice = (value: number) => invoke<void>("set_rec_min_price", { value });
export const getPricingProgress = () => invoke<PricingProgress>("get_pricing_progress");

// notifications + close-to-tray
export const getNotificationPrefs = () => invoke<NotificationPrefs>("get_notification_prefs");
export const setNotificationPrefs = (prefs: NotificationPrefs) =>
  invoke<void>("set_notification_prefs", { prefs });
export const sendTestNotification = () => invoke<void>("send_test_notification");

// cascade overlay
export const getOverlayPrefs = () => invoke<OverlayPrefs>("get_overlay_prefs");
export const setOverlayPrefs = (prefs: OverlayPrefs) =>
  invoke<void>("set_overlay_prefs", { prefs });
export const getCascadeStatus = () => invoke<CascadeStatus>("get_cascade_status");

// relic-crack capture (issue #2)
export const getRelicOcrPrefs = () => invoke<RelicOcrPrefs>("get_relic_ocr_prefs");
export const setRelicOcrPrefs = (prefs: RelicOcrPrefs) =>
  invoke<void>("set_relic_ocr_prefs", { prefs });
export const triggerRelicCrack = () => invoke<CrackCapture>("trigger_relic_crack");
export const testRelicOverlay = () => invoke<void>("test_relic_overlay");
export const getLastCrackCapture = () => invoke<CrackCapture | null>("get_last_crack_capture");
export const relicOcrRunFile = (path: string) =>
  invoke<CrackCapture>("relic_ocr_run_file", { path });

// computed
export const getSets = () => invoke<SetRow[]>("get_sets");
export const getDucats = () => invoke<DucatRow[]>("get_ducats");
export const getArcaneDashboard = () => invoke<ArcaneDashboard>("get_arcane_dashboard");
export const getCollectionBreakdown = (key: string) =>
  invoke<ArcaneBreakdown[]>("get_collection_breakdown", { key });
export const getTrends = (timeframe?: string, excludeOutliers = true) =>
  invoke<TrendsData>("get_trends", { timeframe: timeframe ?? null, excludeOutliers });
export const getListingRecommendations = () =>
  invoke<RecommendationRow[]>("get_listing_recommendations");

// prices / detail
export const pricesRefresh = (slugs?: string[], force?: boolean) =>
  invoke<number>("prices_refresh", { slugs: slugs ?? null, force: force ?? null });
export const getItemDetail = (slug: string) => invoke<ItemDetail>("get_item_detail", { slug });
export const getItemOrders = (slug: string) => invoke<ItemOrders>("get_item_orders", { slug });
export const getItemSellers = (slug: string) => invoke<ItemSellers>("get_item_sellers", { slug });

// worldstate
export const getWorldstate = () => invoke<Worldstate>("get_worldstate");
// Hard reset: discard the backend's worldstate + arbitration caches and re-fetch.
export const forceWorldstateRefresh = () => invoke<Worldstate>("force_worldstate_refresh");
// The Vendors board: one panel per rotating vendor, stock enriched + check-off state.
export const getVendorBoard = () => invoke<VendorPanel[]>("get_vendor_board");
export const getVendorGroup = (group: string) =>
  invoke<VendorPanel[]>("get_vendor_group", { group });
export const markVendorCheck = (vendorKey: string, itemRef: string) =>
  invoke<void>("mark_vendor_check", { vendorKey, itemRef });
export const unmarkVendorCheck = (vendorKey: string, itemRef: string) =>
  invoke<void>("unmark_vendor_check", { vendorKey, itemRef });
export const clearVendorChecks = (vendorKey: string) =>
  invoke<void>("clear_vendor_checks", { vendorKey });
// Wanted items (watchlist + missing set parts) farmable from a live reward source.
export const getWantedNow = () => invoke<WantedNowRow[]>("get_wanted_now");

// relics (full-catalog browser — drop-EV valued, squad-aware)
export const getRelicBrowser = (squadSize: number) =>
  invoke<RelicBrowserRow[]>("get_relic_browser", { squadSize });
export const getRelicDetail = (tier: string, relicName: string, squadSize: number) =>
  invoke<RelicDetail>("get_relic_detail", { tier, relicName, squadSize });
export const setRelicProtected = (tier: string, relicName: string, protected_: boolean) =>
  invoke<void>("set_relic_protected", { tier, relicName, protected: protected_ });
export const getRelicSources = (slug: string) =>
  invoke<RelicSourceRow[]>("get_relic_sources", { slug });
export const setRelicQty = (tier: string, name: string, refinement: string | null, qty: number) =>
  invoke<void>("set_relic_qty", { tier, name, refinement, qty });
export const importScannedRelics = () => invoke<number>("import_scanned_relics");

// wfm account
export const getWfmAccount = () => invoke<WfmAccount>("get_wfm_account");
export const wfmConnect = (username: string) => invoke<WfmAccount>("wfm_connect", { username });
export const wfmSetSession = (jwt: string) => invoke<WfmAccount>("wfm_set_session", { jwt });
export const wfmSignout = () => invoke<void>("wfm_signout");
export const wfmSyncListings = () => invoke<SyncResult>("wfm_sync_listings");
export const wfmGetListings = () => invoke<ListingRow[]>("wfm_get_listings");
export const wfmCreateOrder = (args: {
  slug: string;
  platinum: number;
  quantity: number;
  rank: number | null;
  visible: boolean;
}) => invoke<number>("wfm_create_order", args);
export const wfmUpdateOrder = (args: {
  orderId: string;
  platinum: number;
  quantity: number;
  visible: boolean;
}) => invoke<number>("wfm_update_order", args);
export const wfmDeleteOrder = (orderId: string) => invoke<number>("wfm_delete_order", { orderId });
export const wfmMarkSold = (orderId: string) => invoke<number>("wfm_mark_sold", { orderId });
export const wfmSetStatus = (status: string) => invoke<WfmAccount>("wfm_set_status", { status });
export const getRecommendedPrice = (slug: string, rank: number | null) =>
  invoke<number | null>("get_recommended_price", { slug, rank });
export const wfmRepricePreview = () => invoke<RepriceRow[]>("wfm_reprice_preview");
export const wfmRepriceApply = (orders: RepriceApply[]) =>
  invoke<number>("wfm_reprice_apply", { orders });

// game inventory import (memory-scan) — opt-in, consent-gated, Linux-only
export const gameScanStatus = () => invoke<GameScanStatus>("game_scan_status");
export const gameScanConsent = (phrase: string) => invoke<void>("game_scan_consent", { phrase });
export const gameScanRevoke = () => invoke<void>("game_scan_revoke");
export const gameScanPreview = () => invoke<ScanDiffRow[]>("game_scan_preview");
export const gameScanApply = (rows: ScanApply[]) => invoke<number>("game_scan_apply", { rows });

// account section (scan-populated Profile / Codex / Resources / Arsenal)
export const accountScan = () => invoke<AccountProfile>("account_scan");
export const getAccountProfile = () => invoke<AccountProfile>("get_account_profile");
export const getAccountArsenal = () => invoke<GearRow[]>("get_account_arsenal");
export const getAccountResources = () => invoke<ResourceRow[]>("get_account_resources");
export const getAccountCodex = () => invoke<CodexData>("get_account_codex");

// rivens (separate API: v2 reference + v1 auction search)
export const listRivenWeapons = () => invoke<RivenWeapon[]>("list_riven_weapons");
export const listRivenAttributes = () => invoke<RivenAttribute[]>("list_riven_attributes");
export const searchRivens = (query: RivenQuery, limit?: number) =>
  invoke<RivenSearchResponse>("search_rivens", { query, limit: limit ?? null });
export const listRivenSearches = () => invoke<RivenSavedSearch[]>("list_riven_searches");
export const createRivenSearch = (
  label: string,
  query: RivenQuery,
  minValues: Record<string, number>,
) => invoke<number>("create_riven_search", { label, query, minValues });
export const deleteRivenSearch = (id: number) => invoke<void>("delete_riven_search", { id });
export const setRivenSearchNotify = (id: number, enabled: boolean) =>
  invoke<void>("riven_search_set_notify", { id, enabled });

// notification center
export const listNotifications = () => invoke<AppNotification[]>("notifications_list");
export const notificationsUnreadCount = () => invoke<number>("notifications_unread_count");
export const markNotificationsRead = () => invoke<void>("notifications_mark_all_read");
export const dismissNotification = (id: number) => invoke<void>("notifications_dismiss", { id });
export const clearNotifications = () => invoke<void>("notifications_clear_all");
