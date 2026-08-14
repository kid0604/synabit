use crate::db::DbState;
use crate::error::AppResult;
use crate::sync::adapter::SyncAdapter;
use crate::sync::core::types::VaultSyncContext;
use crate::sync::core::types::{AssetChunkRef, AssetRef};
use log::{info, warn};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub async fn process_pending_assets(
    app_handle: &tauri::AppHandle,
    vault_ctx: &VaultSyncContext,
    adapter: &Arc<dyn SyncAdapter>,
    vault_e2ee_key: &[u8; 32],
) -> AppResult<()> {
    let db_state = app_handle.state::<DbState>();
    let provider_id = adapter.adapter_id();
    let vault_id = vault_ctx.vault_id.to_string();

    // Lấy danh sách pending assets (giới hạn 50 cái mỗi lượt để không block quá lâu)
    let pending_assets = {
        let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
        db.get_pending_assets(&vault_id, &provider_id, 50)?
    };

    if pending_assets.is_empty() {
        return Ok(());
    }

    info!(
        "Processing {} pending assets for vault {}",
        pending_assets.len(),
        vault_id
    );

    let tmp_dir = vault_ctx.vault_root.join(".synabit").join("tmp");
    if !tmp_dir.exists() {
        let _ = std::fs::create_dir_all(&tmp_dir);
    }

    for asset_row in pending_assets {
        {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.update_pending_asset_status(
                &vault_id,
                &provider_id,
                asset_row.remote_seq,
                &asset_row.asset_id,
                "downloading",
                None,
            );
        }

        let asset_ref: AssetRef = match postcard::from_bytes(&asset_row.asset_ref_blob) {
            Ok(ar) => ar,
            Err(e) => {
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                let _ = db.update_pending_asset_status(
                    &vault_id,
                    &provider_id,
                    asset_row.remote_seq,
                    &asset_row.asset_id,
                    "failed",
                    Some(&format!("Invalid asset ref: {}", e)),
                );
                continue;
            }
        };

        // TODO: Download logic
        // 1. Download chunks to tmp_dir
        // 2. Decrypt & assemble
        // 3. Verify hash & size
        // 4. Atomic rename to target rel_path

        let mut download_success = true;
        let mut err_msg = None;
        let tmp_file_path = tmp_dir.join(format!("{}.tmp", hex::encode(asset_ref.asset_id)));

        // Mở file tmp để ghi
        let mut tmp_file = match std::fs::File::create(&tmp_file_path) {
            Ok(f) => f,
            Err(e) => {
                download_success = false;
                err_msg = Some(format!("Cannot create tmp file: {}", e));
                // Handle err below
                let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
                let _ = db.update_pending_asset_status(
                    &vault_id,
                    &provider_id,
                    asset_row.remote_seq,
                    &asset_row.asset_id,
                    "failed",
                    err_msg.as_deref(),
                );
                continue;
            }
        };

        use chacha20poly1305::{
            aead::{Aead, KeyInit},
            XChaCha20Poly1305, XNonce,
        };
        use std::io::Write;

        for chunk in &asset_ref.chunks {
            match adapter.pull_asset_chunk(chunk.chunk_id).await {
                Ok(Some(encrypted_data)) => {
                    if encrypted_data.len() < 24 {
                        download_success = false;
                        err_msg = Some("Chunk data too short (no nonce)".to_string());
                        break;
                    }
                    let asset_key = crate::sync::core::asset::derive_asset_key(
                        vault_e2ee_key,
                        &asset_ref.asset_id,
                    );
                    let cipher = XChaCha20Poly1305::new((&asset_key).into());
                    let nonce = XNonce::from_slice(&encrypted_data[0..24]);
                    match cipher.decrypt(nonce, &encrypted_data[24..]) {
                        Ok(decrypted) => {
                            let chunk_hash = *blake3::hash(&decrypted).as_bytes();
                            if chunk_hash != chunk.plaintext_hash {
                                download_success = false;
                                err_msg = Some("Chunk hash mismatch".to_string());
                                break;
                            }
                            if let Err(e) = tmp_file.write_all(&decrypted) {
                                download_success = false;
                                err_msg = Some(format!("Failed to write chunk: {}", e));
                                break;
                            }
                        }
                        Err(e) => {
                            download_success = false;
                            err_msg = Some(format!("Failed to decrypt chunk: {}", e));
                            break;
                        }
                    }
                }
                Ok(None) => {
                    download_success = false;
                    err_msg = Some("Chunk not found on server".to_string());
                    break;
                }
                Err(e) => {
                    download_success = false;
                    err_msg = Some(format!("Network error pulling chunk: {}", e));
                    break;
                }
            }
        }

        if download_success {
            // Verify final hash
            if let Ok((computed_hash, computed_size)) =
                crate::sync::core::asset::hash_file_streaming(&tmp_file_path)
            {
                if computed_hash.as_bytes() == &asset_ref.plaintext_hash
                    && computed_size == asset_ref.plaintext_size
                {
                    // Di chuyển file vào đúng vị trí
                    let target_path = vault_ctx.vault_root.join(&asset_row.rel_path);
                    if let Some(parent) = target_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::rename(&tmp_file_path, &target_path) {
                        err_msg = Some(format!("Failed to rename asset: {}", e));
                        download_success = false;
                    }
                } else {
                    err_msg = Some("Final asset hash/size mismatch".to_string());
                    download_success = false;
                }
            } else {
                err_msg = Some("Failed to hash tmp file".to_string());
                download_success = false;
            }
        }

        if !download_success {
            let _ = std::fs::remove_file(&tmp_file_path);
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.update_pending_asset_status(
                &vault_id,
                &provider_id,
                asset_row.remote_seq,
                &asset_row.asset_id,
                "failed",
                err_msg.as_deref(),
            );
        } else {
            let db = db_state.lock().unwrap_or_else(|e| e.into_inner());
            let _ = db.update_pending_asset_status(
                &vault_id,
                &provider_id,
                asset_row.remote_seq,
                &asset_row.asset_id,
                "applied",
                None,
            );
        }
    }

    Ok(())
}

pub async fn push_asset_chunks_for_ops(
    vault_ctx: &VaultSyncContext,
    adapter: &Arc<dyn SyncAdapter>,
    vault_e2ee_key: &[u8; 32],
    ops: &[crate::sync::core::types::SyncOperation],
) -> AppResult<()> {
    use crate::sync::core::types::SyncPayload;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};

    for op in ops {
        let decrypted_payload =
            match crate::sync::core::crypto::decrypt(vault_e2ee_key, &op.encrypted_payload) {
                Ok(j) => j,
                Err(e) => {
                    warn!("Failed to decrypt payload for asset check: {}", e);
                    continue;
                }
            };
        let payload: crate::sync::core::types::SyncPayload =
            match postcard::from_bytes(&decrypted_payload) {
                Ok(p) => p,
                Err(e) => {
                    warn!("Failed to parse payload for asset check: {}", e);
                    continue;
                }
            };

        if let SyncPayload::V2(asset_payload) = payload {
            let asset_ref = &asset_payload.asset;
            let file_path = vault_ctx.vault_root.join(&op.rel_path);

            if !file_path.exists() {
                warn!("File {} does not exist for chunk push", op.rel_path);
                continue;
            }

            let mut file = match File::open(&file_path) {
                Ok(f) => f,
                Err(e) => {
                    warn!("Failed to open {} for chunk push: {}", op.rel_path, e);
                    continue;
                }
            };

            for chunk in &asset_ref.chunks {
                // Dùng HasAsset trước khi push chunk. Nếu AssetExists, skip.
                match adapter.has_asset_chunk(chunk.chunk_id).await {
                    Ok(true) => {
                        info!(
                            "Chunk {} already exists on server, skipping",
                            hex::encode(chunk.chunk_id)
                        );
                        continue;
                    }
                    Ok(false) => {} // Needs push
                    Err(e) => {
                        return Err(crate::error::AppError::SyncError(format!(
                            "has_asset_chunk failed: {}",
                            e
                        )));
                    }
                }

                // Push chunk
                let mut buffer = vec![0u8; chunk.plaintext_size as usize];
                let offset = (chunk.index as u64) * (asset_ref.chunk_size as u64);
                if let Err(e) = file.seek(SeekFrom::Start(offset)) {
                    return Err(crate::error::AppError::General(format!(
                        "Failed to seek file: {}",
                        e
                    )));
                }
                if let Err(e) = file.read_exact(&mut buffer) {
                    return Err(crate::error::AppError::General(format!(
                        "Failed to read chunk from file: {}",
                        e
                    )));
                }

                let encrypted_chunk = crate::sync::core::asset::encrypt_chunk(
                    &buffer,
                    asset_ref,
                    chunk,
                    vault_e2ee_key,
                )?;
                adapter
                    .push_asset_chunk(chunk.chunk_id, encrypted_chunk)
                    .await?;
                info!("Pushed chunk {} successfully", hex::encode(chunk.chunk_id));
            }
        }
    }

    Ok(())
}
