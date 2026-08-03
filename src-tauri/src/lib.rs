mod commands;
mod db;
// Dev-only tooling (dashboard/metrics/faults). The module is always present so
// hot-path shims resolve, but its server + axum dep compile only under the
// `dev-dashboard` feature — see devtools/mod.rs.
mod devtools;
mod domain;
mod error;
mod gamescan;
mod hotkeys;
mod market;
mod notify;
mod overlay;
mod relic_ocr;
mod rivens;
mod types;
mod updater;
mod wfm_account;
mod wfm_socket;
mod worldstate;

use chrono::{DateTime, Duration, Utc};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// One User-Agent for every outbound request; tracks the crate version so a
/// release bump can never leave a stale version string behind.
pub const USER_AGENT: &str = concat!("wfit-desktop/", env!("CARGO_PKG_VERSION"));

/// Public re-exports for criterion benches only (feature `bench`). Lets `benches/`
/// reach the otherwise crate-private valuation hot paths without widening the
/// normal public API. Never enabled in release.
#[cfg(feature = "bench")]
pub mod bench_api {
    pub use crate::db::inventory::realizable_value;
}

pub struct AppState {
    pub db: db::Db,
    pub market: market::Market,
    pub worldstate: worldstate::WorldstateClient,
    /// Drives the warframe.market presence socket (online/ingame/invisible).
    pub presence: wfm_socket::Presence,
    /// True for the whole launch warm-up (catalog → vault → owned → drain), so the UI
    /// can show a "syncing…" indicator the entire time prices are still filling in —
    /// distinguishing "still loading" from a settled value (incl. a fresh post-wipe sync).
    pub pricing_active: AtomicBool,
    /// When true, closing the main window hides it to the tray instead of quitting.
    /// Mirrors the persisted notification pref; the close handler reads this (a DB
    /// read inside the window event loop would be needless), and the set-prefs
    /// command keeps it in sync. Forced false when the tray icon couldn't be built.
    pub close_to_tray: AtomicBool,
    /// Monotonic counter for the Cascade overlay: each hotkey press bumps it and
    /// captures the new value; the press's auto-hide timer only hides the window
    /// if the counter is unchanged when it fires. Lets a re-press cancel the
    /// previous hide and restart the on-screen duration without tracking handles.
    pub overlay_gen: AtomicU64,
    /// Same generation-counter pattern for the relic-crack overlay.
    pub relic_overlay_gen: AtomicU64,
    /// One-shot latch: a capture with an unreadable card triggers at most one
    /// background WFCD relic-data refresh per app run (new-Prime-release gap).
    pub relic_vocab_refresh: std::sync::atomic::AtomicBool,
    /// Most recent relic-crack capture (in-memory only — it's a snapshot of a
    /// 10-second on-screen moment, not durable data). The main window's "last
    /// capture" view and a rebuilt overlay both self-fetch from here.
    pub last_crack: parking_lot::Mutex<Option<types::CrackCapture>>,
}

/// Managed INSTEAD of AppState when startup fails (corrupt DB, failed
/// migration, unwritable data dir): the window still opens and the frontend
/// renders the recovery screen from this. Mutually exclusive with AppState —
/// `commands::startup_status` tells the frontend which mode it's in.
pub struct RecoveryInfo {
    pub error: String,
    pub db_path: std::path::PathBuf,
}

const CATALOG_STALE_HOURS: i64 = 24;
const PRICE_TTL_HOURS: i64 = 6;
const DRAIN_BATCH: i64 = 40;
// ---- live heartbeat (the app should feel alive, not snapshot-stale) ----
// A perpetual rolling repricer: every tick it refreshes the most time-sensitive
// stale slice, tiered watchlist → owned → rest-of-catalog. Budget math: worst
// case ~12 stats + ~6 order books per 45s tick ≈ 24 req/min peak (vs the 350ms
// throttle's ~170/min ceiling); steady state is far lower — ~800 owned items on
// a 60min cycle averages ~13 stats/min, watchlist is small, catalog rides the
// 6h TTL. Each tick that changed anything emits `prices-updated` so the UI
// refetches immediately instead of waiting for a poll.
const HEARTBEAT_SECS: u64 = 45;
const HEARTBEAT_BATCH: i64 = 12; // max statistics calls per tick
const HEARTBEAT_ORDER_BATCH: usize = 6; // max order-book calls per tick
const WATCH_FRESH_MINS: i64 = 10; // watchlist targets are the time-sensitive tier
const OWNED_FRESH_MINS: i64 = 60; // owned drives the headline value
const LISTINGS_SYNC_TICKS: u64 = 13; // listings piggyback every ~10 min (1 call)
                                     // Bump whenever the price-derivation logic changes. On launch a mismatch wipes the
                                     // derived price caches and recomputes them, so fixes take effect without a manual
                                     // "rebuild cache" and stale old-logic prices can't survive behind the TTL.
const PRICING_VERSION: &str = "4"; // 4: delta_7d = None (not 0%) when there's no prior window

/// Keeps the non-blocking file-log writer alive for the app's lifetime —
/// dropping it would silently stop file logging.
static LOG_GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> =
    std::sync::OnceLock::new();

/// Stdout (dev) + a daily-rolling file in `app_data_dir/logs/` (~7 days kept) —
/// the installed app has no visible stdout, and market/worldstate failures are
/// exactly the things that need diagnosing after the fact.
fn init_logging(log_dir: &std::path::Path) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // The appender's retention scan reads the dir at build time — make sure it
    // exists first or the very first launch prints a spurious error.
    let _ = std::fs::create_dir_all(log_dir);

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,wfit_lib=debug"));
    let stdout = tracing_subscriber::fmt::layer();

    let file_layer = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("wfit")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .ok()
        .map(|appender| {
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let _ = LOG_GUARD.set(guard);
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer)
        });

    tracing_subscriber::registry()
        .with(filter)
        .with(stdout)
        .with(file_layer) // Option<Layer> — file logging degrades gracefully
        .init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            // Global hotkeys (Cascade overlay + relic capture). The actual keys
            // are registered dynamically from prefs (hotkeys::apply_all); this
            // just installs the handler, which routes the fired shortcut to the
            // right feature. Fire on Pressed only — not the matching Released —
            // so one keypress = one show.
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        hotkeys::dispatch(app, shortcut);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            // Close-to-tray: hide the main window instead of quitting when the
            // pref is on (mirrored into AppState). `try_state` (not `state`) so
            // recovery mode — which has no AppState and also no OS titlebar —
            // falls through to a real close and never traps the user.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Overlay windows are hide-only (issue #3): letting a WM close
                // destroy one would take its hotkey with it for the rest of
                // the session (trigger() would find no window). Real
                // destruction is still recovered — the trigger paths rebuild
                // from config — but never let one die on purpose.
                if window.label() == "overlay" || window.label() == "relic-overlay" {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                if window.label() != "main" {
                    return;
                }
                let close_to_tray = window
                    .app_handle()
                    .try_state::<Arc<AppState>>()
                    .map(|s| s.close_to_tray.load(Ordering::Relaxed))
                    .unwrap_or(false);
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            // Updater plugin: registered here (the docs' v2 pattern) and
            // unconditionally — a recovery-mode session must still be able to
            // check for the fixed version.
            if let Err(e) = app
                .handle()
                .plugin(tauri_plugin_updater::Builder::new().build())
            {
                tracing::warn!(error = %e, "updater plugin failed to initialize");
            }
            // Tray icon: always built (even in recovery mode) so "Quit" is always
            // reachable. If it fails to build — e.g. no StatusNotifierItem host on
            // a bare Wayland session — close-to-tray is force-disabled in init_app
            // so the user can never be stranded with a hidden, unrecoverable window.
            let tray_ok = match build_tray(app.handle()) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!(error = %e, "tray icon unavailable — close-to-tray disabled");
                    false
                }
            };
            // app_data_dir failing is OS-level breakage; fall back so even that
            // lands in the recovery screen instead of a pre-window panic.
            let app_data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            init_logging(&app_data_dir.join("logs"));
            if let Err(e) = init_app(app, &app_data_dir, tray_ok) {
                tracing::error!(error = %e, "startup failed — entering recovery mode");
                app.manage(RecoveryInfo {
                    error: e.to_string(),
                    db_path: app_data_dir.join("wfit.sqlite"),
                });
            }
            // Never Err: the window must open either way so the user can act.
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // catalog
            commands::catalog_count,
            commands::catalog_refresh,
            commands::update_game_data,
            commands::get_catalog,
            commands::get_catalog_item,
            commands::search_catalog,
            // inventory
            commands::get_inventory,
            commands::add_to_inventory,
            commands::set_qty,
            commands::remove_item,
            commands::get_summary,
            // sales
            commands::record_sale,
            commands::undo_sale,
            commands::get_sales,
            // watchlist
            commands::get_watchlist,
            commands::add_watch,
            commands::remove_watch,
            commands::set_target,
            // buy list
            commands::get_buy_list,
            commands::add_to_buy_list,
            commands::set_buy_qty,
            commands::remove_buy,
            commands::purchase_buy,
            commands::get_budget,
            commands::set_budget,
            commands::get_excluded_rarities,
            commands::set_excluded_rarities,
            commands::get_excluded_min_plat,
            commands::set_excluded_min_plat,
            commands::get_excluded_min_plat_by_cat,
            commands::set_excluded_min_plat_by_cat,
            commands::get_rec_min_price,
            commands::set_rec_min_price,
            commands::get_notification_prefs,
            commands::set_notification_prefs,
            commands::send_test_notification,
            // cascade overlay
            commands::get_overlay_prefs,
            commands::set_overlay_prefs,
            commands::get_cascade_status,
            // relic-crack capture (issue #2)
            commands::get_relic_ocr_prefs,
            commands::set_relic_ocr_prefs,
            commands::test_relic_overlay,
            commands::trigger_relic_crack,
            commands::get_last_crack_capture,
            commands::relic_ocr_run_file,
            commands::get_pricing_progress,
            // computed
            commands::get_sets,
            commands::get_ducats,
            commands::get_arcane_dashboard,
            commands::get_collection_breakdown,
            commands::get_trends,
            commands::get_listing_recommendations,
            // prices / detail
            commands::prices_refresh,
            commands::get_item_detail,
            commands::get_item_history,
            commands::get_item_orders,
            commands::get_item_sellers,
            commands::rebuild_cache,
            commands::wipe_app,
            // backups
            commands::backup_now,
            commands::list_backups,
            commands::open_backups_dir,
            commands::dev_dashboard_url,
            commands::open_dev_dashboard,
            // developer — simulate fake inventory
            commands::simulate_inventory,
            commands::clear_simulated_inventory,
            // startup / recovery
            commands::startup_status,
            commands::recovery_backup_db,
            commands::recovery_reset_db,
            // worldstate
            commands::get_worldstate,
            commands::force_worldstate_refresh,
            commands::get_vendor_board,
            commands::get_vendor_group,
            commands::mark_vendor_check,
            commands::unmark_vendor_check,
            commands::clear_vendor_checks,
            commands::get_wanted_now,
            // relics
            commands::set_relic_qty,
            commands::get_relic_browser,
            commands::get_relic_detail,
            commands::set_relic_protected,
            commands::get_relic_sources,
            commands::import_scanned_relics,
            // wfm account
            commands::get_wfm_account,
            commands::wfm_connect,
            commands::wfm_set_session,
            commands::wfm_signout,
            commands::wfm_sync_listings,
            commands::wfm_get_listings,
            commands::wfm_create_order,
            commands::wfm_update_order,
            commands::wfm_delete_order,
            commands::wfm_mark_sold,
            commands::wfm_set_status,
            commands::get_recommended_price,
            commands::wfm_reprice_preview,
            commands::wfm_reprice_apply,
            // rivens
            commands::list_riven_weapons,
            commands::list_riven_attributes,
            commands::search_rivens,
            commands::list_riven_searches,
            commands::create_riven_search,
            commands::delete_riven_search,
            commands::riven_search_set_notify,
            // notification center
            commands::notifications_list,
            commands::notifications_unread_count,
            commands::notifications_mark_all_read,
            commands::notifications_dismiss,
            commands::notifications_clear_all,
            // app self-update
            commands::check_app_update,
            commands::install_app_update,
            commands::restart_app,
            // set composition (Pass B)
            commands::sets_refresh,
            // game inventory import (memory-scan)
            commands::game_scan_status,
            commands::game_scan_consent,
            commands::game_scan_revoke,
            commands::game_scan_preview,
            commands::game_scan_apply,
            commands::account_scan,
            commands::get_account_profile,
            commands::get_account_arsenal,
            commands::get_account_resources,
            commands::get_account_codex,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Build the system-tray icon + menu (Show / Quit). Left-click toggles the main
/// window; "Quit" calls `app.exit(0)`, which raises `ExitRequested` (NOT
/// `CloseRequested`), so it bypasses the close-to-tray interception and really
/// exits. Errs when the platform has no tray host — the caller disables
/// close-to-tray in that case.
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItem::with_id(app, "show", "Show WFIT", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("WFIT")
        .menu(&menu)
        .show_menu_on_left_click(false) // left-click toggles; menu on right-click
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

/// Reveal + focus the main window (from the tray "Show" item or a left-click).
fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Toggle the main window's visibility (tray left-click).
fn toggle_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

/// Everything fallible about startup: data dir, DB open/migration, state +
/// background tasks. On Err the caller manages [`RecoveryInfo`] instead of
/// AppState and the frontend shows the recovery screen.
fn init_app(
    app: &tauri::App,
    app_data_dir: &std::path::Path,
    tray_ok: bool,
) -> error::AppResult<()> {
    std::fs::create_dir_all(app_data_dir)?;
    let db_path = app_data_dir.join("wfit.sqlite");
    tracing::info!(?db_path, "opening database");

    let db = db::Db::open(&db_path)?;
    // Presence is per-session and not restored: we hold no socket until the
    // user picks a status, so reset the mirror to invisible on launch (it
    // naturally clears to offline when WFIT closes).
    let _ = db::wfm::set_status(&db, "invisible");
    // Close-to-tray follows the persisted pref, but only if the tray actually
    // built — otherwise hiding the window would leave no way to bring it back.
    let close_to_tray = tray_ok
        && db::settings::notification_prefs(&db)
            .unwrap_or_default()
            .close_to_tray;
    let (presence, presence_rx) = wfm_socket::Presence::new();
    let state = Arc::new(AppState {
        db,
        market: market::Market::new(),
        worldstate: worldstate::WorldstateClient::new(),
        presence,
        pricing_active: AtomicBool::new(false),
        close_to_tray: AtomicBool::new(close_to_tray),
        overlay_gen: AtomicU64::new(0),
        relic_overlay_gen: AtomicU64::new(0),
        relic_vocab_refresh: std::sync::atomic::AtomicBool::new(false),
        last_crack: parking_lot::Mutex::new(None),
    });
    app.manage(state.clone());

    // Register every enabled global hotkey (Cascade overlay + relic capture)
    // from the persisted prefs. Failures degrade gracefully inside.
    hotkeys::apply_all(app.handle());

    // Pre-warm the OCR engine off the startup path when the relic capture is
    // enabled: model deserialization costs ~100ms, and the first hotkey press
    // during a 10-second reward window must not pay it.
    #[cfg(feature = "relic-ocr")]
    if db::settings::relic_ocr_prefs(&state.db)
        .map(|p| p.enabled)
        .unwrap_or(false)
    {
        tauri::async_runtime::spawn_blocking(|| {
            if let Err(e) = relic_ocr::ocr::warm() {
                tracing::warn!(error = %e, "relic_ocr: engine pre-warm failed");
            }
        });
    }

    // Dev-only local stress/observability dashboard (feature `dev-dashboard`).
    // Loopback-only; `WFIT_DASH_PORT=0` disables. It does NOT auto-open — Settings ›
    // Developer has an "Open dashboard" button (the URL is published on bind).
    #[cfg(feature = "dev-dashboard")]
    {
        let port: u16 = std::env::var("WFIT_DASH_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8848);
        if port != 0 {
            tauri::async_runtime::spawn(devtools::server::serve(
                state.clone(),
                db_path.clone(),
                port,
            ));
        }
    }

    // Presence keeper: holds the warframe.market socket open while the
    // user is online/ingame so their orders show active to buyers.
    tauri::async_runtime::spawn(wfm_socket::supervisor(presence_rx));

    // Keep the worldstate cache confirmed-fresh every ~3min — the
    // Rotation screen's own poll stops when the window is backgrounded.
    state.worldstate.spawn_refresher();

    // Live price heartbeat: rolling tiered repricer + UI notifications,
    // so data keeps arriving the whole session, not just at launch.
    spawn_price_heartbeat(state.clone(), app.handle().clone());

    // Desktop-notification engine: watches world-state and fires OS toasts for
    // the user's enabled event categories (works while the window is hidden).
    notify::spawn(state.clone(), app.handle().clone());
    updater::spawn_update_check(state.clone(), app.handle().clone());

    // In-app riven watcher: periodically runs notify-enabled saved riven searches
    // and files notification-center entries for matching auctions.
    rivens::watch::spawn_riven_watch(state.clone(), app.handle().clone());

    // Kick off catalog/price warm-up off the UI thread; never block launch.
    tauri::async_runtime::spawn(async move {
        if let Err(e) = launch_refresh(state).await {
            tracing::warn!(error = %e, "launch refresh failed");
        }
    });
    Ok(())
}

/// RAII flag for the "syncing…" indicator: marks `pricing_active` true for its
/// lifetime and false on drop, so the flag clears on every exit path of the warm-up
/// (normal completion or an early error) and can never get stuck on.
struct PricingGuard<'a>(&'a AtomicBool);
impl<'a> PricingGuard<'a> {
    fn new(b: &'a AtomicBool) -> Self {
        b.store(true, Ordering::Relaxed);
        Self(b)
    }
}
impl Drop for PricingGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Relaxed);
    }
}

/// On launch: refresh the catalog if empty/stale, prime owned + watchlist prices,
/// then drain the rest in the background at the throttled rate. UI never blocks
/// and the 350 ms global limit is never exceeded.
pub(crate) async fn launch_refresh(state: Arc<AppState>) -> error::AppResult<()> {
    use db::{catalog, meta, prices, relic_data, vault};

    // Hold the "syncing…" flag for the entire warm-up (catalog → vault → owned →
    // drain) so the UI shows progress the whole time; the guard resets it on any exit.
    let _active = PricingGuard::new(&state.pricing_active);

    // 0) If the pricing logic changed since these caches were built, wipe the
    // DERIVED price caches (not raw history) so everything re-derives below with
    // the current logic. Owned prices repopulate first, the rest via the drain.
    if meta::get(&state.db, meta::KEY_PRICING_VERSION)?.as_deref() != Some(PRICING_VERSION) {
        tracing::info!("pricing logic changed → clearing price caches for a clean reprice");
        state.db.with(|c| {
            c.execute_batch(
                "DELETE FROM price_cache; DELETE FROM price_rank; DELETE FROM order_cache; DELETE FROM buy_orders; DELETE FROM order_fetch_meta;",
            )?;
            Ok(())
        })?;
        meta::set(&state.db, meta::KEY_PRICING_VERSION, PRICING_VERSION)?;
    }

    // 0.5) Backfill mod rarity into existing catalog rows from the bundled dataset
    // (no network; version-gated so it runs once). New mods get it via the upsert.
    let filled = catalog::backfill_mod_rarity(&state.db)?;
    if filled > 0 {
        tracing::info!(n = filled, "backfilled mod rarity");
    }

    // 1) Catalog skeleton (1 call) if empty or older than the stale window.
    let need_catalog = {
        let count = catalog::count(&state.db)?;
        if count == 0 {
            true
        } else if catalog::missing_game_ref_count(&state.db)? > 0 {
            // 0003 added game_ref; existing rows are NULL until one refetch backfills
            // them from /v2/items `gameRef`. Needed for the game-inventory mapping.
            true
        } else if !catalog::has_any_max_rank(&state.db)? {
            // 0004 added max_rank; one refetch backfills it from /v2/items `maxRank`
            // (needed for rank-aware mod/arcane valuation + the drawer breakdown).
            true
        } else {
            match meta::get(&state.db, meta::KEY_LAST_CATALOG_SYNC)? {
                Some(ts) => DateTime::parse_from_rfc3339(&ts)
                    .map(|t| Utc::now().signed_duration_since(t.with_timezone(&Utc)))
                    .map(|age| age > Duration::hours(CATALOG_STALE_HOURS))
                    .unwrap_or(true),
                None => true,
            }
        }
    };
    if need_catalog {
        tracing::info!("launch: refreshing catalog");
        let items = state.market.fetch_catalog().await?;
        catalog::upsert_many(&state.db, &items)?;
        meta::set(
            &state.db,
            meta::KEY_LAST_CATALOG_SYNC,
            &Utc::now().to_rfc3339(),
        )?;
    }

    // 1.5) Vault status from warframe-items (own host; TTL-gated ~monthly, bundled
    // fallback). Runs after the catalog so the set→parts propagation has rows to mark.
    if let Err(e) = vault::refresh_if_stale(&state.db).await {
        tracing::warn!(error = %e, "vault status refresh failed");
    }

    // 1.6) Relic reference data: seed the DB tables from the bundled snapshot (or
    // re-seed if a newer bundle shipped), then mirror the DB into the in-memory store.
    if let Err(e) = relic_data::seed_if_empty_or_stale(&state.db)
        .and_then(|()| relic_data::load_into_memory(&state.db))
    {
        tracing::warn!(error = %e, "relic data load failed; using bundled defaults");
    }
    // …then refresh from live WFCD when the last sync is older than the TTL. Vault
    // status rotates with every Prime Access / Resurgence cycle, so a bundled or
    // months-old snapshot shows the currently-farmable relics as VAULT. Best-effort
    // (own host, not the warframe.market throttle); failure keeps existing data.
    {
        const RELIC_REF_TTL_DAYS: i64 = 3;
        let need = match meta::get(&state.db, meta::KEY_LAST_RELIC_SYNC)? {
            Some(ts) => DateTime::parse_from_rfc3339(&ts)
                .map(|t| {
                    Utc::now().signed_duration_since(t.with_timezone(&Utc))
                        > Duration::days(RELIC_REF_TTL_DAYS)
                })
                .unwrap_or(true),
            None => true,
        };
        if need {
            tracing::info!("launch: refreshing relic data from WFCD");
            if let Err(e) = relic_data::refresh(&state.db).await {
                tracing::warn!(error = %e, "relic data refresh failed; keeping existing data");
            }
        }
    }

    // 1.65) Set-membership quantities (issue #1): the old set pass silently
    // parsed nothing (snake_case deserializer vs camelCase API), so every
    // install has an empty set_membership table. One background re-pass per
    // version marker (~157 throttled calls, ~1 min behind the price prime);
    // failure retries on the next launch.
    if meta::get(&state.db, meta::KEY_SET_MEMBERSHIP_PASS)?.as_deref() != Some("2") {
        let st = state.clone();
        tauri::async_runtime::spawn(async move {
            tracing::info!("launch: backfilling set_membership quantities");
            match commands::sets_refresh_background(&st).await {
                Ok(n) => tracing::info!(rows = n, "set membership backfill done"),
                Err(e) => tracing::warn!(error = %e, "set membership backfill failed"),
            }
        });
    }

    // 1.7) Item manifest (Account screen's non-tradeable name/icon/mastery source):
    // seed from the bundled TSV (or re-seed on a newer bundle). Live refresh via
    // "Update game data". Same bundled-baseline pattern as relics.
    if let Err(e) = crate::db::account::seed_if_empty_or_stale(&state.db) {
        tracing::warn!(error = %e, "item manifest seed failed; Account names may be sparse");
    }

    // 1.8) Riven reference (weapons + attributes, incl. disposition) for the Riven
    // Search tab — 2 v2 calls, refreshed when the cache is empty or older than the
    // TTL (disposition only changes per Prime Access). Best-effort; a search also
    // lazily refreshes on a cold cache.
    {
        const RIVEN_REF_TTL_DAYS: i64 = 14;
        let need = crate::db::rivens::weapons_empty(&state.db)?
            || match meta::get(&state.db, meta::KEY_LAST_RIVEN_REF_SYNC)? {
                Some(ts) => DateTime::parse_from_rfc3339(&ts)
                    .map(|t| {
                        Utc::now().signed_duration_since(t.with_timezone(&Utc))
                            > Duration::days(RIVEN_REF_TTL_DAYS)
                    })
                    .unwrap_or(true),
                None => true,
            };
        if need {
            match crate::rivens::ensure_reference(&state).await {
                Ok(()) => {
                    meta::set(
                        &state.db,
                        meta::KEY_LAST_RIVEN_REF_SYNC,
                        &Utc::now().to_rfc3339(),
                    )?;
                }
                Err(e) => tracing::warn!(error = %e, "riven reference refresh failed"),
            }
        }
    }

    // 2+3) Owned (then watchlist) pricing — the phase the inventory value depends on.
    // pricing_active is held for the WHOLE warm-up by the guard at the top of this fn,
    // so the "syncing…" indicator stays up through the background drain below too.
    // Owned orders go first (primary price), then the slower statistics pass.
    refresh_owned_orders(&state, false).await?;
    let mut priority = prices::stale_inventory_slugs(&state.db)?;
    for s in prices::stale_watchlist_slugs(&state.db)? {
        if !priority.contains(&s) {
            priority.push(s);
        }
    }
    refresh_slugs(&state, &priority).await?;
    if !priority.is_empty() {
        meta::set(
            &state.db,
            meta::KEY_LAST_PRICE_SYNC,
            &Utc::now().to_rfc3339(),
        )?;
    }

    // 4) Background drain of everything else, oldest-first, batch by batch.
    loop {
        let batch = prices::stale_catalog_slugs(&state.db, DRAIN_BATCH)?;
        if batch.is_empty() {
            break;
        }
        refresh_slugs(&state, &batch).await?;
        meta::set(
            &state.db,
            meta::KEY_LAST_PRICE_SYNC,
            &Utc::now().to_rfc3339(),
        )?;
    }
    tracing::info!("launch: price drain complete");
    Ok(())
}

/// Perpetual freshness loop ("the app should feel alive"). Every tick it
/// refreshes the stalest tiered slice — watchlist → owned → catalog tail —
/// plus a few owned order books, and (every ~10 min) the listings mirror.
/// Skips while a launch warm-up / manual refresh holds the throttle. Each
/// tick that changed anything stamps `last_price_sync` and emits
/// `prices-updated`, which the frontend listens for to refetch immediately.
fn spawn_price_heartbeat(state: Arc<AppState>, app: tauri::AppHandle) {
    use tauri::Emitter;
    tauri::async_runtime::spawn(async move {
        let mut tick: u64 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_SECS)).await;
            tick += 1;
            if state.pricing_active.load(Ordering::Relaxed) {
                continue; // a full sync owns the throttle; we'd be redundant
            }
            let mut changed = match heartbeat_tick(&state).await {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(error = %e, "heartbeat tick failed");
                    continue;
                }
            };
            // Listings mirror rides along occasionally (a single API call).
            if tick % LISTINGS_SYNC_TICKS == 0 {
                match commands::sync_listings_impl(&state).await {
                    Ok(r) => changed += r.mirrored,
                    Err(error::AppError::NotConnected(_)) => {} // no account — fine
                    Err(e) => tracing::warn!(error = %e, "heartbeat listings sync failed"),
                }
            }
            // Record every tick (even no-change) so the dashboard sees cadence + age.
            devtools::rec_heartbeat(changed as u64);
            if changed > 0 {
                tracing::debug!(changed, "heartbeat: refreshed");
                let _ = db::meta::set(
                    &state.db,
                    db::meta::KEY_LAST_PRICE_SYNC,
                    &Utc::now().to_rfc3339(),
                );
                let _ = app.emit("prices-updated", changed);
            }
        }
    });
}

/// One heartbeat pass: statistics for the stalest watchlist → owned → catalog
/// slugs (oldest-first within each tier), then live order books for the
/// stalest owned subset. Returns how many slugs were touched.
async fn heartbeat_tick(state: &Arc<AppState>) -> error::AppResult<usize> {
    use db::prices;

    let watch_cutoff = (Utc::now() - Duration::minutes(WATCH_FRESH_MINS)).to_rfc3339();
    let owned_cutoff = (Utc::now() - Duration::minutes(OWNED_FRESH_MINS)).to_rfc3339();

    let mut batch =
        prices::slugs_older_than(&state.db, "watchlist", &watch_cutoff, HEARTBEAT_BATCH)?;
    for s in prices::slugs_older_than(&state.db, "inventory_items", &owned_cutoff, HEARTBEAT_BATCH)?
    {
        if batch.len() >= HEARTBEAT_BATCH as usize {
            break;
        }
        if !batch.contains(&s) {
            batch.push(s);
        }
    }
    // Spare budget drains the long catalog tail (6h TTL), oldest-first.
    let spare = HEARTBEAT_BATCH - batch.len() as i64;
    if spare > 0 {
        for s in prices::stale_catalog_slugs(&state.db, spare)? {
            if !batch.contains(&s) {
                batch.push(s);
            }
        }
    }
    let mut touched = batch.len();
    if !batch.is_empty() {
        refresh_slugs(state, &batch).await?;
    }

    // Live order books — the primary price for owned items — for the stalest few.
    let mut order_slugs = prices::owned_order_slugs(&state.db, &owned_cutoff)?;
    order_slugs.truncate(HEARTBEAT_ORDER_BATCH);
    for slug in &order_slugs {
        match state.market.fetch_order_book(slug).await {
            // An empty book is stored too: it clears stale ladders and stamps
            // freshness so the slug isn't refetched every tick.
            Ok(Some(book)) => {
                prices::store_order_book(&state.db, slug, &book.sells, &book.bids)?;
                touched += 1;
            }
            Ok(None) => {} // non-2xx (warned in the client); keep stale data
            Err(e) => tracing::warn!(slug, error = %e, "heartbeat order book failed"),
        }
    }
    Ok(touched)
}

/// Fetch + persist statistics for each slug (throttled inside the client).
async fn refresh_slugs(state: &Arc<AppState>, slugs: &[String]) -> error::AppResult<()> {
    use db::prices;
    let mut updates = Vec::new();
    for slug in slugs {
        match state.market.fetch_statistics(slug).await {
            Ok(Some(p)) => updates.push(p),
            Ok(None) => {}
            Err(e) => tracing::warn!(slug, error = %e, "fetch_statistics failed"),
        }
        // Persist in small chunks so progress survives and the UI sees data early.
        if updates.len() >= 20 {
            prices::upsert_many(&state.db, &updates, Duration::hours(PRICE_TTL_HOURS))?;
            updates.clear();
        }
    }
    if !updates.is_empty() {
        prices::upsert_many(&state.db, &updates, Duration::hours(PRICE_TTL_HOURS))?;
    }
    Ok(())
}

/// Fetch + store live lowest-sell prices for illiquid owned items, so their value
/// tracks real asks instead of sparse/gameable trade statistics. Throttled inside
/// the market client. Bounded to the (usually small) illiquid-owned subset.
pub(crate) async fn refresh_owned_orders(
    state: &Arc<AppState>,
    force: bool,
) -> error::AppResult<()> {
    use db::prices;
    // force → cutoff = now (nothing is fresher, so refetch all); else skip orders
    // refreshed within the price TTL.
    let cutoff = if force {
        Utc::now().to_rfc3339()
    } else {
        (Utc::now() - Duration::hours(PRICE_TTL_HOURS)).to_rfc3339()
    };
    let slugs = prices::owned_order_slugs(&state.db, &cutoff)?;
    if slugs.is_empty() {
        return Ok(());
    }
    tracing::info!(
        n = slugs.len(),
        "refreshing live sell orders for owned items"
    );
    let mut priced = 0usize;
    for slug in &slugs {
        match state.market.fetch_order_book(slug).await {
            Ok(Some(book)) => {
                prices::store_order_book(&state.db, slug, &book.sells, &book.bids)?;
                priced += 1;
            }
            Ok(None) => {} // non-2xx (warned in the client); keep stale data
            Err(e) => tracing::warn!(slug, error = %e, "fetch_order_book failed"),
        }
    }
    tracing::info!(priced, of = slugs.len(), "live sell orders refreshed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_tracks_crate_version() {
        assert_eq!(
            USER_AGENT,
            format!("wfit-desktop/{}", env!("CARGO_PKG_VERSION"))
        );
        assert!(USER_AGENT.starts_with("wfit-desktop/1."));
    }
}
