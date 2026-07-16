# Direct P2P Phase 0 safety baseline

Phase 0 prevents the known direct-P2P v1 rollback path from running in a
release build. Managed Sync Server transport remains available and is the
fallback while direct protocol v2 is being implemented.

## Rollout gates

- Persisted v2 rollout key: `feature:direct_p2p_v2` in `kv_store`.
- Current direct protocol version: `1`.
- A requested v2 flag does not enable protocol v1.
- Release builds cannot opt into protocol v1.
- Debug builds may temporarily set
  `SYNABIT_ENABLE_UNSAFE_DIRECT_P2P_V1=1` to reproduce the legacy issue on two
  test devices.

Both outgoing transport creation and incoming direct connections use the same
gate. With paired devices but no Sync Server, a blocked direct-only sync returns
an explicit error rather than reporting an empty successful sync.

## Automated baseline

The ignored regression test models the production v1 ordering where a peer
stages a pushed edit but serves its stale local snapshot before applying it:

```sh
cd src-tauri
cargo test --lib direct_v1_new_note_edit_must_not_roll_back -- --ignored
```

The test is expected to fail during Phase 0. Phase 1 must change the production
ordering, replace the model with the real direct engine harness, make the test
pass, and remove `#[ignore]`.

## Manual two-device reproduction

Use debug builds and an isolated test vault on both devices. Do not use a real
vault because this deliberately enables the unsafe protocol.

1. Set `SYNABIT_ENABLE_UNSAFE_DIRECT_P2P_V1=1` for both app processes.
2. Pair the devices and disconnect the managed Sync Server.
3. On device A, create a note, immediately replace its title, and enter body
   content.
4. Wait for watcher-triggered syncs without editing on device B.
5. Capture log records sharing each `sync_run run_id=...`.
6. Confirm whether A pulls B's placeholder/empty snapshot after its push.

Expected legacy failure: title or body rolls back and multiple sync requests are
queued. Expected Phase 1 behavior: both devices converge to the edit and become
idle after at most one trailing sync.

## Structured log fields

Every requested sync now records:

- `run_id`
- normalized `trigger`
- redacted `vault_tag`
- redacted `transport_tag`
- provider and phase
- pulled, pushed, deleted and error counters
- transmitted and received byte counters

Free-form trigger labels are normalized to `unknown`, preventing untrusted log
field injection.
