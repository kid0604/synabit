# Files — performance baseline, 2026-08-26

Numbers to compare against, not targets to hit. Each one comes from an
`#[ignore]`d probe that lives beside the code it measures, so a regression can
be reproduced rather than argued about.

Run them all:

```bash
cargo test --lib -- --ignored --nocapture scan_cost list_cost text_cost backlink_cost
```

Measured on an Apple silicon laptop, debug build, in-memory SQLite. Debug is
deliberate: it is the build these run under during development, and a release
figure would flatter every one of them.

| What | Probe | Measured |
| --- | --- | --- |
| Scan 10,000 files, first pass | `scan_cost_for_ten_thousand_files` | **1.27 s** |
| Scan 10,000 files, rescan | same | **1.22 s** |
| List 50,000 files (SQL) | `list_cost_for_fifty_thousand_files` | **260 ms** |
| List 50,000 files (build + serialise) | same | **335 ms**, 14 MB of JSON |
| Read 1,000 office documents | `text_cost_for_a_thousand_documents` | **30.6 s** (30.6 ms each) |
| Backlinks on a 10,000-node vault | `backlink_cost_on_a_vault_of_ten_thousand_nodes` | **236 µs** |

## What each number is worth knowing

**Scanning.** Before the resolver was hoisted out of the walk, the same ten
thousand files took over ten minutes — the walk rebuilt an index of every node
in the vault once per file. Content hashing was added afterwards and cost
nothing measurable, because it was paid for by dropping `infer`'s per-file
magic-byte read and by not writing edges for files that have no links.

**Listing.** The one figure here that is not comfortable. `query_files` returns
every indexed file in a single array and the front end filters it in the
browser, so a fifty-thousand-file library ships 14 MB of JSON across the IPC
bridge and the webview parses all of it. It is the shape, not the query: 260 ms
of SQL is fine, and the rest is the payload. Pagination is the fix and it is a
front-end change — the filter model, the search box, the tag list and the
duplicate screen all read the whole array today.

**Reading documents.** Generated `.docx` of four thousand paragraphs each, which
is far larger than an ordinary document. PDFs are slower per page, so treat this
as a floor rather than a promise about them.

**Backlinks.** The scan it replaced took 1.6 ms on the same data, so this is
about seven times faster — but speed was never the reason to change it. The
old answer was a substring match, which reported every note containing the word
"note" as a user of a file called `note.pdf`.
