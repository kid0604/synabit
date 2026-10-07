//! A Rich Table, out of the app: an Excel workbook of one view, saved where
//! the person chose. CSV needs no help from here; a workbook does, because
//! the xlsx writer is Rust (`syn::spreadsheet::write_xlsx`, desktop only).

use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};

/// The most characters Excel holds in one cell. The writer refuses a longer
/// string, and with it the whole workbook.
const MAX_CELL_CHARS: usize = 32_767;

#[derive(Deserialize)]
pub struct ExportSheet {
    pub name: String,
    /// The header row first. Numbers and booleans arrive as such, so the
    /// workbook's cells can be summed the moment it is opened.
    pub rows: Vec<Vec<Value>>,
}

/// Write a workbook to the destination a save dialog returned.
///
/// Written to the cache first and copied over through the same opener the
/// vault export uses, so a `content://` URI from Android's picker and the
/// dialog's grant of the chosen path are treated alike. A cell beginning
/// with `=` is written as text, not a formula: see `write_xlsx`.
#[tauri::command]
pub async fn export_table_xlsx(
    app_handle: tauri::AppHandle,
    destination: String,
    sheets: Vec<ExportSheet>,
) -> AppResult<()> {
    if !crate::syn::spreadsheet::WORKBOOKS {
        return Err(AppError::General("Workbooks can only be written in the desktop app.".into()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let dir = app_handle
            .path()
            .app_cache_dir()
            .map_err(|e| AppError::General(format!("could not locate the cache directory: {e}")))?;
        std::fs::create_dir_all(&dir)?;
        let staged = dir.join(format!("table-{}.xlsx", uuid::Uuid::new_v4()));

        let outcome = (|| {
            crate::syn::spreadsheet::write_xlsx(&staged, &workbook_of(sheets)).map_err(AppError::General)?;
            let mut from = std::fs::File::open(&staged)?;
            let mut to = crate::commands::vault::open_chosen_for_write(&app_handle, &destination)?;
            std::io::copy(&mut from, &mut to)?;
            Ok::<_, AppError>(())
        })();
        let _ = std::fs::remove_file(&staged);
        outcome
    })
    .await
    .map_err(|e| AppError::General(format!("the export did not finish: {e}")))?
}

/// The sheets as Excel will take them: every name one it accepts, and every
/// cell short enough to fit.
///
/// A view's name is whatever the person typed, and Excel is strict about a
/// sheet's — thirty-one characters, none of `[]:*?/\`, no `'` at either end,
/// not blank, not used twice, not "History" (which English Excel keeps for
/// itself). Any one of those used to fail the export outright. A cell past
/// Excel's 32,767 characters is cut there rather than failing it either: a
/// long note in a cell is worth its first 32,767 characters more than nothing.
fn workbook_of(sheets: Vec<ExportSheet>) -> Vec<(String, Vec<Vec<Value>>)> {
    let mut taken: Vec<String> = vec!["History".into()];
    let mut out = Vec::with_capacity(sheets.len());
    for (i, sheet) in sheets.into_iter().enumerate() {
        // Cut first and trimmed after, so a cut that lands on an apostrophe
        // does not leave one at the end.
        let mut raw: String = sheet.name.chars().take(31).collect();
        loop {
            let trimmed = raw.trim().trim_matches('\'');
            if trimmed.len() == raw.len() {
                break;
            }
            raw = trimmed.to_string();
        }
        let name = crate::syn::spreadsheet::sheet_name(&raw, i, &taken);
        taken.push(name.clone());
        let rows = sheet.rows.into_iter().map(|row| row.into_iter().map(fit_cell).collect()).collect();
        out.push((name, rows));
    }
    out
}

/// A cell as Excel can hold it: text no longer than it allows. A list or an
/// object is written as its JSON, so it is cut the same way.
fn fit_cell(value: Value) -> Value {
    let text = match value {
        Value::String(s) => s,
        Value::Array(_) | Value::Object(_) => value.to_string(),
        other => return other,
    };
    if text.chars().count() > MAX_CELL_CHARS {
        Value::String(text.chars().take(MAX_CELL_CHARS).collect())
    } else {
        Value::String(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(raw: &[&str]) -> Vec<String> {
        let sheets = raw.iter().map(|n| ExportSheet { name: n.to_string(), rows: vec![] }).collect();
        workbook_of(sheets).into_iter().map(|(name, _)| name).collect()
    }

    #[test]
    fn every_sheet_name_is_one_excel_accepts() {
        let thirty = "a".repeat(30);
        let got = names(&["Chi tiêu [10/2026]: *?", "", "  ", "'quoted'", &format!("{thirty}'tail"), "history", "Tháng 10", "tháng 10"]);
        assert_eq!(got[0], "Chi tiêu _10_2026__ __");
        assert_eq!(got[1], "Sheet2");
        assert_eq!(got[2], "Sheet3");
        assert_eq!(got[3], "quoted");
        assert_eq!(got[4], thirty, "a cut that lands on an apostrophe drops it");
        assert_eq!(got[5], "history (2)");
        assert_eq!(got[7], "tháng 10 (2)");
        for name in &got {
            assert!(!name.is_empty() && name.chars().count() <= 31, "{name}");
            assert!(!name.contains(['[', ']', ':', '*', '?', '/', '\\']), "{name}");
            assert!(!name.starts_with('\'') && !name.ends_with('\''), "{name}");
        }
    }

    #[test]
    fn a_cell_longer_than_excel_holds_is_cut_not_fatal() {
        let long = "ư".repeat(MAX_CELL_CHARS + 10);
        let sheets = vec![ExportSheet { name: "S".into(), rows: vec![vec![json!("A")], vec![json!(long)], vec![json!(5)], vec![json!(true)]] }];
        let book = workbook_of(sheets);
        let rows = &book[0].1;
        assert_eq!(rows[1][0].as_str().unwrap().chars().count(), MAX_CELL_CHARS);
        assert_eq!((rows[2][0].clone(), rows[3][0].clone()), (json!(5), json!(true)));

        #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
        {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("long.xlsx");
            crate::syn::spreadsheet::write_xlsx(&path, &book).expect("the workbook is written");
        }
    }
}
