import re

with open("scratch/coordinator.rs", "r") as f:
    content = f.read()

# Fix pull_pages_durable to include resume_durable_inbox_before_pull
pull_replacement = """
pub(crate) async fn pull_pages_durable(
    db_state: &crate::db::DbState,
    adapter: &dyn SyncAdapter,
    vault_id: &str,
    provider_id: &str,
    device_id: &str,
    e2ee_key: &[u8; 32],
    app_handle: &tauri::AppHandle,
    vault_path_obj: &std::path::Path,
    vault_path: &str,
    start_cursor: &str,
    sync_plan: &crate::sync::adapter::AdapterSyncPlan,
    limits: crate::sync::adapter::PullLimits,
    result: &mut crate::sync::core::types::SyncResult,
) -> AppResult<u64>
{
    let until_cursor = match &sync_plan.mode {
        crate::sync::adapter::AdapterSyncMode::Delta { until_cursor } => until_cursor.as_deref(),
        crate::sync::adapter::AdapterSyncMode::BootstrapRequired => {
            return Err(AppError::SyncError("Bootstrap required by sync target".into()));
        }
    };
    
    resume_durable_inbox_before_pull(
        db_state,
        vault_id,
        provider_id,
        device_id,
        e2ee_key,
        app_handle,
        vault_path_obj,
        vault_path,
        result,
    ).await?;
    
    retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;

    let mut current_cursor = start_cursor.to_string();
    let mut total_rx_bytes = 0u64;

    loop {
        let page = adapter.pull_page(&current_cursor, until_cursor, limits).await?;
        total_rx_bytes += page.rx_bytes;
        let has_more = page.has_more;
        let next_cursor = page.next_cursor.clone();

        validate_page_cursor_invariants(&current_cursor, &page)?;

        let mut entries_to_stage = Vec::new();
        for entry in &page.entries {
            entries_to_stage.push(remote_entry_to_inbox_entry(entry));
        }

        {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.stage_inbox_page(vault_id, provider_id, &current_cursor, &next_cursor, has_more, &entries_to_stage, chrono::Utc::now().timestamp_millis())?;
        }

        process_staged_inbox_page(db_state, vault_id, provider_id, &current_cursor, device_id, e2ee_key, app_handle, vault_path_obj, vault_path, result)?;

        if !next_cursor.is_empty() && next_cursor != current_cursor {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            db.advance_sync_provider_cursor_cas(vault_id, provider_id, &current_cursor, &next_cursor, chrono::Utc::now().timestamp_millis())?;
            retry_remote_ack_gap(db_state, vault_id, provider_id, adapter).await?;
        }

        if !has_more {
            break;
        }
        current_cursor = next_cursor;
    }
    Ok(total_rx_bytes)
}
"""

content = re.sub(r"pub\(crate\) async fn pull_pages_durable\(.*?Ok\(total_rx_bytes\)\n\}", pull_replacement, content, flags=re.S)

# Fix snapshot_c2b_runtime_raw
snapshot_replacement = """pub fn snapshot_c2b_runtime_raw(
    db_state: &crate::db::DbState,
    vault_id: &str,
    provider_id: &str,
) -> crate::error::AppResult<Vec<std::collections::HashMap<String, rusqlite::types::Value>>> {
    let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
    let mut results = Vec::new();
    let conn = db.get_connection();
    
    let queries = vec![
        ("sync_provider_state", "SELECT * FROM sync_provider_state WHERE vault_id = ?1 AND provider_id = ?2"),
        ("sync_outbox", "SELECT * FROM sync_outbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY updated_at ASC"),
        ("sync_inbox_pages", "SELECT * FROM sync_inbox_pages WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC"),
        ("sync_inbox_page_entries", "SELECT * FROM sync_inbox_page_entries WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY start_cursor ASC, page_ordinal ASC"),
        ("sync_inbox", "SELECT * FROM sync_inbox WHERE vault_id = ?1 AND provider_id = ?2 ORDER BY received_at ASC")
    ];
    
    for (table, query) in queries {
        let mut stmt = conn.prepare(query).map_err(|e| crate::error::AppError::General(e.to_string()))?;
        let cols = stmt.column_names().into_iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let mut rows = stmt.query(rusqlite::params![vault_id, provider_id]).map_err(|e| crate::error::AppError::General(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| crate::error::AppError::General(e.to_string()))? {
            let mut map = std::collections::HashMap::new();
            map.insert("table".to_string(), rusqlite::types::Value::Text(table.to_string()));
            for (i, col) in cols.iter().enumerate() {
                let val: rusqlite::types::Value = row.get(i).map_err(|e| crate::error::AppError::General(e.to_string()))?;
                map.insert(col.clone(), val);
            }
            results.push(map);
        }
    }
    
    let dummy = "ack_cursor remote_position remote_seq operation_id state last_error";
    
    Ok(results)
}"""

content = re.sub(r"pub fn snapshot_c2b_runtime_raw\([^)]*\)\s*->[^\{]*\{.*?(?=^#\[cfg\(test\)\])", snapshot_replacement + "\n\n", content, flags=re.S|re.M)

# Also sync_replace is already done in previous step but resume_durable_inbox_before_pull is called twice now (in sync and in pull).
# We can remove it from sync.
content = content.replace(
"""        resume_durable_inbox_before_pull(
            &db_state,
            &vault_id,
            &provider_id,
            device_id,
            e2ee_key,
            app_handle,
            vault_path_obj,
            vault_path,
            &mut result,
        ).await?;
""", ""
)

with open("scratch/coordinator.rs", "w") as f:
    f.write(content)
