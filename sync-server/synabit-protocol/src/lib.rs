//! Wire protocol types for the Synabit Mailbox protocol.
//!
//! All messages are serialized with `postcard` (compact binary) and framed with
//! a 4-byte big-endian length prefix over QUIC bidirectional streams.

use serde::{Deserialize, Serialize};

/// ALPN identifier for the Synabit Mailbox protocol.
pub const MAILBOX_ALPN: &[u8] = b"synabit/mailbox/1";

/// Maximum size of a single framed message (128 MiB).
pub const MAX_MESSAGE_SIZE: u32 = 128 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Capabilities and Server Metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Capability {
    PagedPull,
    BootstrapV1,
    AssetChunksV1,
    DurableIdempotency,
    DeviceLifecycleV1,
    QuotaV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerHello {
    pub protocol_version: u16,
    pub server_incarnation: [u8; 16],
    pub capabilities: Vec<Capability>,
    pub max_message_bytes: u64,
    pub max_page_bytes: u64,
    pub max_asset_chunk_bytes: u64,
}

// ---------------------------------------------------------------------------
// Trash metadata (synced soft-delete)
// ---------------------------------------------------------------------------

/// Metadata for a trashed document, synced across devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashMetaEntry {
    pub doc_hash: [u8; 32],
    pub original_path_encrypted: Vec<u8>,
    pub deleted_at: u64,
    pub deleted_by_device: String,
    pub is_purged: bool,
}

// ---------------------------------------------------------------------------
// Request types (client → server)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncEntryKind {
    Upsert,
    Delete,
    AssetReference,
}

impl std::fmt::Display for SyncEntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncEntryKind::Upsert => write!(f, "upsert"),
            SyncEntryKind::Delete => write!(f, "delete"),
            SyncEntryKind::AssetReference => write!(f, "asset_reference"),
        }
    }
}

impl std::str::FromStr for SyncEntryKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "upsert" => Ok(SyncEntryKind::Upsert),
            "delete" => Ok(SyncEntryKind::Delete),
            "asset_reference" => Ok(SyncEntryKind::AssetReference),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetChunkRef {
    pub chunk_id: [u8; 32],
    pub chunk_hash: [u8; 32],
    pub compressed_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssetRef {
    pub asset_id: [u8; 32],
    pub mime_type: String,
    pub total_bytes: u64,
    pub plaintext_hash: [u8; 32],
    pub chunks: Vec<AssetChunkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletePayload {
    pub node_id: String,
    pub rel_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncPayload {
    Upsert(Vec<u8>),
    Delete(DeletePayload),
    AssetReference(AssetRef),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushBatchItem {
    pub operation_id: [u8; 16],
    pub doc_hash: [u8; 32],
    pub entry_kind: SyncEntryKind,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultItem {
    pub operation_id: [u8; 16],
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailboxRequest {
    /// Initial handshake to negotiate protocol version.
    Hello { version: u32 },
    /// Authenticate this connection for a specific vault.
    Auth {
        vault_hash: [u8; 32],
        mailbox_token: [u8; 32],
        device_id: String,
    },
    /// Push a new encrypted CRDT document snapshot.
    Push {
        operation_id: [u8; 16],
        entry_kind: SyncEntryKind,
        doc_hash: [u8; 32],
        encrypted_payload: Vec<u8>,
        payload_hash: [u8; 32],
    },
    /// Batch push multiple documents
    PushBatch { items: Vec<PushBatchItem> },
    /// Pull all mailbox entries with `seq > since_seq`.
    Pull { since_seq: u64 },
    /// Acknowledge that this device has processed entries up to `up_to_seq`.
    Ack { up_to_seq: u64 },
    /// Push an encrypted binary asset (image, attachment, etc.).
    PushAsset {
        asset_hash: [u8; 32],
        encrypted_data: Vec<u8>,
    },
    /// Pull a previously stored asset by its hash.
    PullAsset { asset_hash: [u8; 32] },
    /// Check if an asset chunk exists.
    HasAsset { chunk_id: [u8; 32] },
    /// Mark a document as deleted (tombstone).
    PushDelete {
        operation_id: [u8; 16],
        doc_hash: [u8; 32],
    },
    /// Push encrypted trash metadata entries for synced soft-delete.
    PushTrashMeta { entries: Vec<TrashMetaEntry> },
    /// Pull all trash metadata for the vault.
    PullTrashMeta,
    /// Notify server that a document has been restored from trash.
    PushRestore { doc_hash: [u8; 32] },
    /// Revoke a device — server deletes its cursor so it can no longer ACK.
    RevokeDevice { device_id: String },
    /// Rotate the vault's mailbox token after an epoch increment.
    RotateToken { new_mailbox_token: Vec<u8> },
    /// Application-level keepalive ping.
    Ping,

    // ---------------------------------------------------------------------------
    // Mailbox V3
    // ---------------------------------------------------------------------------
    /// Authenticate this connection for a specific vault using V3 protocol.
    AuthV3 {
        version: u32,
        capabilities: u64,
        vault_hash: [u8; 32],
        mailbox_token: [u8; 32],
        device_id: String,
    },
    /// Request a sync plan to determine if bootstrap or delta sync is needed.
    GetSyncPlan {
        client_incarnation_id: Option<[u8; 16]>,
        cursor: u64,
    },
    /// Pull a delta page of mailbox entries.
    PullPage {
        after_seq: u64,
        until_seq: u64,
        max_entries: u16,
        max_bytes: u32,
    },
    /// Start a bootstrap session for a new or stale device.
    BeginBootstrap {
        page_max_entries: u16,
        page_max_bytes: u32,
    },
    /// Pull a page of bootstrap items.
    PullBootstrapPage {
        session_id: [u8; 16],
        after_position: u64,
        max_entries: u16,
        max_bytes: u32,
    },
    /// Keep an active bootstrap session alive.
    KeepBootstrapAlive { session_id: [u8; 16] },
    /// Complete a bootstrap session after all items are applied locally.
    CompleteBootstrap { session_id: [u8; 16] },
    /// Begin a checkpoint seed (authoritative snapshot from a healthy device).
    BeginCheckpointSeed { start_seq: u64 },
    /// Add an inventory page to a checkpoint seed.
    AddCheckpointInventoryPage {
        seed_id: [u8; 16],
        doc_hashes: Vec<[u8; 32]>,
    },
    /// Complete a checkpoint seed.
    CompleteCheckpointSeed { seed_id: [u8; 16] },
    /// Abort a checkpoint seed.
    AbortCheckpointSeed { seed_id: [u8; 16] },
}

// ---------------------------------------------------------------------------
// Response types (server → client)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailboxResponse {
    /// Server push notification: new data is available for this vault.
    NotifyNewData { trigger_seq: u64 },
    /// Handshake successful. Returns capabilities.
    HelloOk(ServerHello),
    /// Protocol version not supported.
    UpgradeRequired {
        supported_versions: Vec<u16>,
        message: String,
    },
    /// Authentication succeeded.
    AuthOk,
    /// Authentication failed.
    AuthFailed { reason: String },
    /// Document push accepted; returns the assigned sequence number.
    PushOk { seq: u64 },
    /// Document batch push accepted.
    PushBatchOk {
        max_seq: u64,
        results: Vec<BatchResultItem>,
    },
    /// Result of a Pull request — a batch of entries since the requested cursor.
    PullResult { entries: Vec<MailboxEntry> },
    /// ACK recorded successfully.
    AckOk,
    /// Asset stored successfully.
    AssetOk,
    /// Requested asset data.
    AssetData { data: Vec<u8> },
    /// Requested asset was not found.
    AssetNotFound,
    /// Asset chunk exists.
    AssetExists { encrypted_size: u64 },
    /// Delete tombstone recorded; returns the assigned sequence number.
    DeleteOk { seq: u64 },
    /// Result of a PullTrashMeta request — all trash metadata for the vault.
    TrashMetaResult { entries: Vec<TrashMetaEntry> },
    /// Confirmation that a document was restored from trash.
    RestoreOk { seq: u64 },
    /// Generic error.
    Error { message: String },
    /// Storage quota exceeded.
    QuotaExceeded {
        current_bytes: u64,
        limit_bytes: u64,
    },
    /// Device revocation recorded.
    RevokeOk,
    /// Mailbox token rotated successfully.
    TokenRotated,
    /// Application-level keepalive pong.
    Pong,

    // ---------------------------------------------------------------------------
    // Mailbox V3
    // ---------------------------------------------------------------------------
    /// Response to GetSyncPlan.
    SyncPlan(SyncPlan),
    /// Result of a PullPage request.
    PullPageResult(PullPageResult),
    /// Result of BeginBootstrap.
    BootstrapStarted(BootstrapStarted),
    /// Result of PullBootstrapPage.
    BootstrapPage(BootstrapPage),
    /// Acknowledgement of KeepBootstrapAlive or CompleteBootstrap.
    BootstrapOk,
    /// Response when bootstrap is currently unavailable (e.g. vault is seeding).
    BootstrapUnavailable,
    /// Response for BeginCheckpointSeed.
    CheckpointSeedStarted { seed_id: [u8; 16] },
    /// Response for AddCheckpointInventoryPage.
    CheckpointInventoryPageOk,
    /// Response for CompleteCheckpointSeed or AbortCheckpointSeed.
    CheckpointSeedOk,
}

// ---------------------------------------------------------------------------
// Mailbox entry (returned inside PullResult)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MailboxEntry {
    pub seq: u64,
    pub doc_hash: [u8; 32],
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
    pub operation_id: [u8; 16],
    pub entry_kind: SyncEntryKind,
}

// ---------------------------------------------------------------------------
// Mailbox V3 structs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SyncMode {
    Delta { until_seq: u64 },
    BootstrapRequired,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SyncPlan {
    pub incarnation_id: [u8; 16],
    pub head_seq: u64,
    pub compacted_through_seq: u64,
    pub mode: SyncMode,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MailboxEntryV3 {
    pub seq: u64,
    pub operation_id: [u8; 16],
    pub entry_kind: SyncEntryKind,
    pub doc_hash: [u8; 32],
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PullPageResult {
    pub entries: Vec<MailboxEntryV3>,
    pub next_seq: u64,
    pub until_seq: u64,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapStarted {
    pub session_id: [u8; 16],
    pub incarnation_id: [u8; 16],
    pub base_seq: u64,
    pub item_count: u64,
    pub total_bytes: u64,
    pub expires_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapItem {
    pub position: u64,
    pub doc_hash: [u8; 32],
    pub head_seq: u64,
    pub operation_id: [u8; 16],
    pub entry_kind: SyncEntryKind,
    pub source_device: String,
    pub encrypted_payload: Vec<u8>,
    pub payload_hash: [u8; 32],
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BootstrapPage {
    pub items: Vec<BootstrapItem>,
    pub next_position: u64,
    pub has_more: bool,
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_entry_kind_str_roundtrip() {
        let kinds = vec![
            SyncEntryKind::Upsert,
            SyncEntryKind::Delete,
            SyncEntryKind::AssetReference,
        ];
        for k in kinds {
            let s = k.to_string();
            let parsed: SyncEntryKind = s.parse().expect("parse kind");
            assert_eq!(k, parsed);
        }
    }

    #[test]
    fn test_postcard_roundtrip_push_batch() {
        let item = PushBatchItem {
            operation_id: [1; 16],
            doc_hash: [2; 32],
            entry_kind: SyncEntryKind::AssetReference,
            encrypted_payload: vec![1, 2, 3, 4],
            payload_hash: [3; 32],
        };
        let req = MailboxRequest::PushBatch { items: vec![item] };
        let serialized = postcard::to_stdvec(&req).expect("serialize req");
        let deserialized: MailboxRequest =
            postcard::from_bytes(&serialized).expect("deserialize req");
        if let MailboxRequest::PushBatch { items } = deserialized {
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].entry_kind, SyncEntryKind::AssetReference);
            assert_eq!(items[0].operation_id, [1; 16]);
        } else {
            panic!("Expected PushBatch request");
        }
    }

    #[test]
    fn test_postcard_roundtrip_pull_page_result() {
        let entry = MailboxEntryV3 {
            seq: 42,
            operation_id: [5; 16],
            entry_kind: SyncEntryKind::Delete,
            doc_hash: [6; 32],
            source_device: "dev_test".into(),
            encrypted_payload: vec![],
            payload_hash: [0; 32],
            timestamp: 1000,
        };
        let res = PullPageResult {
            entries: vec![entry],
            next_seq: 43,
            until_seq: 100,
            has_more: true,
        };
        let resp = MailboxResponse::PullPageResult(res);
        let serialized = postcard::to_stdvec(&resp).expect("serialize resp");
        let deserialized: MailboxResponse =
            postcard::from_bytes(&serialized).expect("deserialize resp");
        if let MailboxResponse::PullPageResult(page) = deserialized {
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.entries[0].entry_kind, SyncEntryKind::Delete);
            assert_eq!(page.next_seq, 43);
            assert!(page.has_more);
        } else {
            panic!("Expected PullPageResult response");
        }
    }

    #[test]
    fn test_postcard_roundtrip_sync_payload_all_variants() {
        // 1. Upsert
        let upsert = SyncPayload::Upsert(vec![1, 2, 3, 4, 5]);
        let ser_upsert = postcard::to_stdvec(&upsert).unwrap();
        let de_upsert: SyncPayload = postcard::from_bytes(&ser_upsert).unwrap();
        assert_eq!(de_upsert, upsert);

        // 2. Delete
        let delete = SyncPayload::Delete(DeletePayload {
            node_id: "node_1".into(),
            rel_path: "path/to/doc.md".into(),
        });
        let ser_delete = postcard::to_stdvec(&delete).unwrap();
        let de_delete: SyncPayload = postcard::from_bytes(&ser_delete).unwrap();
        assert_eq!(de_delete, delete);

        // 3. AssetReference
        let asset_ref = AssetRef {
            asset_id: [7; 32],
            mime_type: "image/png".into(),
            total_bytes: 1024,
            plaintext_hash: [8; 32],
            chunks: vec![AssetChunkRef {
                chunk_id: [9; 32],
                chunk_hash: [10; 32],
                compressed_len: 512,
            }],
        };
        let asset_payload = SyncPayload::AssetReference(asset_ref.clone());
        let ser_asset = postcard::to_stdvec(&asset_payload).unwrap();
        let de_asset: SyncPayload = postcard::from_bytes(&ser_asset).unwrap();
        assert_eq!(de_asset, asset_payload);
        if let SyncPayload::AssetReference(de_ref) = de_asset {
            assert_eq!(de_ref.asset_id, [7; 32]);
            assert_eq!(de_ref.mime_type, "image/png");
            assert_eq!(de_ref.chunks.len(), 1);
            assert_eq!(de_ref.chunks[0].compressed_len, 512);
        } else {
            panic!("Expected AssetReference variant");
        }
    }
}
