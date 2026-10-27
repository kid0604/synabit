//! Bước 0's proof: a page that tries every way out of the visual frame, and a
//! command that counts the times one of them reached Rust.
//!
//! Debug builds only. Run it from Syn's settings with developer mode on; the
//! results go into appendix A of `docs/syn-visuals-plan-2026-10-26.md`.
//!
//! The page cannot know whether an IPC message it sent was handled — a message
//! with the wrong invoke key is dropped without an answer, like one that never
//! arrived. So every IPC attempt it makes names [`visual_spike_ping`], and the
//! panel asks [`visual_spike_pings`] afterwards: anything above zero means a
//! frame reached the app's commands.

use std::sync::atomic::{AtomicU32, Ordering};

pub const ATTACK_PAGE: &str = include_str!("spike_attack.html");

/// A visual as Syn would draw one, using every part of the kit: d3, KaTeX and
/// its fonts, a slider, the host's theme, a question asked back. It reports
/// what worked as `synabit/selftest`. Served through `protocol::assemble`,
/// like any visual.
pub const KIT_PAGE: &str = include_str!("spike_kit.html");

static PINGS: AtomicU32 = AtomicU32::new(0);

/// The command the attack page aims at. Reaching it is the failure.
#[tauri::command]
pub fn visual_spike_ping(webview: tauri::Webview, request: tauri::ipc::Request<'_>) {
    PINGS.fetch_add(1, Ordering::SeqCst);
    let origin = request.headers().get("Origin").and_then(|v| v.to_str().ok()).unwrap_or("-");
    log::error!(
        "[visual-spike] IPC reached Rust: webview '{}', origin {origin}, url {}",
        webview.label(),
        webview.url().map(|u| u.to_string()).unwrap_or_default()
    );
}

/// How many times [`visual_spike_ping`] ran since the last reset.
#[tauri::command]
pub fn visual_spike_pings() -> u32 {
    PINGS.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn visual_spike_reset() {
    PINGS.store(0, Ordering::SeqCst);
}

/// Where an unattended run's report goes: `SYNVISUAL_SPIKE_OUT`, or the
/// system's temporary folder.
fn report_path() -> std::path::PathBuf {
    std::env::var_os("SYNVISUAL_SPIKE_OUT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("synvisual-spike.json"))
}

/// The front end's report of an unattended run, written where it can be read
/// without anyone at the machine. See `autorunVisualSpike` in `visualSpike.ts`.
#[tauri::command]
pub fn visual_spike_report(report: serde_json::Value) -> Result<String, String> {
    let path = report_path();
    let text = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    log::warn!(
        "[visual-spike] report written to {} — leaked: {}, broken: {}, unclear: {}",
        path.display(),
        report["leaked"],
        report["broken"],
        report["unclear"]
    );
    Ok(path.display().to_string())
}
