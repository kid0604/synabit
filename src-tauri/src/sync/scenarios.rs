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
