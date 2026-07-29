//! App self-update. Rust owns the whole flow (the `@tauri-apps/plugin-updater`
//! JS package is deliberately not used): a `check` command for Settings, an
//! `install` command streaming download progress as Tauri events, and a daily
//! background check that files an in-app notification — backend-driven like
//! `notify.rs`, because a hidden WebKitGTK window throttles JS timers.
//!
//! Install-format gate: the updater plugin can only replace Windows installs
//! (NSIS/MSI) and Linux AppImages. deb/rpm installs and bare dev binaries must
//! NEVER invoke the plugin (newer plugin versions attempt pkexec-driven deb/rpm
//! installs, and a bare binary trips its AppImage path detection) — they get a
//! plain latest.json version compare and a "download from GitHub" answer.

use crate::db::notifications::{self, NewNotification};
use crate::db::settings;
use crate::error::{AppError, AppResult};
use crate::types::{UpdateProgress, UpdateStatus};
use crate::AppState;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;
use tauri_plugin_updater::UpdaterExt;

/// Release feed. One source of truth for both the plugin path and the fallback
/// GET; `WFIT_UPDATE_ENDPOINT` overrides it for local chain testing.
const ENDPOINT: &str =
    "https://github.com/finneritter/Warframe-Item-Tracker/releases/latest/download/latest.json";
/// Daily background cadence + a warmup so launch traffic (catalog/price drain)
/// isn't competing with a same-instant update check.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const WARMUP: Duration = Duration::from_secs(120);

fn endpoint() -> String {
    std::env::var("WFIT_UPDATE_ENDPOINT").unwrap_or_else(|_| ENDPOINT.to_string())
}

/// Can this install self-update in place? Windows: yes (NSIS/MSI). Linux: only
/// when running from an AppImage (the `APPIMAGE` env var is set by the runtime).
/// Everything else (deb/rpm, bare binary, macOS source builds): no.
fn supported_install() -> bool {
    if cfg!(target_os = "windows") {
        return true;
    }
    if cfg!(target_os = "linux") {
        return std::env::var_os("APPIMAGE").is_some();
    }
    false
}

fn plugin_err(e: tauri_plugin_updater::Error) -> AppError {
    AppError::Updater(e.to_string())
}

/// The subset of latest.json the fallback path needs.
#[derive(Deserialize)]
struct Feed {
    version: String,
    notes: Option<String>,
}

/// Strip an optional leading "v" (tauri-action writes the tag name verbatim).
fn parse_version(s: &str) -> Option<semver::Version> {
    semver::Version::parse(s.trim().trim_start_matches('v')).ok()
}

/// Check for an update. Supported installs go through the updater plugin
/// (which also verifies the feed's signature material); unsupported installs
/// do a plain GET + semver compare so they can at least point at GitHub.
pub async fn check(app: &tauri::AppHandle) -> AppResult<UpdateStatus> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    if supported_install() {
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint()
                .parse()
                .map_err(|e| AppError::Updater(format!("bad endpoint: {e}")))?])
            .map_err(plugin_err)?
            .build()
            .map_err(plugin_err)?;
        let update = updater.check().await.map_err(plugin_err)?;
        return Ok(match update {
            Some(u) => UpdateStatus {
                current_version: current,
                latest_version: Some(u.version.clone()),
                update_available: true,
                in_place: true,
                notes: u.body.clone(),
            },
            None => UpdateStatus {
                current_version: current,
                latest_version: None,
                update_available: false,
                in_place: true,
                notes: None,
            },
        });
    }

    // Fallback: plain fetch of the feed. Its own tiny client — this is not
    // warframe.market traffic, so it stays away from the market throttle.
    let http = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(AppError::Http)?;
    let feed: Feed = http
        .get(endpoint())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let latest = parse_version(&feed.version)
        .ok_or_else(|| AppError::Updater(format!("unparseable feed version {:?}", feed.version)))?;
    let cur = parse_version(&current)
        .ok_or_else(|| AppError::Updater("unparseable own version".into()))?;
    let newer = latest > cur;
    Ok(UpdateStatus {
        current_version: current,
        latest_version: newer.then(|| latest.to_string()),
        update_available: newer,
        in_place: false,
        notes: if newer { feed.notes } else { None },
    })
}

/// Download + install (supported installs only). Progress goes out as
/// `update-download-progress` events, throttled to whole-percent ticks (or
/// every 512 KiB when the server sends no length). NOTE: on Windows the
/// plugin hands off to the installer and EXITS THE PROCESS inside this call;
/// only Linux (AppImage) returns for an explicit restart.
pub async fn install(app: tauri::AppHandle) -> AppResult<()> {
    if !supported_install() {
        return Err(AppError::Invalid(
            "this install can't self-update — download the new version from GitHub".into(),
        ));
    }
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint()
            .parse()
            .map_err(|e| AppError::Updater(format!("bad endpoint: {e}")))?])
        .map_err(plugin_err)?
        .build()
        .map_err(plugin_err)?;
    let update = updater
        .check()
        .await
        .map_err(plugin_err)?
        .ok_or_else(|| AppError::NotFound("no update available".into()))?;

    let progress_app = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_mark: u64 = 0; // last emitted percent, or byte bucket when total unknown
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let mark = match total {
                    Some(t) if t > 0 => downloaded * 100 / t,
                    _ => downloaded / (512 * 1024),
                };
                if mark != last_mark {
                    last_mark = mark;
                    let _ = progress_app.emit(
                        "update-download-progress",
                        UpdateProgress { downloaded, total },
                    );
                }
            },
            {
                let app = app.clone();
                move || {
                    let _ = app.emit("update-download-finished", ());
                }
            },
        )
        .await
        .map_err(plugin_err)?;
    Ok(())
}

/// Daily background check → in-app notification (never an OS toast, never an
/// auto-install). Deduped per version via the notification center's dedup_key,
/// so a dismissed version stays dismissed and each new version fires once.
/// Skipped in dev builds — `cargo run` is a bare binary and would spam the
/// fallback wording at every session.
pub fn spawn_update_check(state: Arc<AppState>, app: tauri::AppHandle) {
    if cfg!(debug_assertions) && std::env::var_os("WFIT_UPDATE_ENDPOINT").is_none() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(WARMUP).await;
        loop {
            let enabled = settings::notification_prefs(&state.db)
                .map(|p| p.auto_check_updates)
                .unwrap_or(true);
            if enabled {
                match check(&app).await {
                    Ok(s) if s.update_available => {
                        let version = s.latest_version.unwrap_or_default();
                        let body = if s.in_place {
                            "Open Settings › About to install it.".to_string()
                        } else {
                            "This install can't self-update — grab it from GitHub Releases."
                                .to_string()
                        };
                        let filed = notifications::insert_deduped(
                            &state.db,
                            &NewNotification {
                                kind: "update".into(),
                                dedup_key: Some(format!("update:{version}")),
                                title: format!("WFIT v{version} is available"),
                                body,
                                nav_screen: Some("settings".into()),
                                nav_slug: None,
                                // Deep-link: land on (and flash) the Updates row
                                // instead of the top of a long Settings page.
                                payload: Some(r#"{"settings_section":"updates"}"#.into()),
                            },
                        );
                        match filed {
                            Ok(n) if n > 0 => {
                                let _ = app.emit("notifications-updated", n);
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!(error = %e, "update notification failed"),
                        }
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!(error = %e, "update check failed"),
                }
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_tolerates_v_prefix() {
        assert_eq!(
            parse_version("v1.2.0"),
            semver::Version::parse("1.2.0").ok()
        );
        assert_eq!(parse_version("1.2.0"), semver::Version::parse("1.2.0").ok());
        assert!(parse_version("not-a-version").is_none());
    }

    #[test]
    fn feed_comparison_orders_correctly() {
        let newer = parse_version("1.2.0").unwrap();
        let cur = parse_version(env!("CARGO_PKG_VERSION")).unwrap();
        // Guards the fallback compare direction; bump alongside real versions.
        assert!(newer > semver::Version::parse("1.1.0").unwrap());
        assert!(cur >= semver::Version::parse("1.1.0").unwrap());
    }
}
