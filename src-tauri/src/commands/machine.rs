//! The rooms, as the rest of the app reaches them (`syn::machine`).
//!
//! Stopping is here rather than behind the model on purpose: the tray, quitting
//! and the screen all stop rooms without asking anything that could be talked out
//! of it. `app_shell::gate` keeps these to the main window.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::syn::machine::doors::{self, Door, OpenFailed, Protected, Refused};
use crate::syn::machine::rooms::{Rooms, Status};
use crate::syn::machine::selftest::Report;

fn rooms(app: &AppHandle) -> Result<Arc<Rooms>, String> {
    app.try_state::<Arc<Rooms>>()
        .map(|rooms| rooms.inner().clone())
        .ok_or_else(|| "the rooms could not be set up at startup".to_string())
}

/// At startup: finds the rooms, and ends whatever a crash left running in them.
/// On a thread, since no window should wait for it. Runs no attacks: nothing has
/// asked for a room yet, and the feature is off until somebody turns it on.
pub fn setup(app: &AppHandle) {
    let root = match app.path().app_data_dir() {
        Ok(data) => data.join("machine"),
        Err(error) => {
            log::warn!("machine: no data folder: {error}");
            return;
        }
    };
    let rooms = match Rooms::at(&root) {
        Ok(rooms) => Arc::new(rooms),
        Err(error) => {
            log::warn!("machine: could not set up {}: {error}", root.display());
            return;
        }
    };
    app.manage(rooms.clone());
    // A batch the last run did not finish: said in the log, and on the screen,
    // where undo puts it back.
    for batch in crate::syn::machine::apply::batches(rooms.root()) {
        if batch.standing == crate::syn::machine::apply::Standing::Interrupted {
            log::warn!("machine: a batch of changes to {} was cut short; it can be undone from Settings", batch.root);
        }
    }
    std::thread::spawn(move || log_stopped("left by the last run", rooms.stop_all()));
}

fn log_stopped(which: &str, stopped: std::io::Result<usize>) {
    match stopped {
        Ok(0) => {}
        Ok(count) => log::info!("machine: stopped {count} processes {which}"),
        Err(error) => log::warn!("machine: could not stop everything {which}: {error}"),
    }
}

/// From the tray, which must not wait.
pub fn stop_all_from_tray(app: &AppHandle) {
    if let Ok(rooms) = rooms(app) {
        std::thread::spawn(move || log_stopped("from the tray", rooms.stop_all()));
    }
}

/// While the app exits, on the way out by whichever door. It takes as long as the
/// kernel takes, a moment and bounded, and what it misses the next launch clears.
pub fn stop_all_on_exit(app: &AppHandle) {
    if let Ok(rooms) = rooms(app) {
        log_stopped("on the way out", rooms.stop_all());
    }
}

#[tauri::command]
pub async fn syn_machine_status(app: AppHandle) -> Result<Status, String> {
    Ok(rooms(&app)?.status().await)
}

/// Attacks the room now. For when the switch is turned on, and for the screen.
#[tauri::command]
pub async fn syn_machine_check(app: AppHandle) -> Result<Report, String> {
    Ok(rooms(&app)?.check().await)
}

/// How many processes that ended.
#[tauri::command]
pub async fn syn_machine_stop_all(app: AppHandle) -> Result<usize, String> {
    let rooms = rooms(&app)?;
    tokio::task::spawn_blocking(move || rooms.stop_all())
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// A door, as the screen lists it.
#[derive(serde::Serialize)]
pub struct DoorView {
    #[serde(flatten)]
    pub door: Door,
    pub expires: chrono::DateTime<chrono::Utc>,
}

/// What came of putting the folder dialog up.
#[derive(serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Picked {
    Opened { door: Door, examples: Vec<String> },
    Refused { why: Refused },
    Cancelled,
}

/// The folder dialog, and the door it opens. The path is the person's answer to
/// a dialog this process put up, never a string from the page; `title` is only
/// what the dialog says, in the screen's language.
async fn pick(app: &AppHandle, vault_path: &str, title: Option<String>) -> Result<Picked, String> {
    let start = app.path().document_dir().ok();
    let Some(picked) = crate::app_shell::dialogs::ask_folder(app, title, start).await.map_err(|e| e.to_string())? else {
        return Ok(Picked::Cancelled);
    };
    let protected = Protected::of(app, vault_path).ok_or("this Mac's folders could not be found")?;
    let vault = vault_path.to_string();
    let opened = tokio::task::spawn_blocking(move || doors::open(&vault, std::path::Path::new(&picked), &protected, chrono::Utc::now()))
        .await
        .map_err(|e| e.to_string())?;
    match opened {
        Ok(opened) => Ok(Picked::Opened { door: opened.door, examples: opened.examples }),
        Err(OpenFailed::Refused(why)) => Ok(Picked::Refused { why }),
        Err(other) => Err(other.to_string()),
    }
}

/// The folders open to the rooms.
#[tauri::command]
pub async fn syn_doors(vault_path: String) -> Result<Vec<DoorView>, String> {
    Ok(doors::all(&vault_path, chrono::Utc::now())
        .into_iter()
        .map(|door| DoorView { expires: door.expires(), door })
        .collect())
}

/// Opens a folder from the screen, before anything has asked for one.
#[tauri::command]
pub async fn syn_add_door(app: AppHandle, vault_path: String, title: Option<String>) -> Result<Picked, String> {
    pick(&app, &vault_path, title).await
}

/// Answers a card asking for a folder by choosing one. Only a folder chosen
/// answers it: a closed dialog, or a folder that cannot be a door, leaves the
/// card where it is. The call that asked takes the door when the work carries
/// on (`doors::take_pick`).
#[tauri::command]
pub async fn syn_pick_door(app: AppHandle, vault_path: String, run_id: String, title: Option<String>) -> Result<Picked, String> {
    let run = crate::syn::run::get_run(&vault_path, &run_id).map_err(|e| e.to_string())?;
    let asking = run.pending_consent.as_ref().is_some_and(|ask| ask.capability == crate::syn::consent::Capability::Door);
    let (true, Some(conversation)) = (asking, run.conversation_id.clone()) else {
        return Err("nothing is asking for a folder".into());
    };
    let picked = pick(&app, &vault_path, title).await?;
    if let Picked::Opened { door, .. } = &picked {
        doors::picked_for(&conversation, &door.id);
        crate::commands::syn::syn_answer_consent(vault_path, run_id, crate::syn::consent::Answer::Once)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(picked)
}

/// Closes a door. A room reading it reads it no more from its next command.
#[tauri::command]
pub async fn syn_remove_door(vault_path: String, id: String) -> Result<bool, String> {
    doors::remove(&vault_path, &id, chrono::Utc::now()).map_err(|e| e.to_string())
}

/// Whether a door may be used from Telegram and in routines too.
#[tauri::command]
pub async fn syn_door_away(vault_path: String, id: String, away: bool) -> Result<bool, String> {
    doors::set_away(&vault_path, &id, away, chrono::Utc::now()).map_err(|e| e.to_string())
}

// ── Rehearsals: the card, the apply, and undo (design §9) ─────────────

/// One proposed change, as the card shows it: Rust's words, names made safe.
#[derive(serde::Serialize)]
pub struct OpView {
    pub n: usize,
    pub what: crate::syn::machine::rehearse::What,
    pub from: Option<String>,
    pub to: Option<String>,
    pub size: u64,
    pub class: crate::syn::machine::rehearse::Class,
    pub why: Option<crate::syn::machine::rehearse::Why>,
}

#[derive(serde::Serialize)]
pub struct ProposalView {
    pub id: String,
    pub door_name: String,
    pub root: String,
    pub made: chrono::DateTime<chrono::Utc>,
    pub unchanged: usize,
    pub read_outside: bool,
    pub ops: Vec<OpView>,
}

fn view(p: &crate::syn::machine::rehearse::Proposal) -> ProposalView {
    use crate::syn::machine::sanitize::shown;
    ProposalView {
        id: p.id.clone(),
        door_name: shown(&p.door_name),
        root: shown(&p.root),
        made: p.made,
        unchanged: p.unchanged,
        read_outside: p.read_outside,
        ops: p
            .ops
            .iter()
            .map(|o| OpView {
                n: o.n,
                what: o.what,
                from: o.from.as_deref().map(shown),
                to: o.to.as_deref().map(shown),
                size: o.size,
                class: o.class,
                why: o.why,
            })
            .collect(),
    }
}

/// The proposals waiting in one conversation.
#[tauri::command]
pub async fn syn_waiting_rehearsals(app: AppHandle, vault_path: String, conversation_id: String) -> Result<Vec<ProposalView>, String> {
    let rooms = rooms(&app)?;
    Ok(crate::syn::machine::rehearse::waiting(&rooms, &vault_path)
        .iter()
        .filter(|p| p.conversation.as_deref() == Some(conversation_id.as_str()))
        .map(view)
        .collect())
}

/// The system's own dialog, for files that are not plainly documents. A page
/// can press the card's button; it cannot press this one (A22). In the app's two
/// languages, from here: the words are not the page's to choose.
async fn confirm_unknown(app: &AppHandle, locale: &str, names: &[String]) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let listed: String = names.iter().take(12).map(|n| format!("• {n}\n")).collect();
    let more = names.len().saturating_sub(12);
    let (title, body, ok, cancel) = if locale.starts_with("vi") {
        (
            "Áp dụng cả những file không rõ là tài liệu?",
            format!(
                "Những mục này không rõ là tài liệu: script, cài đặt, chương trình, hoặc file có nội dung không khớp với tên. Áp dụng thì chúng nằm trong thư mục của bạn đúng như Syn đã làm, sau khi nó đọc các file trong thư mục.\n\n{listed}{}\nChỉ chọn Áp dụng nếu bạn biết rõ từng mục là gì.",
                if more > 0 { format!("… và {more} mục nữa\n") } else { String::new() }
            ),
            "Áp dụng",
            "Huỷ",
        )
    } else {
        (
            "Also apply files that are not plainly documents?",
            format!(
                "These are not plainly documents: scripts, settings, programs, or files whose content does not match their name. Applied, they are in your folder as Syn made them, after it read the folder's files.\n\n{listed}{}\nChoose Apply only if you know what each one is.",
                if more > 0 { format!("… and {more} more\n") } else { String::new() }
            ),
            "Apply",
            "Cancel",
        )
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .message(body)
        .title(title)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(ok.into(), cancel.into()));
    if let Some(window) = crate::app_shell::app_window(app) {
        dialog = dialog.parent(&window);
    }
    dialog.show(move |yes| {
        let _ = tx.send(yes);
    });
    rx.await.unwrap_or(false)
}

#[derive(serde::Serialize)]
pub struct Applied {
    /// False when the system's dialog was answered no: nothing was done.
    pub confirmed: bool,
    pub outcome: Option<crate::syn::machine::apply::Outcome>,
}

/// Applies what the person chose on the card. Only a waiting proposal of this
/// vault, only the ops it holds, and the unknown ones only once the system's
/// dialog says so.
#[tauri::command]
pub async fn syn_apply_rehearsal(app: AppHandle, vault_path: String, id: String, approved: Vec<usize>, locale: String) -> Result<Applied, String> {
    use crate::syn::machine::rehearse::{self, Class, State};
    let rooms = rooms(&app)?;
    let mut proposal = rehearse::load_proposal(&rooms, &id).ok_or("there is no such proposal")?;
    if proposal.state != State::Waiting || proposal.vault != vault_path {
        return Err("that proposal is not waiting".into());
    }
    let unknown: Vec<String> = proposal
        .ops
        .iter()
        .filter(|o| approved.contains(&o.n) && o.class == Class::Unknown)
        .map(|o| crate::syn::machine::sanitize::shown(o.to.as_deref().or(o.from.as_deref()).unwrap_or_default()))
        .collect();
    if !unknown.is_empty() && !confirm_unknown(&app, &locale, &unknown).await {
        return Ok(Applied { confirmed: false, outcome: None });
    }
    // Nothing in the room runs while the folder is changed.
    let cell = rooms.open(&proposal.room).await.ok();
    if let Some(cell) = cell.clone() {
        let r = rooms.clone();
        let _ = tokio::task::spawn_blocking(move || r.clear(&cell)).await;
    }
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    let (r, p) = (rooms.clone(), proposal.clone());
    let outcome = tokio::task::spawn_blocking(move || {
        crate::syn::machine::apply::apply(r.root(), &p, |n| rehearse::staged_file(&r, &p.id, n), &approved, &crate::syn::machine::apply::Trash::System { home })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    rehearse::settle(&rooms, cell.as_ref(), &mut proposal, State::Applied).map_err(|e| e.to_string())?;
    Ok(Applied { confirmed: true, outcome: Some(outcome) })
}

/// The person said no to the whole proposal.
#[tauri::command]
pub async fn syn_decline_rehearsal(app: AppHandle, vault_path: String, id: String) -> Result<bool, String> {
    use crate::syn::machine::rehearse::{self, State};
    let rooms = rooms(&app)?;
    let Some(mut proposal) = rehearse::load_proposal(&rooms, &id) else { return Ok(false) };
    if proposal.state != State::Waiting || proposal.vault != vault_path {
        return Ok(false);
    }
    let cell = rooms.open(&proposal.room).await.ok();
    rehearse::settle(&rooms, cell.as_ref(), &mut proposal, State::Declined).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Every batch applied on this Mac, newest first, with whether it can be undone.
#[tauri::command]
pub async fn syn_rehearsal_batches(app: AppHandle) -> Result<Vec<crate::syn::machine::apply::Batch>, String> {
    Ok(crate::syn::machine::apply::batches(rooms(&app)?.root()))
}

/// Puts a batch back, finished or cut short.
#[tauri::command]
pub async fn syn_undo_rehearsal(app: AppHandle, id: String) -> Result<crate::syn::machine::apply::Undone, String> {
    let rooms = rooms(&app)?;
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || crate::syn::machine::apply::undo(rooms.root(), &id, &crate::syn::machine::apply::Trash::System { home }))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
