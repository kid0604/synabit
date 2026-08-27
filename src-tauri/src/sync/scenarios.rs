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

// ---------------------------------------------------------------------------
// Three and four devices
//
// Everything above this line converges at n=2. Several properties the design
// depends on are invisible at that size: a head that must survive for a device
// which has not arrived yet, an acknowledgement floor held down by the slowest
// participant, and a merge with more than two sides to it.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_note_written_once_reaches_both_other_devices() {
    // The plainest thing three devices can do, and it had never been run.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(NOTE, "# Plan\n\nWritten once.\n");
    a.sync_ok().await;

    b.sync_ok().await;
    c.sync_ok().await;

    for (name, dev) in [("B", b), ("C", c)] {
        assert!(dev.exists(NOTE), "{name} never received the note");
        assert!(
            dev.body(NOTE).unwrap().contains("Written once."),
            "{name} received the note without its content: {:?}",
            dev.body(NOTE)
        );
    }
}

#[tokio::test]
async fn three_devices_editing_the_same_note_all_end_up_with_the_same_text() {
    // The merge property itself. Two devices exercise one merge; three exercise
    // merging a merge, which is where a CRDT either earns its keep or does not.
    //
    // The assertion is convergence, not a particular winner: all three must
    // agree, and every edit must survive somewhere in the result.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(NOTE, "# Plan\n\nshared base\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    // Each device appends its own line without seeing the others.
    for (dev, line) in [(a, "from A"), (b, "from B"), (c, "from C")] {
        let current = dev.read(NOTE).expect("every device has the note");
        dev.write(NOTE, &format!("{current}{line}\n"));
    }

    // Publish all three, then let everyone catch up twice: the second pass is
    // what carries each device's view of the merge to the others.
    for dev in [a, b, c] {
        dev.sync_ok().await;
    }
    for _ in 0..2 {
        for dev in [a, b, c] {
            dev.sync_ok().await;
        }
    }

    let a_body = a.body(NOTE).expect("A has the note");
    let b_body = b.body(NOTE).expect("B has the note");
    let c_body = c.body(NOTE).expect("C has the note");

    assert_eq!(a_body, b_body, "A and B disagree after converging");
    assert_eq!(b_body, c_body, "B and C disagree after converging");

    for line in ["from A", "from B", "from C"] {
        assert!(
            a_body.contains(line),
            "the merge lost '{line}':\n{a_body}"
        );
    }
}

#[tokio::test]
async fn a_fourth_device_joining_a_compacted_vault_receives_everything() {
    // The reason entry collection preserves heads. Three devices work for a
    // while, the server collects what they have all acknowledged, and only then
    // does a fourth device appear — with no history of its own, replaying from
    // sequence zero.
    //
    // Getting this wrong is silent: the newcomer receives a vault that is merely
    // incomplete, with no error anywhere to say so.
    let (mailbox, devices) = vault_with_devices(&["a", "b", "c", "d"]);
    let (a, b, c, d) = (&devices[0], &devices[1], &devices[2], &devices[3]);

    a.write("Notes/one.md", "# One\n\nfirst version\n");
    a.write("Notes/two.md", "# Two\n\nnever touched again\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    // B revises one of them, so that document has history to collect.
    let current = b.read("Notes/one.md").expect("B has it");
    b.write("Notes/one.md", &current.replace("first version", "second version"));
    b.sync_ok().await;
    a.sync_ok().await;
    c.sync_ok().await;

    let removed = mailbox.compact_acked_by_all();
    assert!(removed > 0, "nothing was collected, so the test proves nothing");

    // Now D arrives.
    d.sync_ok().await;

    assert!(d.exists("Notes/one.md"), "D never received the revised note");
    assert!(d.exists("Notes/two.md"), "D never received the untouched note");
    assert!(
        d.body("Notes/one.md").unwrap().contains("second version"),
        "D received a stale version: {:?}",
        d.body("Notes/one.md")
    );
    assert!(
        d.body("Notes/two.md").unwrap().contains("never touched again"),
        "D received the second note without its content: {:?}",
        d.body("Notes/two.md")
    );
}

#[tokio::test]
async fn the_slowest_device_holds_the_floor_under_collection() {
    // Collection waits for everyone. A device that has synced once and then gone
    // quiet keeps the entries it has not seen alive, and this is the property
    // that makes collection safe rather than merely tidy.
    let (mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write("Notes/one.md", "# One\n\noriginal\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    // C stops here. A and B carry on revising.
    for revision in ["second", "third"] {
        let current = a.read("Notes/one.md").expect("A has it");
        let previous = if revision == "second" { "original" } else { "second" };
        a.write("Notes/one.md", &current.replace(previous, revision));
        a.sync_ok().await;
        b.sync_ok().await;
    }

    let floor_before = mailbox.ack_floor();
    mailbox.compact_acked_by_all();

    // C now returns and must still be able to rebuild the document.
    c.sync_ok().await;
    assert!(
        c.body("Notes/one.md").unwrap().contains("third"),
        "the quiet device came back to a stale or missing note: {:?}",
        c.body("Notes/one.md")
    );
    assert!(
        mailbox.ack_floor() > floor_before,
        "the floor should rise once the quiet device catches up"
    );
}

#[tokio::test]
async fn an_attachment_reaches_a_device_that_was_not_there_when_it_was_published() {
    // Attachment bytes live beside the entries rather than inside them, so a
    // device arriving later has to resolve a reference against chunks published
    // before it existed. This is the case the chunk collector can break.
    let (mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    let picture = png_bytes(11);
    std::fs::write(a.vault_path().join("assets/photo.png"), &picture).unwrap();
    a.write("Notes/note.md", "# Note\n\n![](assets/photo.png)\n");
    a.sync_ok().await;
    b.sync_ok().await;

    // Only now does C appear, after the chunks were uploaded and acknowledged
    // by everyone who was present.
    mailbox.compact_acked_by_all();
    c.sync_ok().await;

    let received = std::fs::read(c.vault_path().join("assets/photo.png"))
        .expect("the late device never received the attachment");
    assert_eq!(received, picture, "the late device received it corrupted");
}

#[tokio::test]
async fn two_devices_editing_different_notes_both_arrive_at_the_third() {
    // Concurrent work on separate documents must not interfere. A single shared
    // mailbox sequence carries both, and the third device applies them in one
    // pass.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write("Notes/shared.md", "# Shared\n\nbase\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    a.write("Notes/from_a.md", "# From A\n\nA's work.\n");
    b.write("Notes/from_b.md", "# From B\n\nB's work.\n");
    a.sync_ok().await;
    b.sync_ok().await;

    c.sync_ok().await;

    assert!(c.exists("Notes/from_a.md"), "C missed A's note");
    assert!(c.exists("Notes/from_b.md"), "C missed B's note");
    assert!(
        c.body("Notes/from_b.md").unwrap().contains("B's work."),
        "C got B's note without its content: {:?}",
        c.body("Notes/from_b.md")
    );
}

#[tokio::test]
async fn a_delete_from_one_device_reaches_every_other_device() {
    // A tombstone has to fan out like anything else. With two devices a delete
    // that only half works is indistinguishable from one that works.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(NOTE, "# Plan\n\nto be removed\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;
    assert!(b.exists(NOTE) && c.exists(NOTE), "precondition: all three have it");

    b.delete(NOTE);
    b.sync_ok().await;

    a.sync_ok().await;
    c.sync_ok().await;

    assert!(!a.exists(NOTE), "A kept a note that B deleted");
    assert!(!c.exists(NOTE), "C kept a note that B deleted");
}

#[tokio::test]
async fn a_device_returning_after_a_long_absence_catches_up_in_one_run() {
    // Ten revisions and several new notes accumulate while one device is away.
    // Coming back must not require repeated runs, and must not lose the notes it
    // already had.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write("Notes/journal.md", "# Journal\n\nentry 0\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    // C goes away here.
    for entry in 1..=10 {
        let current = a.read("Notes/journal.md").expect("A has it");
        a.write(
            "Notes/journal.md",
            &current.replace(&format!("entry {}", entry - 1), &format!("entry {entry}")),
        );
        a.write(&format!("Notes/note_{entry}.md"), &format!("# Note {entry}\n"));
        a.sync_ok().await;
        b.sync_ok().await;
    }

    let result = c.sync_ok().await;

    assert!(
        c.body("Notes/journal.md").unwrap().contains("entry 10"),
        "the returning device has a stale journal: {:?}",
        c.body("Notes/journal.md")
    );
    for entry in 1..=10 {
        assert!(
            c.exists(&format!("Notes/note_{entry}.md")),
            "the returning device missed note_{entry}.md after pulling {} file(s)",
            result.pulled
        );
    }
}

#[tokio::test]
async fn two_devices_creating_the_same_path_do_not_destroy_each_other() {
    // Two people make a note with the same title on the same day. Nothing
    // co-ordinated the two paths, so both devices publish a document that claims
    // the same location with different content and a different identity.
    //
    // Losing one silently is the failure mode worth guarding: whatever the
    // resolution, the devices must end up agreeing with each other.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write("Notes/2026-08-16.md", "# Daily\n\nA wrote this first.\n");
    b.write("Notes/2026-08-16.md", "# Daily\n\nB wrote this too.\n");

    a.sync_ok().await;
    b.sync_ok().await;
    for _ in 0..2 {
        for dev in [a, b, c] {
            dev.sync_ok().await;
        }
    }

    let a_body = a.body("Notes/2026-08-16.md").expect("A has a daily note");
    let b_body = b.body("Notes/2026-08-16.md").expect("B has a daily note");
    let c_body = c.body("Notes/2026-08-16.md").expect("C has a daily note");

    assert_eq!(a_body, b_body, "A and B disagree about the shared path");
    assert_eq!(b_body, c_body, "B and C disagree about the shared path");

    // Agreeing is not enough. One of the two notes lost the path, and the writing
    // in it belongs to somebody — it has to still be somewhere, on every device.
    let holds_both = |dev: &HarnessDevice| {
        let mut found = (false, false);
        for entry in walkdir::WalkDir::new(dev.vault_path().join("Notes"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Ok(text) = std::fs::read_to_string(entry.path()) {
                if text.contains("A wrote this first.") {
                    found.0 = true;
                }
                if text.contains("B wrote this too.") {
                    found.1 = true;
                }
            }
        }
        found
    };

    for (name, dev) in [("A", a), ("B", b), ("C", c)] {
        let (has_a, has_b) = holds_both(dev);
        assert!(has_a, "{name} lost what A wrote");
        assert!(has_b, "{name} lost what B wrote");
    }
}

#[tokio::test]
async fn a_delete_that_one_device_refuses_is_not_left_half_applied() {
    // B deletes a note. C has edited the same note without publishing, so C is
    // entitled to keep it — that rule already has a two-device test. What two
    // devices cannot show is what the vault looks like afterwards: A obeyed the
    // tombstone, C did not, and the two now disagree about whether the note
    // exists at all. C's surviving edit has to make its way back.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(NOTE, "# Plan\n\nshared draft\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    let current = c.read(NOTE).expect("C has the note");
    c.write(NOTE, &current.replace("shared draft", "C was still working here"));

    b.delete(NOTE);
    b.sync_ok().await;

    a.sync_ok().await;
    c.sync_ok().await;

    assert!(!a.exists(NOTE), "A should have obeyed the tombstone");
    assert!(c.exists(NOTE), "C's unpublished edit was destroyed");

    for _ in 0..2 {
        for dev in [c, a, b] {
            dev.sync_ok().await;
        }
    }

    assert!(
        a.exists(NOTE),
        "the vault stayed split: C kept the note but never got it back to A"
    );
    assert!(
        a.body(NOTE).unwrap().contains("C was still working here"),
        "A got the note back without C's edit: {:?}",
        a.body(NOTE)
    );
}

#[tokio::test]
async fn two_devices_deleting_the_same_note_at_once_is_not_an_error() {
    // Both delete it, both publish a tombstone for the same document. The second
    // tombstone arrives for a file that is already gone, which must be a no-op
    // rather than a failure reported on every future run.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(NOTE, "# Plan\n\nboth will delete this\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    a.delete(NOTE);
    b.delete(NOTE);
    a.sync_ok().await;
    b.sync_ok().await;

    c.sync_ok().await;
    assert!(!c.exists(NOTE), "C kept a note both other devices deleted");

    let later = c.sync_ok().await;
    assert!(
        later.errors.is_empty(),
        "a duplicate tombstone is still being reported: {:?}",
        later.errors
    );
}

#[tokio::test]
async fn a_rename_reaches_a_device_that_was_offline_when_it_happened() {
    // C holds the file at its old path and never saw the move. Applying the
    // rename means removing one path and creating another on a device whose only
    // record of the document is the old location. Getting it wrong leaves two
    // copies — the vault gains a file nobody created.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write("Notes/old-name.md", "# Note\n\ncontent that moves\n");
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;
    assert!(c.exists("Notes/old-name.md"), "precondition: C has the original");

    a.rename("Notes/old-name.md", "Notes/new-name.md");
    a.sync_ok().await;
    b.sync_ok().await;

    c.sync_ok().await;

    assert!(c.exists("Notes/new-name.md"), "C never received the renamed file");
    assert!(
        !c.exists("Notes/old-name.md"),
        "C ended up with the note at both paths — the rename duplicated it"
    );
    assert!(
        c.body("Notes/new-name.md").unwrap().contains("content that moves"),
        "C received the new path without the content: {:?}",
        c.body("Notes/new-name.md")
    );
}

#[tokio::test]
async fn an_attachment_revised_while_a_device_was_away_arrives_as_the_new_version() {
    // The picture is replaced while C is offline. C must end up with the new
    // bytes, and must not be left holding a reference to chunks that were only
    // ever needed by the version it never saw.
    let (mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    std::fs::create_dir_all(a.vault_path().join("assets")).unwrap();
    let first = png_bytes(21);
    std::fs::write(a.vault_path().join("assets/photo.png"), &first).unwrap();
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    let second = png_bytes(99);
    assert_ne!(first, second, "the fixture must actually change");
    std::fs::write(a.vault_path().join("assets/photo.png"), &second).unwrap();
    a.sync_ok().await;
    b.sync_ok().await;

    mailbox.compact_acked_by_all();
    c.sync_ok().await;

    let received = std::fs::read(c.vault_path().join("assets/photo.png"))
        .expect("C lost the attachment entirely");
    assert_eq!(received, second, "C came back to the old version of the picture");
}

#[tokio::test]
async fn four_devices_working_at_once_all_agree_at_the_end() {
    // The convergence property at the largest size the harness runs. Every
    // device writes its own note and edits a shared one, all before anything is
    // published, so no device has seen any other's work.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c", "d"]);

    devices[0].write("Notes/shared.md", "# Shared\n\nbase\n");
    devices[0].sync_ok().await;
    for dev in &devices {
        dev.sync_ok().await;
    }

    for (i, dev) in devices.iter().enumerate() {
        dev.write(&format!("Notes/own_{i}.md"), &format!("# Own {i}\n"));
        let current = dev.read("Notes/shared.md").expect("everyone has the shared note");
        dev.write("Notes/shared.md", &format!("{current}line from {i}\n"));
    }

    for _ in 0..3 {
        for dev in &devices {
            dev.sync_ok().await;
        }
    }

    let reference = devices[0]
        .body("Notes/shared.md")
        .expect("device 0 has the shared note");

    for (i, dev) in devices.iter().enumerate() {
        assert_eq!(
            dev.body("Notes/shared.md").expect("shared note"),
            reference,
            "device {i} disagrees about the shared note"
        );
        for j in 0..devices.len() {
            assert!(
                dev.exists(&format!("Notes/own_{j}.md")),
                "device {i} never received own_{j}.md"
            );
        }
        assert!(
            reference.contains(&format!("line from {i}")),
            "the merge lost device {i}'s line:\n{reference}"
        );
    }
}

#[tokio::test]
async fn two_devices_adding_a_different_picture_under_the_same_name_still_agree() {
    // Attachments are identified by their path — there is nowhere inside a JPEG
    // to record an identity — so two devices that both save "Pasted image.png"
    // are, as far as the system is concerned, editing one document. Editors
    // generate exactly these names, which makes this the collision most likely
    // to happen in practice.
    //
    // Whatever the outcome, the devices must agree on it. Disagreement is what
    // splits a vault.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    for dev in [a, b] {
        std::fs::create_dir_all(dev.vault_path().join("assets")).unwrap();
    }
    let from_a = png_bytes(31);
    let from_b = png_bytes(77);
    assert_ne!(from_a, from_b, "the two pictures must actually differ");

    std::fs::write(a.vault_path().join("assets/Pasted image.png"), &from_a).unwrap();
    std::fs::write(b.vault_path().join("assets/Pasted image.png"), &from_b).unwrap();

    a.sync_ok().await;
    b.sync_ok().await;
    for _ in 0..2 {
        for dev in [a, b, c] {
            dev.sync_ok().await;
        }
    }

    let read = |dev: &HarnessDevice| {
        std::fs::read(dev.vault_path().join("assets/Pasted image.png"))
            .expect("every device should have a picture at that path")
    };
    let (ra, rb, rc) = (read(a), read(b), read(c));

    assert_eq!(ra, rb, "A and B disagree about the picture");
    assert_eq!(rb, rc, "B and C disagree about the picture");
}

#[tokio::test]
async fn a_picture_replaced_by_another_device_is_kept_rather_than_lost() {
    // Gate 3. Two devices import different files that happen to share a name, so
    // one of them loses the position. Losing the position must not mean losing
    // the file: the loser is set aside under a derived name, and travels to the
    // other devices as an ordinary new attachment.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    for dev in [a, b] {
        std::fs::create_dir_all(dev.vault_path().join("assets")).unwrap();
    }
    let from_a = png_bytes(31);
    let from_b = png_bytes(77);
    std::fs::write(a.vault_path().join("assets/report.png"), &from_a).unwrap();
    std::fs::write(b.vault_path().join("assets/report.png"), &from_b).unwrap();

    a.sync_ok().await;
    b.sync_ok().await;
    for _ in 0..3 {
        for dev in [a, b, c] {
            dev.sync_ok().await;
        }
    }

    // Whoever won, every device agrees about the contested path.
    let at_path = |dev: &HarnessDevice| {
        std::fs::read(dev.vault_path().join("assets/report.png")).expect("a picture at the path")
    };
    assert_eq!(at_path(a), at_path(b), "the devices disagree about the path");
    assert_eq!(at_path(b), at_path(c), "the devices disagree about the path");

    // And both pictures still exist somewhere on every device.
    let holds_both = |dev: &HarnessDevice| {
        let mut found = (false, false);
        for entry in walkdir::WalkDir::new(dev.vault_path().join("assets"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if bytes == from_a {
                    found.0 = true;
                }
                if bytes == from_b {
                    found.1 = true;
                }
            }
        }
        found
    };

    for (name, dev) in [("A", a), ("B", b), ("C", c)] {
        let (has_a, has_b) = holds_both(dev);
        assert!(has_a, "{name} lost the picture that A imported");
        assert!(has_b, "{name} lost the picture that B imported");
    }
}

#[tokio::test]
async fn a_conflict_is_reported_to_the_user_and_not_as_a_failure() {
    // Keeping the file is only half the job. A copy nobody is told about arrives
    // under a name nobody recognises, and gets deleted as clutter — so the sync
    // that preserved it loses it anyway, just more slowly.
    //
    // It must also not be reported as an error: the sync worked. Dressing a
    // conflict as a failure is how people learn to ignore the failures that
    // matter.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    for dev in [a, b] {
        std::fs::create_dir_all(dev.vault_path().join("assets")).unwrap();
    }
    std::fs::write(a.vault_path().join("assets/report.png"), png_bytes(41)).unwrap();
    std::fs::write(b.vault_path().join("assets/report.png"), png_bytes(42)).unwrap();

    a.sync_ok().await;
    b.sync_ok().await;

    let mut reported = Vec::new();
    for _ in 0..3 {
        for dev in [a, b] {
            let result = dev.sync_ok().await;
            assert!(
                result.errors.is_empty(),
                "a conflict was reported as a sync failure: {:?}",
                result.errors
            );
            reported.extend(result.conflicts);
        }
    }

    assert!(
        !reported.is_empty(),
        "the file was kept, but nothing told the user it happened"
    );
    let first = &reported[0];
    assert_eq!(first.rel_path, "assets/report.png");
    assert!(
        first.kept_as.contains("conflict"),
        "the report should name where the file went, got {:?}",
        first.kept_as
    );
    assert!(
        std::fs::metadata(a.vault_path().join(&first.kept_as)).is_ok()
            || std::fs::metadata(b.vault_path().join(&first.kept_as)).is_ok(),
        "the report names a file that is not there: {}",
        first.kept_as
    );
}

#[tokio::test]
async fn importing_the_same_file_on_two_devices_makes_no_conflict_copy() {
    // Gate 2, and the reason gate 3 stays rare. The same file arriving on two
    // machines under one name collides in every mechanical sense, but there is
    // nothing to preserve: the bytes are identical. Making a copy anyway would
    // litter the vault every time someone imported a file they already had.
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    let shared = png_bytes(55);
    for dev in [a, b] {
        std::fs::create_dir_all(dev.vault_path().join("assets")).unwrap();
        std::fs::write(dev.vault_path().join("assets/logo.png"), &shared).unwrap();
    }

    a.sync_ok().await;
    b.sync_ok().await;
    for _ in 0..2 {
        for dev in [a, b, c] {
            dev.sync_ok().await;
        }
    }

    for (name, dev) in [("A", a), ("B", b), ("C", c)] {
        let files: Vec<String> = walkdir::WalkDir::new(dev.vault_path().join("assets"))
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            files.len(),
            1,
            "{name} ended up with more than the one picture: {files:?}"
        );
        assert_eq!(
            std::fs::read(dev.vault_path().join("assets/logo.png")).unwrap(),
            shared,
            "{name} has the wrong bytes at the path"
        );
    }
}

// ---------------------------------------------------------------------------
// Hostile input
//
// Everything that arrives from the mailbox was written by somebody holding the
// vault key. That makes them trusted with the *contents* of the vault, and with
// nothing else — not with where those contents land on this machine's disk.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_document_cannot_be_written_outside_the_vault() {
    // `Path::join` does not normalise: joining a relative path containing `..`
    // yields a path that escapes on write, and joining an *absolute* path
    // discards the base entirely. A device that has been compromised, or a
    // recovery phrase that has leaked, would otherwise be able to write any file
    // anywhere the app can reach — a shell profile, an ssh authorized_keys —
    // which turns access to someone's notes into access to their machine.
    //
    // The delete path has refused this for a while. The document path did not.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("Notes/ordinary.md", "# Ordinary\n\nthis one is fine\n");
    a.sync_ok().await;

    let escape_dir = tempfile::tempdir().expect("somewhere outside the vault");
    let outside = escape_dir.path().join("escaped.md");
    assert!(!outside.exists(), "precondition: the target does not exist");

    // Publish an entry whose path climbs out of the vault, exactly as a hostile
    // device would.
    mailbox.publish_document_at_path(
        &crate::sync::harness::HARNESS_E2EE_KEY,
        outside.to_string_lossy().as_ref(),
        "# Escaped\n\nwritten outside the vault\n",
    );

    let result = b.sync().await;

    assert!(
        !outside.exists(),
        "a remote entry wrote a file outside the vault at {}",
        outside.display()
    );

    // And it must not take the healthy document down with it.
    b.sync_ok().await;
    assert!(
        b.exists("Notes/ordinary.md"),
        "the hostile entry stopped an ordinary one from arriving"
    );
    let _ = result;
}

#[tokio::test]
async fn a_hostile_entry_does_not_stop_the_healthy_ones_behind_it() {
    // Two failures used to compound here. An entry that cannot be applied
    // returned an error from the whole run while retries remained, so one file
    // that would never apply — a path climbing out of the vault, say — failed
    // every sync and held up every entry behind it in the page.
    //
    // Refusing the entry is right. Refusing the run is not: the vault stops
    // moving entirely, and the user sees a sync error they cannot act on.
    let (mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    // A path that climbs out with `..`, which is the ordinary shape of this
    // attack and the one `Path::join` will happily resolve.
    mailbox.publish_document_at_path(
        &crate::sync::harness::HARNESS_E2EE_KEY,
        "../../escaped.md",
        "# Escaped\n",
    );

    // A perfectly good document published afterwards, sitting behind it.
    a.write("Notes/behind.md", "# Behind\n\nthis must still arrive\n");
    a.sync_ok().await;

    let result = b.sync_ok().await;

    assert!(
        b.exists("Notes/behind.md"),
        "a refused entry stopped the healthy one behind it from arriving"
    );

    let escaped = b.vault_path().parent().map(|p| p.join("escaped.md"));
    if let Some(escaped) = escaped {
        assert!(
            !escaped.exists(),
            "an entry wrote outside the vault at {}",
            escaped.display()
        );
    }

    // The refusal is reported once, not raised as a run failure.
    assert!(
        result.errors.len() <= 1,
        "the refusal should be reported once, got {:?}",
        result.errors
    );

    // And it settles: the next run is clean rather than repeating the complaint.
    for _ in 0..crate::sync::coordinator::MAX_INBOX_APPLY_ATTEMPTS + 1 {
        b.sync_ok().await;
    }
    let settled = b.sync_ok().await;
    assert!(
        settled.errors.is_empty(),
        "the refused entry is still being reported every run: {:?}",
        settled.errors
    );
}

// ---------------------------------------------------------------------------
// Frontmatter under a text CRDT
// ---------------------------------------------------------------------------

const TASK: &str = "Tasks/probe.md";

fn task_file(status: &str, due: &str) -> String {
    format!("---\ntitle: Probe\ntype: task\nstatus: {status}\npriority: P3\ndue_date: {due}\n---\nthe body\n")
}

fn frontmatter_field(text: &str, key: &str) -> Option<String> {
    gray_matter::Matter::<gray_matter::engine::YAML>::new()
        .parse::<serde_json::Value>(text)
        .ok()
        .and_then(|p| p.data)
        .and_then(|d| d.get(key).and_then(|v| v.as_str()).map(str::to_string))
}

/// Two fields, two devices, no overlap. This is the case the text CRDT is
/// good at, and it is worth pinning: any move to a field-level document must
/// keep it working, not merely fix the case below.
#[tokio::test]
async fn two_devices_editing_different_fields_keep_both_edits() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(TASK, &task_file("todo", "2026-08-01"));
    a.sync_ok().await;
    b.sync_ok().await;

    // A finishes it; B, not having seen that, pushes the date out.
    a.write(TASK, &task_file("done", "2026-08-01"));
    b.write(TASK, &task_file("todo", "2026-09-15"));
    for _ in 0..3 {
        a.sync_ok().await;
        b.sync_ok().await;
    }

    let merged = a.read(TASK).expect("A has the task");
    assert_eq!(merged, b.read(TASK).expect("B has the task"), "devices disagree");
    assert_eq!(frontmatter_field(&merged, "status").as_deref(), Some("done"));
    assert_eq!(frontmatter_field(&merged, "due_date").as_deref(), Some("2026-09-15"));
}

/// One field, two devices, two different values.
///
/// This used to be a known defect. The whole file lived in one `LoroText`, so
/// a merge was character-level, and two devices editing the same frontmatter
/// line did not resolve to one value — their characters interleaved. `done`
/// against `in_progress` came out as `in_pronegress`: valid YAML, meaning
/// nothing, and the task appeared in a column nobody put it in.
///
/// Frontmatter now lives in a `LoroMap`, one entry per field, where concurrent
/// writes resolve to one of them. See `sync/core/node_document.rs`.
///
/// The assertion is deliberately loose about *which* side wins. That is the
/// CRDT's business and depends on peer ids; what matters is that the answer is
/// one of the two values a device actually wrote.
#[tokio::test]
async fn two_devices_editing_the_same_field_resolve_to_one_of_the_two_values() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(TASK, &task_file("todo", "2026-08-01"));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(TASK, &task_file("done", "2026-08-01"));
    b.write(TASK, &task_file("in_progress", "2026-08-01"));
    for _ in 0..3 {
        a.sync_ok().await;
        b.sync_ok().await;
    }

    let merged = a.read(TASK).expect("A has the task");
    let status = frontmatter_field(&merged, "status");
    assert!(
        matches!(status.as_deref(), Some("done") | Some("in_progress")),
        "the merge invented a status neither device wrote: {status:?}\n{merged}"
    );
}

/// A vault written before the frontmatter moved out of the text.
///
/// The migration has to be able to happen on any device at any time without
/// coordination, because there is no version in `DocSyncPayload` to coordinate
/// with. What makes that safe is that migrating is a *deletion* from the text
/// plus a map write, and both are idempotent under concurrency: two devices
/// stripping the same frontmatter block independently both arrive at the body,
/// where two devices *inserting* the same repair would arrive at it twice.
#[tokio::test]
async fn a_document_written_the_old_way_still_reads_and_syncs() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(TASK, &task_file("todo", "2026-08-01"));
    a.sync_ok().await;
    b.sync_ok().await;

    let arrived = b.read(TASK).expect("B has the task");
    assert_eq!(frontmatter_field(&arrived, "status").as_deref(), Some("todo"));
    assert!(arrived.contains("the body"), "the body was lost:\n{arrived}");
}

/// The body still merges the way prose should.
///
/// Splitting the document must not cost the thing the text CRDT was there for:
/// two devices adding a line each still end up with both lines.
#[tokio::test]
async fn splitting_the_document_does_not_cost_the_body_merge() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(TASK, &task_file("todo", "2026-08-01"));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(TASK, &task_file("todo", "2026-08-01").replace("the body", "the body\nfrom A"));
    b.write(TASK, &task_file("todo", "2026-08-01").replace("the body", "the body\nfrom B"));
    for _ in 0..3 {
        a.sync_ok().await;
        b.sync_ok().await;
    }

    let merged = a.read(TASK).expect("A has the task");
    assert_eq!(merged, b.read(TASK).expect("B has the task"), "devices disagree");
    for line in ["from A", "from B"] {
        assert!(merged.contains(line), "the body merge lost '{line}':\n{merged}");
    }
}

/// Syncing an unchanged file must not rewrite it.
///
/// The frontmatter is rebuilt from the map on every read, so if the rebuild
/// disagreed with what is on disk by so much as a key order, every device would
/// see a change, publish it, and receive the other's — a loop with no user in
/// it. This pins that a round trip through the split document is a no-op.
#[tokio::test]
async fn a_quiet_file_is_not_rewritten_by_syncing() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(TASK, &task_file("todo", "2026-08-01"));
    a.sync_ok().await;
    b.sync_ok().await;

    let settled = b.read(TASK).expect("B has the task");
    for _ in 0..3 {
        a.sync_ok().await;
        b.sync_ok().await;
    }
    assert_eq!(b.read(TASK).as_deref(), Some(settled.as_str()), "syncing kept rewriting a file nobody touched");
    assert_eq!(a.read(TASK).as_deref(), Some(settled.as_str()), "the two devices settled on different bytes");
}

// ---------------------------------------------------------------------------
// Whiteboards
// ---------------------------------------------------------------------------

const BOARD: &str = "Whiteboards/plan.whiteboard.json";

/// A board file as the whiteboard app writes one, reduced to the parts sync
/// looks at: the stamp it resolves conflicts by, and something to tell two
/// versions apart.
fn board_file(stamp: &str, label: &str) -> String {
    format!(
        r#"{{
  "title": "Plan",
  "tags": [],
  "created_at": "2026-01-01T00:00:00.000Z",
  "metadata": {{ "updated_at": "{stamp}" }},
  "viewport": {{ "x": 0, "y": 0, "zoom": 1 }},
  "nodes": [
    {{ "id": "n1", "type": "text", "position": {{ "x": 0, "y": 0 }}, "data": {{ "label": "{label}" }} }}
  ],
  "edges": []
}}"#
    )
}

/// A board is not text, so it cannot be merged: one of the two versions wins
/// whole. Which one is decided by `metadata.updated_at`, and boards did not
/// write one — so both sides read as the empty string, `remote >= local` held,
/// and the copy that arrived replaced the copy that was here no matter which
/// was newer. This pins that the later edit survives regardless of who pulls
/// last.
#[tokio::test]
async fn the_later_edit_of_a_board_survives_whichever_device_pulls_last() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(BOARD, &board_file("2026-01-01T00:00:00.000Z", "seed"));
    a.sync_ok().await;
    b.sync_ok().await;

    // B edits first and publishes; A edits afterwards and publishes second,
    // so A's copy is the newer of the two on every measure.
    b.write(BOARD, &board_file("2026-01-01T00:00:10.000Z", "from B"));
    b.sync_ok().await;

    a.write(BOARD, &board_file("2026-01-01T00:00:20.000Z", "from A"));
    a.sync_ok().await;
    b.sync_ok().await;

    let a_board = a.read(BOARD).expect("A has the board");
    let b_board = b.read(BOARD).expect("B has the board");

    assert!(
        a_board.contains("from A"),
        "A's own later edit was replaced by B's older one:\n{a_board}"
    );
    assert!(
        b_board.contains("from A"),
        "B kept its older edit instead of taking the later one:\n{b_board}"
    );
}

// ═══════════════════════════════════════════════════════════
//  Finance: a ledger two people keep at once
// ═══════════════════════════════════════════════════════════

const LEDGER: &str = "Finance/2026-08.json";

/// A month of the ledger as Finance writes one.
///
/// `financeSchema` and the minor-unit amounts are what the app stores today;
/// `updated_at` is the stamp whole-file conflict resolution reads.
fn month_file(stamp: &str, rows: &[(&str, i64, &str)]) -> String {
    let transactions: Vec<String> = rows
        .iter()
        .map(|(id, amount, note)| {
            format!(
                r#"      {{
        "accountId": "acc-1",
        "amount": {amount},
        "category": "Food & Dining",
        "date": "2026-08-15T10:00:00.000Z",
        "id": "{id}",
        "note": "{note}",
        "type": "expense"
      }}"#
            )
        })
        .collect();

    format!(
        r#"{{
  "content": "",
  "metadata": {{
    "created_at": "2026-08-01T00:00:00.000Z",
    "financeSchema": 2,
    "transactions": [
{}
    ],
    "updated_at": "{stamp}"
  }},
  "title": "Month 08/2026",
  "type": "finance_month"
}}"#,
        transactions.join(",\n")
    )
}

/// Which transactions a device can see in its copy of the month.
fn transaction_ids(device: &HarnessDevice) -> Vec<String> {
    let text = device.read(LEDGER).expect("the device has the ledger");
    let file: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("the ledger stopped being JSON: {e}\n{text}"));

    let mut ids: Vec<String> = file["metadata"]["transactions"]
        .as_array()
        .unwrap_or_else(|| panic!("no transactions array:\n{text}"))
        .iter()
        .filter_map(|tx| tx["id"].as_str().map(str::to_string))
        .collect();
    ids.sort();
    ids
}

/// The scenario the whole of Finance rests on.
///
/// Two people share a vault, or one person has a laptop and a phone. Each
/// records a purchase while the other is offline. Both purchases happened, so
/// both have to survive — and the month they land in is one file, which is
/// where a whole-file merge loses one of them without saying so.
#[tokio::test]
async fn two_devices_recording_a_purchase_at_once_keep_both() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(LEDGER, &month_file("2026-08-01T00:00:00.000Z", &[]));
    a.sync_ok().await;
    b.sync_ok().await;

    // Offline, each records their own purchase into the same month.
    a.write(
        LEDGER,
        &month_file("2026-08-15T10:00:00.000Z", &[("tx-from-a", 4500, "lunch")]),
    );
    b.write(
        LEDGER,
        &month_file("2026-08-15T11:00:00.000Z", &[("tx-from-b", 12000, "petrol")]),
    );

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;
    b.sync_ok().await;

    for (name, device) in [("A", a), ("B", b)] {
        let ids = transaction_ids(device);
        assert!(
            ids.iter().any(|id| id == "tx-from-a"),
            "{name} lost the purchase recorded on A: {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id == "tx-from-b"),
            "{name} lost the purchase recorded on B: {ids:?}"
        );
    }

    assert_eq!(
        transaction_ids(a),
        transaction_ids(b),
        "the two devices disagree about what is in the ledger"
    );
}

/// A row deleted on one device must not be brought back by the other.
///
/// This is the half of the design that a "keep everything" merge gets wrong.
/// Two devices adding rows both win; a device *removing* a row has to win too,
/// or deleting a mistaken purchase would undo itself on the next sync.
#[tokio::test]
async fn a_deleted_transaction_does_not_come_back() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    let seeded = month_file(
        "2026-08-01T00:00:00.000Z",
        &[("tx-1", 4500, "lunch"), ("tx-2", 12000, "petrol")],
    );
    a.write(LEDGER, &seeded);
    a.sync_ok().await;
    b.sync_ok().await;

    // A deletes the mistaken row; B, offline, records something new.
    a.write(
        LEDGER,
        &month_file("2026-08-15T10:00:00.000Z", &[("tx-2", 12000, "petrol")]),
    );
    b.write(
        LEDGER,
        &month_file(
            "2026-08-15T11:00:00.000Z",
            &[("tx-1", 4500, "lunch"), ("tx-2", 12000, "petrol"), ("tx-3", 800, "coffee")],
        ),
    );

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;
    b.sync_ok().await;

    for (name, device) in [("A", a), ("B", b)] {
        let ids = transaction_ids(device);
        assert!(!ids.iter().any(|id| id == "tx-1"), "{name} resurrected the deleted row: {ids:?}");
        assert!(ids.iter().any(|id| id == "tx-2"), "{name} lost the untouched row: {ids:?}");
        assert!(ids.iter().any(|id| id == "tx-3"), "{name} lost B's new row: {ids:?}");
    }
}

/// Editing one transaction on two devices resolves to one of the two, whole.
///
/// A transaction is not prose. Merging two versions of one purchase letter by
/// letter is how you get an amount neither device entered, which is the failure
/// `node_document` was written to avoid for frontmatter and this avoids here.
#[tokio::test]
async fn the_same_transaction_edited_twice_stays_one_of_the_two() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(LEDGER, &month_file("2026-08-01T00:00:00.000Z", &[("tx-1", 4500, "lunch")]));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(LEDGER, &month_file("2026-08-15T10:00:00.000Z", &[("tx-1", 5000, "lunch for two")]));
    b.write(LEDGER, &month_file("2026-08-15T11:00:00.000Z", &[("tx-1", 4700, "lunch plus tip")]));

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;
    b.sync_ok().await;

    let settled = a.read(LEDGER).expect("A has the ledger");
    assert_eq!(settled, b.read(LEDGER).unwrap(), "the devices did not converge");

    let file: serde_json::Value = serde_json::from_str(&settled).expect("still JSON");
    let rows = file["metadata"]["transactions"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "one edit became two rows: {settled}");

    let amount = rows[0]["amount"].as_i64().unwrap();
    assert!(
        amount == 5000 || amount == 4700,
        "the amount is neither device's: {amount}"
    );
}

/// Settings are separate entries, so two devices changing two of them agree.
///
/// Before this, adding a category on the laptop while adding an account on the
/// phone meant one of the two changes was discarded whole — the config is one
/// file, and one file had one winner.
#[tokio::test]
async fn a_new_category_and_a_new_account_both_survive() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    let config = |stamp: &str, categories: &str, accounts: &str| {
        format!(
            r#"{{
  "content": "",
  "metadata": {{
    "accounts": [{accounts}],
    "currency": "USD",
    "expenseCategories": [{categories}],
    "financeSchema": 2,
    "updated_at": "{stamp}"
  }},
  "title": "Finance Config",
  "type": "finance_config"
}}"#
        )
    };

    const CONFIG: &str = "Finance/Config.json";
    let cash = r#"{"id":"acc-1","name":"Cash","initialBalance":0}"#;
    let bank = r#"{"id":"acc-2","name":"Bank","initialBalance":0}"#;

    a.write(CONFIG, &config("2026-08-01T00:00:00.000Z", r#""Food""#, cash));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(CONFIG, &config("2026-08-15T10:00:00.000Z", r#""Food","Books""#, cash));
    b.write(
        CONFIG,
        &config("2026-08-15T11:00:00.000Z", r#""Food""#, &format!("{cash},{bank}")),
    );

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;
    b.sync_ok().await;

    for (name, device) in [("A", a), ("B", b)] {
        let text = device.read(CONFIG).expect("has the config");
        let file: serde_json::Value = serde_json::from_str(&text).expect("still JSON");

        let categories = file["metadata"]["expenseCategories"].to_string();
        let accounts = file["metadata"]["accounts"].to_string();

        assert!(categories.contains("Books"), "{name} lost the category A added: {categories}");
        assert!(accounts.contains("acc-2"), "{name} lost the account B added: {accounts}");
    }
}

/// Three devices, three purchases, one month.
#[tokio::test]
async fn three_devices_recording_at_once_all_end_up_with_all_three() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b", "c"]);
    let (a, b, c) = (&devices[0], &devices[1], &devices[2]);

    a.write(LEDGER, &month_file("2026-08-01T00:00:00.000Z", &[]));
    a.sync_ok().await;
    b.sync_ok().await;
    c.sync_ok().await;

    a.write(LEDGER, &month_file("2026-08-15T10:00:00.000Z", &[("tx-a", 100, "a")]));
    b.write(LEDGER, &month_file("2026-08-15T11:00:00.000Z", &[("tx-b", 200, "b")]));
    c.write(LEDGER, &month_file("2026-08-15T12:00:00.000Z", &[("tx-c", 300, "c")]));

    for _ in 0..2 {
        for device in [a, b, c] {
            device.sync_ok().await;
        }
    }

    for (name, device) in [("A", a), ("B", b), ("C", c)] {
        let ids = transaction_ids(device);
        assert_eq!(
            ids,
            vec!["tx-a".to_string(), "tx-b".to_string(), "tx-c".to_string()],
            "{name} does not have all three purchases"
        );
    }
}

/// The ledger has to stay a file the app can open, not only a file that merged.
#[tokio::test]
async fn a_merged_ledger_is_still_the_shape_finance_reads() {
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write(LEDGER, &month_file("2026-08-01T00:00:00.000Z", &[]));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(LEDGER, &month_file("2026-08-15T10:00:00.000Z", &[("tx-a", 4500, "lunch")]));
    b.write(LEDGER, &month_file("2026-08-15T11:00:00.000Z", &[("tx-b", 12000, "petrol")]));

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;

    let text = a.read(LEDGER).unwrap();
    let file: serde_json::Value = serde_json::from_str(&text).expect("still JSON");

    assert_eq!(file["type"], "finance_month");
    assert_eq!(file["title"], "Month 08/2026");
    assert_eq!(file["content"], "");
    assert_eq!(
        file["metadata"]["financeSchema"], 2,
        "the amounts stopped saying which units they are in: {text}"
    );
    assert!(
        file["metadata"]["created_at"].is_string(),
        "a metadata key went missing: {text}"
    );

    let row = &file["metadata"]["transactions"][0];
    for key in ["id", "amount", "type", "accountId", "category", "date", "note"] {
        assert!(!row[key].is_null(), "the row lost its {key}: {row}");
    }
}

// ═══════════════════════════════════════════════════════════
//  Interactions: why they are files rather than a list
// ═══════════════════════════════════════════════════════════

/// The person's file as it used to be written, with the list inside it.
fn person_with_interactions(entries: &[(&str, &str)]) -> String {
    let mut text = String::from("---\ntitle: An Nguyễn\ntype: person\ncontact_frequency: monthly\ninteractions:\n");
    for (date, note) in entries {
        text.push_str(&format!("  - type: coffee\n    date: {}\n    note: {}\n", date, note));
    }
    text.push_str("---\n\n");
    text
}

#[tokio::test]
async fn two_devices_recording_a_coffee_at_once_keep_both() {
    // The reason interactions are their own files.
    //
    // A `.md` file is merged character by character, which is right for prose
    // and wrong for a list of objects in YAML: two devices appending to the
    // same list produce an interleave that is neither version and need not
    // parse. A person's frontmatter was the largest such list in the app and
    // grew with every recorded coffee.
    //
    // One file each has nothing to merge. Both arrive, whole.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("People/an.md", "---\ntitle: An Nguyễn\ntype: person\n---\n\n");
    a.sync_ok().await;
    b.sync_ok().await;

    // Both go offline and each records a meeting with the same person.
    a.write(
        "People/Interactions/from-a.md",
        "---\ntitle: Coffee · An Nguyễn\ntype: interaction\ndate: 2026-08-20\n---\n\nTalked about the new job.\n",
    );
    b.write(
        "People/Interactions/from-b.md",
        "---\ntitle: Call · An Nguyễn\ntype: interaction\ndate: 2026-08-21\n---\n\nQuick catch-up before the trip.\n",
    );

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;

    for device in [a, b] {
        assert!(
            device.exists("People/Interactions/from-a.md"),
            "one device's coffee went missing"
        );
        assert!(
            device.exists("People/Interactions/from-b.md"),
            "the other device's call went missing"
        );
    }
    assert!(a
        .body("People/Interactions/from-b.md")
        .unwrap()
        .contains("Quick catch-up before the trip."));
    assert!(b
        .body("People/Interactions/from-a.md")
        .unwrap()
        .contains("Talked about the new job."));

    // And the person's own file is untouched by either — it does not grow, and
    // there is nothing in it for the two of them to disagree about.
    assert!(
        !a.read("People/an.md").unwrap().contains("interactions"),
        "the person should carry no list: {:?}",
        a.read("People/an.md")
    );
}

#[tokio::test]
async fn a_list_inside_one_file_is_merged_by_character_not_by_entry() {
    // Kept as evidence for why interactions moved out.
    //
    // Two devices appending to the same YAML list inside one file is exactly
    // what a character-level merge cannot do safely: it converges — the engine
    // is doing its job — but on a result assembled from both texts rather than
    // on a list holding both entries. What comes out is not "A's entry then
    // B's"; it is whatever the diff made of two overlapping edits.
    //
    // Nothing here asserts corruption, because the exact outcome depends on the
    // diff. What it does assert is the part that matters: the merged file is
    // not simply both entries, so a list kept this way cannot be trusted to
    // hold what was put in it.
    let (_mailbox, devices) = vault_with_devices(&["a", "b"]);
    let (a, b) = (&devices[0], &devices[1]);

    a.write("People/an.md", &person_with_interactions(&[("2026-08-01", "the first one")]));
    a.sync_ok().await;
    b.sync_ok().await;

    a.write(
        "People/an.md",
        &person_with_interactions(&[("2026-08-01", "the first one"), ("2026-08-20", "coffee from A")]),
    );
    b.write(
        "People/an.md",
        &person_with_interactions(&[("2026-08-01", "the first one"), ("2026-08-21", "call from B")]),
    );

    a.sync_ok().await;
    b.sync_ok().await;
    a.sync_ok().await;
    b.sync_ok().await;

    let merged = a.read("People/an.md").unwrap();
    assert_eq!(
        merged,
        b.read("People/an.md").unwrap(),
        "the devices should at least converge on the same text"
    );

    let both_intact = merged.contains("note: coffee from A") && merged.contains("note: call from B");
    let expected = person_with_interactions(&[
        ("2026-08-01", "the first one"),
        ("2026-08-20", "coffee from A"),
        ("2026-08-21", "call from B"),
    ]);
    assert!(
        !both_intact || merged.trim() != expected.trim(),
        "if this ever produces exactly the list both devices meant, the \
         argument for moving interactions out has weakened and this test \
         should be revisited:\n{merged}"
    );
}
