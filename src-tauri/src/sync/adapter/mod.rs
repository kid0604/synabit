use crate::error::AppResult;
use crate::sync::core::types::SyncOperation;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod gdrive;
pub mod server;

/// Core adapter trait — mọi sync target phải implement
#[async_trait]
pub trait SyncAdapter: Send + Sync {
    /// Tên hiển thị
    fn name(&self) -> &str;

    /// ID duy nhất cho adapter instance (dùng cho cursor tracking)
    fn adapter_id(&self) -> String;

    /// Kiểm tra kết nối
    async fn is_connected(&self) -> bool;

    /// Kết nối / authenticate
    async fn connect(&self) -> AppResult<()>;

    /// Ngắt kết nối
    async fn disconnect(&self) -> AppResult<()>;

    /// Push các operations lên remote
    async fn push(&self, operations: Vec<SyncOperation>) -> AppResult<PushResult>;

    /// Lấy sync plan cho cursor hiện tại
    async fn get_sync_plan(
        &self,
        cursor: &str,
        client_incarnation_id: Option<[u8; 16]>,
    ) -> AppResult<AdapterSyncPlan>;

    /// Pull một page operations từ remote kể từ cursor
    async fn pull_page(
        &self,
        cursor: &str,
        until_cursor: Option<&str>,
        limits: PullLimits,
    ) -> AppResult<AdapterPullPage>;

    /// Acknowledge đã xử lý đến cursor
    async fn ack(&self, cursor: &str) -> AppResult<()>;

    /// Push một asset (binary file)
    async fn push_asset(&self, hash: [u8; 32], data: Vec<u8>) -> AppResult<()>;

    /// Pull một asset
    async fn pull_asset(&self, hash: [u8; 32]) -> AppResult<Option<Vec<u8>>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushAck {
    pub operation_id: [u8; 16],
    pub remote_position: String,
    pub remote_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResult {
    pub accepted: Vec<PushAck>,
    pub rejected: Vec<PushAck>,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterSyncMode {
    Delta { until_cursor: Option<String> },
    BootstrapRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterSyncPlan {
    pub mode: AdapterSyncMode,
    pub incarnation_id: Option<[u8; 16]>,
    pub remote_vault_id: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy)]
pub struct PullLimits {
    pub max_entries: u16,
    pub max_bytes: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdapterPullPage {
    pub entries: Vec<RemoteEntry>,
    pub next_cursor: String,
    pub has_more: bool,
    pub rx_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub remote_position: String,
    pub remote_seq: Option<u64>,
    pub doc_hash: [u8; 32],
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
    pub operation_id: [u8; 16],
    pub entry_kind: synabit_protocol::SyncEntryKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    struct DummyOpaqueAdapter;

    #[async_trait]
    impl SyncAdapter for DummyOpaqueAdapter {
        fn name(&self) -> &str {
            "Dummy Adapter"
        }
        fn adapter_id(&self) -> String {
            "dummy_id".to_string()
        }
        async fn is_connected(&self) -> bool {
            true
        }
        async fn connect(&self) -> AppResult<()> {
            Ok(())
        }
        async fn disconnect(&self) -> AppResult<()> {
            Ok(())
        }
        async fn push(&self, _operations: Vec<SyncOperation>) -> AppResult<PushResult> {
            Ok(PushResult {
                accepted: vec![],
                rejected: vec![],
                tx_bytes: 0,
            })
        }
        async fn get_sync_plan(
            &self,
            cursor: &str,
            _client_incarnation_id: Option<[u8; 16]>,
        ) -> AppResult<AdapterSyncPlan> {
            assert_eq!(cursor, "opaque_token_abc");
            Ok(AdapterSyncPlan {
                mode: AdapterSyncMode::Delta {
                    until_cursor: Some("opaque_until_xyz".into()),
                },
                incarnation_id: None,
                remote_vault_id: None,
            })
        }
        async fn pull_page(
            &self,
            cursor: &str,
            until_cursor: Option<&str>,
            _limits: PullLimits,
        ) -> AppResult<AdapterPullPage> {
            assert_eq!(cursor, "opaque_token_abc");
            assert_eq!(until_cursor, Some("opaque_until_xyz"));
            Ok(AdapterPullPage {
                entries: vec![],
                next_cursor: "opaque_token_def".into(),
                has_more: false,
                rx_bytes: 0,
            })
        }
        async fn ack(&self, _cursor: &str) -> AppResult<()> {
            Ok(())
        }
        async fn push_asset(&self, _hash: [u8; 32], _data: Vec<u8>) -> AppResult<()> {
            Err(AppError::UnsupportedCapability(
                "push_asset unsupported".into(),
            ))
        }
        async fn pull_asset(&self, _hash: [u8; 32]) -> AppResult<Option<Vec<u8>>> {
            Err(AppError::UnsupportedCapability(
                "pull_asset unsupported".into(),
            ))
        }
    }

    #[tokio::test]
    async fn test_coordinator_facing_contract_opaque_cursor() {
        let adapter = DummyOpaqueAdapter;
        let plan = adapter
            .get_sync_plan("opaque_token_abc", None)
            .await
            .unwrap();
        let until = match plan.mode {
            AdapterSyncMode::Delta { until_cursor } => until_cursor,
            _ => panic!("Expected Delta mode"),
        };
        let page = adapter
            .pull_page(
                "opaque_token_abc",
                until.as_deref(),
                PullLimits {
                    max_entries: 10,
                    max_bytes: 100,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.next_cursor, "opaque_token_def");
    }

    #[test]
    fn test_provider_ids_are_stable() {
        let dummy = DummyOpaqueAdapter;
        assert_eq!(dummy.adapter_id(), "dummy_id");

        let gdrive = gdrive::GoogleDriveAdapter::for_testing_dummy();
        assert_eq!(gdrive.adapter_id(), "gdrive");
    }

    #[tokio::test]
    async fn test_unsupported_capability_error_variant() {
        let adapter = DummyOpaqueAdapter;
        let err = adapter.push_asset([0; 32], vec![]).await.unwrap_err();
        match err {
            AppError::UnsupportedCapability(msg) => {
                assert!(msg.contains("push_asset unsupported"));
            }
            _ => panic!("Expected UnsupportedCapability error variant"),
        }
    }
}
