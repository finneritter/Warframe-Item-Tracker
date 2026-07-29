//! Game inventory import — memory-scan of the running Warframe client.
//!
//! ISOLATED, like `worldstate.rs`: own concern, never on the warframe.market data
//! path. Opt-in, consent-gated (typed phrase), off by default. Supported on
//! **Linux and Windows**; macOS is unsupported (SIP/hardened-runtime block reading
//! another process's memory, and Warframe has no native Mac client).
//! **ToS-prohibited and ban-risky** — see `docs/GAME_INVENTORY_IMPORT.md`.
//!
//! Build status: Linux verified live (2026-06-01); Windows implemented and
//! type-checked, runtime-unverified pending a Windows test. `consent`/`map`/`scan`
//! are unit-tested. The portable search/parse lives in `scan.rs`; each OS supplies a
//! `MemReader` (`memory_linux` via `/proc/<pid>/{maps,mem}`, `memory_windows` via
//! `VirtualQueryEx`/`ReadProcessMemory`). Signature `accountId=<24 hex>&nonce=<digits>`,
//! endpoint `mobile.warframe.com/api/inventory.php`. No upstream code is copied.

pub mod account;
pub mod consent;
pub mod fingerprint;
pub mod map;

// The portable scan core is always compiled so its tests run on every host; it's
// only *used* on supported OSes (hence the dead-code allowance elsewhere).
#[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
mod scan;

pub mod season;

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod api;

#[cfg(target_os = "linux")]
#[path = "process_linux.rs"]
mod process;
#[cfg(target_os = "windows")]
#[path = "process_windows.rs"]
mod process;

#[cfg(target_os = "linux")]
#[path = "memory_linux.rs"]
mod memory;
#[cfg(target_os = "windows")]
#[path = "memory_windows.rs"]
mod memory;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
use crate::error::AppError;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};

/// A raw inventory line as read from the game (DE `uniqueName` + count), before
/// it is mapped onto the catalog. `rank` is the mod/arcane rank (0 for unranked
/// stacks, the fingerprint `lvl` for ranked instances) or None for prime parts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawItem {
    pub unique_name: String,
    pub count: i64,
    pub rank: Option<i64>,
}

/// A normalized scan result: the owned lines plus the account id (kept only to
/// detect a different-account scan; the nonce is never carried here).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawInventory {
    pub account_id: Option<String>,
    pub items: Vec<RawItem>,
}

/// A resolved owned line: a catalog slug + quantity at a given rank (post-mapping).
/// `rank` is None for non-ranked items (prime parts); Some(n) for mods/arcanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanItem {
    pub slug: String,
    pub rank: Option<i64>,
    pub qty: i64,
}

/// A resolved owned VOID RELIC line (relics aren't catalog items, so they resolve to
/// a relic identity, not a slug). `refinement` is Intact for an uncracked relic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelicScanItem {
    pub tier: String,
    pub name: String,
    pub refinement: String,
    pub qty: i64,
}

/// Whether the platform can support memory-scanning at all. Linux + Windows;
/// macOS is unsupported (SIP/hardened runtime block reading another process).
pub const fn is_supported() -> bool {
    cfg!(target_os = "linux") || cfg!(target_os = "windows")
}

/// Best-effort detection of a running Warframe client. Process listing only (NOT
/// memory). Returns false on unsupported platforms.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn warframe_running() -> bool {
    process::find_pid().is_some()
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn warframe_running() -> bool {
    false
}

/// One scan's three parses of the SAME inventory.php blob: the tradeable-item inventory
/// (existing item/relic path), the Account snapshot (Account screen), and completed
/// Nightwave acts (SeasonChallengeHistory).
pub struct ScanResult {
    pub inventory: RawInventory,
    pub account: account::AccountSnapshot,
    /// Completed Nightwave acts (SeasonChallengeHistory) — third parse of the blob.
    pub season: Vec<season::CompletedAct>,
}

/// Perform a live scan: process → memory (accountId + nonce) → DE inventory endpoint →
/// one blob parsed three ways (`RawInventory` + `AccountSnapshot` + completed Nightwave acts). Linux + Windows.
pub async fn scan() -> AppResult<ScanResult> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        real_scan().await
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(AppError::Invalid(
            "game inventory scan is not supported on this OS".into(),
        ))
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
async fn real_scan() -> AppResult<ScanResult> {
    use crate::error::AppError;

    let pid = process::find_pid()
        .ok_or_else(|| AppError::NotConnected("Warframe is not running".into()))?;

    // Linux: ptrace_scope 2/3 always blocks the read — fail early with guidance.
    #[cfg(target_os = "linux")]
    if let Some(scope) = process::ptrace_scope() {
        if scope >= 2 {
            return Err(AppError::Invalid(format!(
                "kernel.yama.ptrace_scope={scope} blocks reading game memory. Set it to 0 \
                 (sysctl -w kernel.yama.ptrace_scope=0) or grant WFIT CAP_SYS_PTRACE."
            )));
        }
    }

    // The memory scan is blocking and can touch GBs — keep it off the async runtime.
    let session = tokio::task::spawn_blocking(move || memory::read_session(pid))
        .await
        .map_err(|e| AppError::Other(format!("memory scan task failed: {e}")))??
        .ok_or_else(|| {
            AppError::Other(
                "couldn't find the game session in memory — make sure you're fully logged in"
                    .into(),
            )
        })?;

    let json = api::fetch_inventory(&session.account_id, &session.nonce).await?;
    // Opt-in raw capture for debugging the parser against reality (the field shapes
    // are undocumented and have burned us before). Local disk only, never uploaded.
    if std::env::var("WFIT_GAMESCAN_DUMP").is_ok() {
        let path = std::env::temp_dir().join("wfit-inventory-blob.json");
        match serde_json::to_vec_pretty(&json)
            .map_err(|e| e.to_string())
            .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
        {
            Ok(()) => tracing::info!(path = %path.display(), "gamescan: dumped raw inventory blob"),
            Err(e) => tracing::warn!(error = %e, "gamescan: blob dump failed"),
        }
    }
    let mut inv = map::parse_inventory(&json);
    inv.account_id = Some(session.account_id.clone()); // trust the scanned id over the body
    let mut acct = account::parse_account(&json);
    acct.account_id = Some(session.account_id);
    let season = season::parse_season_history(&json);
    let season_acts = season.len();
    tracing::info!(
        lines = inv.items.len(),
        gear = acct.gear.len(),
        season_acts = season_acts,
        "gamescan: parsed inventory + account + season"
    );
    Ok(ScanResult {
        inventory: inv,
        account: acct,
        season,
    })
}
