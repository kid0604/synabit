//! End-to-end sync scenarios.
//!
//! Each test drives two real devices through the real coordinator and asserts
//! on files that actually exist on disk. A scenario marked `#[ignore]` is a
//! *known defect*, not a flaky test: the assertion states the behaviour we
//! want, and the ignore reason names the bug. Run them with
//! `cargo test --lib scenarios -- --ignored` to see the current gap.

use crate::sync::harness::{vault_with_devices, HarnessDevice, InMemoryMailbox};
use synabit_protocol::SyncEntryKind;

const NOTE: &str = "Notes/plan.md";

// ---------------------------------------------------------------------------
// Baseline: does anything work at all?
// ---------------------------------------------------------------------------

#[tokio::test]
async fn new_file_reaches_the_other_device() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nFirst draft.\n");
    a.sync_ok().await;

    assert!(
        !b.exists(NOTE),
        "B must not have the file before it syncs — otherwise the harness is \
         sharing a filesystem and proves nothing"
    );

    b.sync_ok().await;

    assert!(b.exists(NOTE), "B should have received the file");
    assert!(
        b.body(NOTE).unwrap().contains("First draft."),
        "B got the file but not its content: {:?}",
        b.body(NOTE)
    );
}

#[tokio::test]
async fn edit_propagates_back_to_the_origin_device() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nFirst draft.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    let received = b.read(NOTE).expect("B has the note");
    b.write(NOTE, &received.replace("First draft.", "Second draft."));
    b.sync_ok().await;
    a.sync_ok().await;

    let a_body = a.body(NOTE).expect("A still has the note");
    assert!(
        a_body.contains("Second draft."),
        "A should have B's edit, got: {:?}",
        a_body
    );
}

#[tokio::test]
async fn own_operations_are_not_reapplied() {
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let a = &devices[0];

    a.write(NOTE, "# Plan\n\nOnly A.\n");
    a.sync_ok().await;
    let after_first = mailbox.len();

    // Syncing again with no local change must not push a duplicate.
    let result = a.sync_ok().await;

    assert_eq!(
        mailbox.len(),
        after_first,
        "a no-op sync pushed extra entries"
    );
    assert_eq!(result.pulled, 0, "A re-applied its own operation");
    let _ = &devices[1];
}

// ---------------------------------------------------------------------------
// Deletes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_propagates_to_the_other_device() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nTo be deleted.\n");
    a.sync_ok().await;
    b.sync_ok().await;
    assert!(b.exists(NOTE), "precondition: B has the file");

    a.delete(NOTE);
    a.sync_ok().await;
    b.sync_ok().await;

    assert!(
        !b.exists(NOTE),
        "B should have removed the file after A deleted it"
    );
}

#[tokio::test]
async fn a_delete_loses_to_an_unpublished_local_edit() {
    // Deleting is the only operation that destroys user data on a remote
    // instruction, so a file that has moved on since its last successful sync
    // is kept rather than removed.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nShared draft.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    // B edits locally but never syncs; meanwhile A deletes and publishes.
    let received = b.read(NOTE).expect("B has the note");
    b.write(NOTE, &received.replace("Shared draft.", "B was still working here."));

    a.delete(NOTE);
    a.sync_ok().await;
    b.sync_ok().await;

    assert!(
        b.exists(NOTE),
        "B's unpublished edit was destroyed by A's tombstone"
    );
    assert!(
        b.body(NOTE).unwrap().contains("B was still working here."),
        "B kept the file but lost the edit: {:?}",
        b.body(NOTE)
    );
}

#[tokio::test]
async fn emptying_a_vault_on_purpose_succeeds_on_the_second_try() {
    // The guard must delay a real "delete everything", not block it forever.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/one.md", "# One\n");
    a.write("Notes/two.md", "# Two\n");
    a.sync_ok().await;
    b.sync_ok().await;

    std::fs::remove_dir_all(a.vault_path().join("Notes")).expect("empty the vault");

    let first = a.sync().await;
    assert!(first.is_err(), "the first attempt should stop and explain");

    // Syncing again is the confirmation.
    a.sync_ok().await;
    assert_eq!(
        mailbox.kinds().iter().filter(|k| **k == SyncEntryKind::Delete).count(),
        2,
        "confirming should publish both tombstones"
    );

    b.sync_ok().await;
    assert!(!b.exists("Notes/one.md") && !b.exists("Notes/two.md"));
}

#[tokio::test]
async fn offline_revisions_are_published_once() {
    // Editing the same note repeatedly while the server is unreachable must not
    // queue one full snapshot per edit. Each payload already contains the whole
    // document, so only the newest one is worth sending.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nRevision 0.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    mailbox.reject_pushes();
    let baseline = mailbox.len();
    for revision in 1..=5 {
        // Edit the body the way an editor would, leaving the frontmatter alone.
        let current = a.read(NOTE).unwrap();
        let edited = current.replace(
            &format!("Revision {}.", revision - 1),
            &format!("Revision {revision}."),
        );
        a.write(NOTE, &edited);
        let _ = a.sync().await; // fails to push, by design
    }
    assert_eq!(mailbox.len(), baseline, "nothing should have been accepted yet");

    // Coming back online does not by itself re-arm a backed-off entry; the next
    // edit does, which is also what a returning user does.
    mailbox.accept_pushes();
    let current = a.read(NOTE).unwrap();
    a.write(NOTE, &current.replace("Revision 5.", "Revision 6."));
    a.sync_ok().await;

    assert_eq!(
        mailbox.len() - baseline,
        1,
        "six offline edits produced {} entries instead of one",
        mailbox.len() - baseline
    );

    b.sync_ok().await;
    assert!(
        b.body(NOTE).unwrap().contains("Revision 6."),
        "the peer did not receive the newest revision: {:?}",
        b.body(NOTE)
    );
}

#[tokio::test]
async fn one_unpublishable_file_does_not_block_the_rest() {
    // A vault holds documents the identity assigner cannot handle. Refusing one
    // of them used to abort the whole run, so a single awkward file meant
    // nothing in the vault synced at all.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/good.md", "# Good\n\nThis must still travel.\n");
    // An object root whose `metadata` is a scalar: there is nowhere to put an
    // id, and overwriting the field would destroy what the user put there.
    a.write("Data/awkward.json", r#"{"metadata": "not-an-object"}"#);
    // A JSON array is ordinary content, not an error — the message logs are
    // exactly this shape — and must sync on its path identity.
    a.write("Messages/day.json", r#"[{"id":"m1"},{"id":"m2"}]"#);

    let result = a.sync_ok().await;

    assert_eq!(
        result.errors.len(),
        1,
        "the skipped file should be reported once: {:?}",
        result.errors
    );
    assert!(
        result.errors[0].contains("awkward.json"),
        "the report should name the file: {:?}",
        result.errors
    );

    b.sync_ok().await;
    assert!(
        b.exists("Notes/good.md"),
        "a healthy note was held up by an unrelated file"
    );
    assert!(
        b.exists("Messages/day.json"),
        "a JSON array is ordinary content and must sync"
    );
    assert!(!b.exists("Data/awkward.json"));
}

#[tokio::test]
async fn copied_files_that_share_an_identity_each_get_their_own() {
    // Duplicating a note or a whiteboard copies its metadata, so several files
    // end up claiming to be the same document. They are separate files, so they
    // are separate documents: sharing one identity made them overwrite each
    // other's queued work, so neither ever settled and sync never went idle.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    let shared = r#"{"metadata":{"node_id":"shared-identity"},"title":"board"}"#;
    a.write("Boards/one.json", shared);
    a.write("Boards/two.json", shared);

    a.sync_ok().await;

    // Settled: a second pass with no user edits must publish nothing further.
    let after_first = _mailbox.len();
    let second = a.sync_ok().await;
    assert_eq!(
        _mailbox.len(),
        after_first,
        "sync did not settle — it published again with nothing changed"
    );
    assert_eq!(second.pushed, 0, "a quiet vault should push nothing");

    b.sync_ok().await;
    assert!(b.exists("Boards/one.json"), "one.json never arrived");
    assert!(b.exists("Boards/two.json"), "two.json never arrived");
}

#[tokio::test]
async fn an_attachment_does_not_make_every_sync_report_errors() {
    // Before attachments could be carried, each one was read as UTF-8, rejected,
    // and — because a file that never publishes never records a baseline —
    // re-read and re-reported on every run for the life of the vault.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/note.md", "# Note\n");
    std::fs::write(a.vault_path().join("photo.jpg"), png_bytes(7)).unwrap();

    let first = a.sync_ok().await;
    assert!(first.errors.is_empty(), "unexpected errors: {:?}", first.errors);

    // And it settles: nothing is re-detected once published.
    let second = a.sync_ok().await;
    assert_eq!(second.pushed, 0, "sync did not settle");
    assert!(second.errors.is_empty());

    b.sync_ok().await;
    assert!(b.exists("Notes/note.md"));
    assert!(
        b.vault_path().join("photo.jpg").exists(),
        "the attachment should now travel too"
    );
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

/// A small binary that is definitely not valid UTF-8.
fn png_bytes(seed: u8) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.extend((0..4096u32).map(|i| (i as u8) ^ seed));
    v
}

#[tokio::test]
async fn an_attachment_reaches_the_other_device_intact() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    let picture = png_bytes(0x5A);
    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/photo.png"), &picture).unwrap();
    a.write("Notes/note.md", "# Note\n\n![](assets/photo.png)\n");

    a.sync_ok().await;
    b.sync_ok().await;

    let received = std::fs::read(b.vault_path().join("assets/photo.png"))
        .expect("the attachment never arrived");
    assert_eq!(received, picture, "the attachment arrived corrupted");
    assert!(b.exists("Notes/note.md"), "the note referencing it should arrive too");
}

#[tokio::test]
async fn a_large_attachment_survives_being_chunked() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    // Larger than one chunk, and not a round multiple of it.
    let big: Vec<u8> = (0..(crate::sync::core::asset::CHUNK_BYTES * 2 + 12_345))
        .map(|i| (i % 251) as u8)
        .collect();
    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/clip.mp4"), &big).unwrap();

    a.sync_ok().await;
    b.sync_ok().await;

    let received = std::fs::read(b.vault_path().join("assets/clip.mp4")).expect("never arrived");
    assert_eq!(received.len(), big.len(), "wrong length after reassembly");
    assert_eq!(received, big, "content differs after reassembly");
}

#[tokio::test]
async fn an_unchanged_attachment_is_not_uploaded_twice() {
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let a = &devices[0];

    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/photo.png"), png_bytes(1)).unwrap();

    a.sync_ok().await;
    let after_first = mailbox.chunk_bytes();
    assert!(after_first > 0, "nothing was uploaded");

    let second = a.sync_ok().await;
    assert_eq!(second.pushed, 0, "an unchanged attachment was published again");
    assert_eq!(
        mailbox.chunk_bytes(),
        after_first,
        "an unchanged attachment was uploaded again"
    );
    let _ = &devices[1];
}

#[tokio::test]
async fn deleting_an_attachment_removes_it_from_the_other_device() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/photo.png"), png_bytes(2)).unwrap();
    a.sync_ok().await;
    b.sync_ok().await;
    assert!(b.exists("assets/photo.png"), "precondition: B has it");

    std::fs::remove_file(a.vault_path().join("assets/photo.png")).unwrap();
    a.sync_ok().await;
    b.sync_ok().await;

    assert!(!b.exists("assets/photo.png"), "the deletion did not travel");
}

#[tokio::test]
async fn a_target_that_cannot_store_attachments_leaves_them_quietly_alone() {
    // Google Drive has nowhere to put chunks. Offering it an attachment anyway
    // queues an entry that can never be published and reports the same failure
    // on every sync for the life of the vault.
    let mailbox = InMemoryMailbox::new();
    let a = HarnessDevice::without_asset_support("a", &mailbox);

    a.write("Notes/note.md", "# Note\n");
    std::fs::write(a.vault_path().join("photo.jpg"), png_bytes(3)).unwrap();

    let first = a.sync_ok().await;
    assert_eq!(first.pushed, 1, "the note should still be published");
    assert!(
        first.errors.is_empty(),
        "an attachment this target cannot carry is not a failure: {:?}",
        first.errors
    );

    let second = a.sync_ok().await;
    assert_eq!(second.pushed, 0, "sync did not settle");
    assert!(second.errors.is_empty(), "the same complaint came back: {:?}", second.errors);
    assert_eq!(mailbox.chunk_count(), 0, "no chunks should have been attempted");
}

#[tokio::test]
async fn an_attachment_too_large_to_carry_is_left_local_without_complaint() {
    // Preparing an attachment holds the file and all of its encrypted chunks in
    // memory at once. Without a ceiling, dropping a video into the vault decides
    // how much memory the app needs, and on a phone that is decided by being
    // killed. The file stays local — where it already was — and the refusal is
    // not dressed up as a sync failure, because it will be just as large on
    // every future run.
    let mailbox = InMemoryMailbox::new();
    let a = HarnessDevice::new("a", &mailbox);

    a.write("Notes/note.md", "# Note\n");

    // Sparse, so the test costs a size rather than the bytes behind it.
    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    let huge = a.vault_path().join("assets/video.mp4");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(crate::sync::core::asset::MAX_ASSET_BYTES + 1)
        .unwrap();

    let first = a.sync_ok().await;
    assert_eq!(first.pushed, 1, "the note should still be published");
    assert!(
        first.errors.is_empty(),
        "a file too large to carry is not a sync failure: {:?}",
        first.errors
    );
    assert_eq!(mailbox.chunk_count(), 0, "no chunk should have been uploaded");

    // The point of the whole exercise: it does not come back every run.
    let second = a.sync_ok().await;
    assert!(
        second.errors.is_empty(),
        "the same complaint returned on the next sync: {:?}",
        second.errors
    );

    assert!(huge.exists(), "the file should still be on disk, untouched");
}

#[tokio::test]
async fn an_attachment_whose_bytes_never_arrive_stops_being_asked_for() {
    // A reference whose chunks are missing used to be retried on every sync for
    // ever, pushing the same error into the result each time. Retrying is right
    // — the bytes may be moments behind — but only a bounded number of times.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/photo.png"), png_bytes(9)).unwrap();
    a.sync_ok().await;
    assert!(mailbox.chunk_count() > 0, "the chunk should have been uploaded");

    // The reference survives; the bytes do not.
    mailbox.drop_all_chunks();

    let mut last = b.sync_ok().await;
    for _ in 0..crate::sync::coordinator::MAX_INBOX_APPLY_ATTEMPTS {
        last = b.sync_ok().await;
    }

    assert!(
        !b.vault_path().join("assets/photo.png").exists(),
        "a file must never be written from chunks that were never fetched"
    );
    assert!(
        last.errors.is_empty(),
        "after giving up, the attempt should stop being reported every run: {:?}",
        last.errors
    );
}

#[tokio::test]
async fn a_published_attachment_tells_the_server_which_chunks_it_needs() {
    // The server stores chunks but cannot read the payload naming them, so
    // without this declaration it has no way to tell a chunk still in use from
    // one left behind — which is why its collector could only delete by age,
    // and why deleting by age destroyed live attachments.
    //
    // The declaration has to actually leave the device. A client that sent an
    // empty list would look correct from every angle except the one that
    // matters: chunks would simply never be collected, silently, for ever.
    let mailbox = InMemoryMailbox::new();
    let a = HarnessDevice::new("a", &mailbox);

    a.write("Notes/note.md", "# Note\n");
    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    std::fs::write(a.vault_path().join("assets/photo.png"), png_bytes(4)).unwrap();
    a.sync_ok().await;

    let declared = mailbox.declared_chunk_ids();
    assert!(
        !declared.is_empty(),
        "the attachment was published without saying what it depends on"
    );
    assert_eq!(
        declared.len(),
        mailbox.chunk_count(),
        "every uploaded chunk should be accounted for by a reference"
    );
}

#[test]
fn an_attachment_reference_cannot_ask_for_more_memory_than_it_could_hold() {
    // Every number in an `AssetRef` is asserted by whichever device sent it, and
    // both the size and the chunk count are used to size allocations. A peer
    // with the vault key is trusted with the contents, not with this machine's
    // memory.
    use crate::sync::core::asset::{validate_incoming, MAX_ASSET_BYTES, MAX_ASSET_CHUNKS};
    use synabit_protocol::{AssetChunkRef, AssetRef};

    let chunk = AssetChunkRef {
        chunk_id: [0u8; 32],
        chunk_hash: [0u8; 32],
        compressed_len: 16,
    };
    let reference = |total_bytes: u64, count: usize| AssetRef {
        asset_id: [0u8; 32],
        rel_path: "assets/claim.bin".into(),
        node_id: "assets/claim.bin".into(),
        mime_type: "application/octet-stream".into(),
        total_bytes,
        plaintext_hash: [0u8; 32],
        chunks: vec![chunk.clone(); count],
    };

    assert!(
        validate_incoming(&reference(MAX_ASSET_BYTES + 1, MAX_ASSET_CHUNKS)).is_err(),
        "a reference over the size limit should be refused"
    );
    assert!(
        validate_incoming(&reference(1024, MAX_ASSET_CHUNKS + 1)).is_err(),
        "a reference claiming more chunks than can exist should be refused"
    );
    assert!(
        validate_incoming(&reference(u64::MAX, 1)).is_err(),
        "a huge byte count beside one chunk is the shape of a memory claim"
    );
    assert!(
        validate_incoming(&reference(1024, 1)).is_ok(),
        "an ordinary reference should still pass"
    );
}

// ---------------------------------------------------------------------------
// Joining a vault that already has history
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_device_joining_after_compaction_gets_the_whole_vault() {
    // The server collects history once every device has acknowledged it. A
    // device that joins afterwards replays from zero, so collection must never
    // remove the last remaining record of a document.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/one.md", "# One\n\nOriginal.\n");
    a.write("Notes/two.md", "# Two\n\nUntouched since.\n");
    a.write("Notes/three.md", "# Three\n");
    a.sync_ok().await;
    b.sync_ok().await;

    // One document is revised, so its earlier entry becomes superseded.
    a.write("Notes/one.md", &a.read("Notes/one.md").unwrap().replace("Original.", "Revised."));
    a.sync_ok().await;
    b.sync_ok().await;

    let dropped = mailbox.compact_superseded();
    assert!(dropped > 0, "nothing was compacted, so the test proves nothing");

    // A device that has never connected joins now.
    let c = HarnessDevice::new("c", &mailbox);
    c.sync_ok().await;

    for note in ["Notes/one.md", "Notes/two.md", "Notes/three.md"] {
        assert!(c.exists(note), "the new device never received {note}");
    }
    assert!(
        c.body("Notes/one.md").unwrap().contains("Revised."),
        "the new device got a stale revision: {:?}",
        c.body("Notes/one.md")
    );
}

// ---------------------------------------------------------------------------
// Known defects — each of these should pass once the named bug is fixed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn devices_sharing_a_device_id_still_exchange_changes() {
    // Reproduces the GDrive path, which derives device_id from the app bundle
    // identifier — the same value on every install.
    let mailbox = InMemoryMailbox::new();
    let a = HarnessDevice::new_with_device_id("a", &mailbox, "net.synabit.app");
    let b = HarnessDevice::new_with_device_id("b", &mailbox, "net.synabit.app");

    a.write(NOTE, "# Plan\n\nFrom A.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    assert!(
        b.exists(NOTE),
        "B ignored A's operation because both report the same device_id"
    );
}

#[tokio::test]
async fn rename_arrives_as_a_rename_not_a_deletion() {
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(NOTE, "# Plan\n\nSurvives a rename.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    a.rename(NOTE, "Notes/roadmap.md");
    a.sync_ok().await;
    b.sync_ok().await;

    assert!(
        b.exists("Notes/roadmap.md"),
        "B should have the renamed file; mailbox emitted {:?}",
        mailbox.kinds()
    );
    assert!(
        b.body("Notes/roadmap.md")
            .unwrap()
            .contains("Survives a rename."),
        "renamed file lost its content"
    );
    assert!(
        !mailbox.kinds().contains(&SyncEntryKind::Delete),
        "a rename should not emit a tombstone"
    );
}

#[tokio::test]
async fn a_corrupt_entry_does_not_block_healthy_ones() {
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/first.md", "# First\n\nBefore the bad entry.\n");
    a.sync_ok().await;
    let bad_seq = mailbox.head_seq();

    a.write("Notes/second.md", "# Second\n\nAfter the bad entry.\n");
    a.sync_ok().await;

    // Corrupt the first entry only. The second must still get through.
    mailbox.corrupt_entry_at(bad_seq);

    let _ = b.sync().await; // expected to report an error
    assert!(
        b.exists("Notes/second.md"),
        "a single corrupt entry stopped every healthy entry behind it"
    );
}

#[tokio::test]
async fn an_unreachable_vault_does_not_emit_tombstones() {
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/one.md", "# One\n");
    a.write("Notes/two.md", "# Two\n");
    a.sync_ok().await;
    b.sync_ok().await;

    // Simulate an unmounted drive: the vault root is emptied but the database
    // still knows about both documents.
    std::fs::remove_dir_all(a.vault_path().join("Notes")).expect("empty the vault");

    let before = mailbox.len();
    let _ = a.sync().await;

    assert_eq!(
        mailbox.len(),
        before,
        "A pushed {} tombstone(s) for a vault that merely went missing",
        mailbox.len() - before
    );
}
