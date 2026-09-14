# ADR — spaces, and the graphs inside them, 2026-09-12

**Status:** proposed
**Decides:** what isolates one body of work from another, where a typed
relationship is written down, and what a saved diagram owns.
**Continues** `syn/board.rs`, whose `inside`/`move_into` ops answered "a place,
not a prefix" for one board and left the same question open for the vault.

## The problem

Two conversations from this week, both in the vault:

`545bb8ee` (2026-09-12, 16 turns) draws a network as Mermaid, one item at a
time. Every turn reprints the whole diagram. By turn 15 a single sentence —
*"tất cả những thằng này đều kết nối đến DNS/NTP server"* — adds nine edges and
the picture stops being readable. Nothing was ever edited: it was redrawn eight
times, and no turn can be diffed against the one before it.

`82898d0c` (2026-09-11, 48 turns) hits the harder wall. Turns 10 through 16:

> "hãy để DC 1 bên trái, DC 2 bên phải" → "mày đang vẽ DC 1 bên phải mà" →
> "vẫn thế" → "vẫn không được"

That is not a model failing. Mermaid hands placement to dagre; the author has
no vocabulary for "left". Turn 42 moves the same diagram to a whiteboard, where
placement is nothing *but* the author's — and gets *"hơi xấu, vẽ lại xem"*,
because the coordinates for twenty-five boxes in nested frames were being
invented by hand. One tool has layout without authority, the other authority
without layout.

Underneath both is the same missing thing: there is no model. There are only
pictures of one.

And the vault has a third symptom of the same absence. Draw company A's systems
as notes with links, then company B's, and Nexus merges them — one `SW Core`,
two meanings. The graph stops answering anything.

## What already exists

This is mostly not a new feature. It is three additions to parts that are
already load-bearing.

| Already here | Where |
| --- | --- |
| A node is a file; `type:` is free-form; `Schema/*.md` defines a kind's fields | `utils/node_parser.rs`, `vault/Schema/` |
| `node_edges(source_id, target_id, edge_type, relation)`, keyed on stable ids so a moved file keeps its links | `db/schema.rs:403`, tested at `db/nodes.rs:1570` |
| A typed relationship declared in frontmatter and indexed as an edge | `connections: [{person_id, relation_type}]` → `graph_parser.rs:188` |
| An app that draws any kind with no registration step | `mini-apps/things/` |
| Frames that hold their contents, and a layered arrangement | `syn/board.rs` (`inside`, `move_into`, layering at :429) |
| Tool declarations that are omitted when a vault cannot use them | `VaultTools::definitions`, board tools |

The `connections:` list is the important one. The mechanism this document needs
is already running in production for people; what follows mostly widens it.

## The three levels

- **Space** — an *identity* boundary. Inside one space a name means one thing.
- **Graph** — a saved slice: which nodes, arranged how, with what hidden.
- **Layer** — which set of truths is being read.

The rule that decides between the first two, and it is the only rule a user has
to learn:

> The same name on both sides — is it the same thing?

Three independent infrastructures at one company that share DNS, AD and a WAN
are **one space, three graphs**. Three court cases are **three spaces**, and
must be, because "Nguyễn Văn A" is not the same person in each and a lawyer has
a duty not to let them touch.

Spaces do not nest. A nested space has to answer whether the child sees the
parent's nodes; "yes" reintroduces the exact leak this fixes, and "no" makes the
nesting decoration bought with a tree nobody can navigate in six months. Spaces
are flat and carry a free-text `group` for the picker to head its sections with.

## Decision 1 — a space is a folder

`Spaces/<Name>/` owns its members. The space itself is
`Spaces/<Name>/space.md`, `type: space`. Inside, the existing per-type
convention continues under that root:

```
Spaces/Company A/space.md
Spaces/Company A/Device/sw-core.md
Spaces/Company A/Notes/maintenance-window.md
Spaces/Company A/Graphs/payment-flow.md
```

`folder_for_type` gains a prefix rather than a second convention; the mirror
test against `nodeRoutes.ts` keeps both writers honest, as it does today.

**Not a `space:` property in frontmatter.** A property is a second source of
truth the moment a file is moved, it can be dropped by any writer that does not
know about it, and the vault stops being legible in Finder — which the
repository already treats as most of the point. A path cannot be forgotten.

Membership is therefore derived, not stored: `nodes.space` is a column filled
at index time from `nodes.id`, which is already the relative path. Renaming a
space renames a folder; every edge survives it, because edges are keyed on
stable ids and there is a test that says so.

## Decision 2 — resolution stops at the boundary

This is the whole fix for the merged-graph complaint, and it is one function.

`NodeResolver` becomes space-aware:

- a source inside space *S* resolves titles **only within S**;
- a source outside every space resolves **only among nodes outside spaces**;
- a full vault path resolves anywhere, always — the deliberate escape hatch;
- anything else becomes `ghost:` exactly as it does today.

No fallback across the boundary. A fallback is precisely the bug: `[[SW Core]]`
in B quietly pointing at A's switch is how a graph becomes useless while still
looking fine.

The link picker shows same-space matches first and labels the others
`⬡ A · khác space`, greyed; choosing one writes a path, not a title. A
cross-space edge draws dashed and always carries its space label.

## Decision 3 — a typed edge is written in the file

`connections:` widens to any kind:

```yaml
connections:
  - target: SW Core        # title, stable id, or vault path
    relation_type: plugs_into
    layer: physical        # optional
    since: 2026-04-03      # optional
    until: 2026-06-12      # optional
    note: 2x10G LACP       # optional
```

- `person_id` stays an accepted spelling of `target`, and entries written that
  way keep `edge_type: person_link`, so People's queries
  (`WHERE e.edge_type = 'person_link'`) are untouched. New entries index as
  `edge_type: connection`, `relation: <relation_type>`.
- The edge lives on the **source node only**. One writer per fact; the reverse
  direction is what `get_linked_nodes` already computes.
- Every edge is stored directed. Whether it *draws* as an arrow or a plain line
  is a property of the relation's definition, not of the row.

**One thing has to change under this.** `node_edges` is
`UNIQUE(source_id, target_id, edge_type)`. Two connections between the same
pair with different relations — `App ⎯plugs_into→ FW Core` and
`App ⇢depends_on→ HSM` via the same pair — would collide the moment both carry
`edge_type: connection`. The constraint has to become
`UNIQUE(source_id, target_id, edge_type, relation)`, with `layer`, `since` and
`until` added as columns. That is a table recreate, and it is free: this table
is cache, rebuilt from files, and the whiteboards table was already replaced
this way at `db/schema.rs:395`.

### `since` / `until` ship empty, on purpose

No v1 screen reads them. They go in anyway, for the reason `boardFile.ts`
already gives about per-item stamps:

> *"a board saved today without them can never be merged later"*

The same holds here. A court case, a family tree, a construction schedule all
need "this was true between these dates". Add the fields after five hundred
relationships have been typed in and those five hundred never get dates. Two
empty keys now are cheaper than a migration that cannot be done.

## Decision 4 — the vocabulary belongs to the space

`Spaces/<Name>/space.md`:

```yaml
type: space
title: Company A
group: Công ty A
color: "#2563eb"
layers: [physical, logical]
relations:
  - key: plugs_into
    label: cắm vào
    layer: physical
    directed: false
  - key: calls
    label: gọi tới
    layer: logical
    directed: true
```

Nothing about networks exists in the code. A case file declares
`represents / pays / owns`; a novel declares `loves / betrays / child_of` and
layers `truth / believed / revealed`. The drag-to-connect popover is generated
from this list, and the layer switch on the canvas is generated from the layers
actually in use.

When Syn hears a verb that is not in the list it must ask before adding —
*"'thanh toán cho' là loại mới, hay chính là 'chuyển tiền cho' đã có?"*.
Without that question the vocabulary reaches forty synonyms in a quarter and
every query over it is wrong.

## Decision 5 — a graph owns arrangement, never data

`Spaces/<Name>/Graphs/<title>.md`, `type: graph`:

```yaml
type: graph
title: Luồng thanh toán
include: { kinds: [device], groups: [DC 1, DC 2] }
layers: [physical]
collapsed: [<stable id of DNS/NTP>]
pins:
  <stable id>: { x: 120, y: 40 }
```

Pins are keyed on stable ids, so they survive renames and moves like everything
else. **Pins live in the graph, not on the node**: the same switch sits in two
diagrams at two positions, which is exactly what neither Mermaid nor the
whiteboard could express. Deleting a graph deletes a way of looking, not a
thing.

A new kind rather than the existing `view`, for the reason `NodeType::View`
already records about `Filter`: sharing a type puts one app's saved views in
another app's menu.

## Decision 6 — one arrangement algorithm, called from two places

The canvas is Vue Flow, already a dependency, rendering nodes and edges from
the index. It is **not** the whiteboard's board file; "export to whiteboard" is
a one-way snapshot through `diagramToBoard.ts`.

Layout is the layered pass that already exists in `board.rs:429`, exposed as a
command and called by the canvas on demand. Not ported to TypeScript, and not a
new dependency: this repository's own rule is that two copies of one fact drift,
and it enforces that with mirror tests for `folder_for_type` and `INTERNAL`.
Re-layout is a button, not a drag handler, so an IPC round trip costs nothing
anyone can see.

Dragging a node pins it. A pinned node is a fixed input to the layout, not a
suggestion, and `Sắp xếp lại` must be undoable with ⌘Z — it is the most
frightening button on the screen and an un-undoable one will simply never be
pressed.

## Decision 7 — the tools, and what they cost

`PAYLOAD_BUDGET_CHARS` is 18,400 with about 130 characters unspent, and the
comment above it says the next tool has to make its own case. So:

- **Declared only when the vault holds at least one space**, the way board
  tools are omitted from a vault with no boards. A vault with no spaces pays
  nothing.
- **Two new tools, not five.** `graph_edit(space, graph?, changes[])` with ops
  `add · connect · disconnect · group · rename · remove`, and
  `read_graph(space, graph)` returning a compact text rendering. Reading nodes
  stays `query_nodes` with a `space` argument — a parameter is far cheaper than
  a tool.
- The active space goes into the system prompt as **one line**, and
  `create_node` / `query_nodes` default to it. Ambient scope is what makes
  `545bb8ee` work without a word of extra prompting.
- Every write returns a **diff**, never a redrawn picture:

```
⬡ Company A · Luồng thanh toán
+ Redis (Internal, TPZ)
+ App PSS —dùng→ Redis
− App PSS —cắm vào→ HSM
```

A write that lands in a space other than the active one must say so in that
block. Silently writing into the wrong company is the worst thing this design
can do, and the only defence is that it is always visible.

## The screens

- **A chip in the title bar**, left of the title: `⬡ Tất cả` (default, nothing
  changes for anyone who never makes a space) or `⬡ Company A`, plus a 2px
  coloured stripe down the window edge. Scope is a mode, and a hidden mode is
  the worst kind of bug: at 9am the notes are simply gone and nothing says why.
- **The picker**, sectioned by `group`, `Tất cả` pinned at the top, `Esc`
  returns there. `⌘K` jumps by name. Switching does not change the open app.
- **The graph screen**: tree of groups on the left, canvas centre, inspector
  right. Layer switch top right. The inspector lists a node's connections by
  relation and is where they are edited — dragging changes position, the
  inspector changes truth, and the two never mix.
- **Ubiquitous nodes**: past a threshold of connections the app *offers* to
  collapse one; collapsed, it draws as a dot on each consumer rather than n
  lines. DNS/NTP, the bank everyone holds an account at, the capital city every
  character passes through — the same mechanic, no name-matching.
- **Nexus at `Tất cả`**: each space collapses to one sphere sized by its count;
  double-click expands it in place. The whole vault stays visible without the
  graphs melting into each other.

## Not in v1

Time slider (fields only). Borrowed/shared spaces. Cross-space queries. Impact
analysis. Editing the canvas on a phone — the graph is read-only there.

## Build order

Each step is worth shipping alone, which is the test of whether the order is
right.

1. **Space as folder** — `nodes.space`, the chip, the stripe, scope filtering in
   Nexus / Things / Notes / Whiteboard via a `scoped` flag in `appRegistry.ts`.
   Tasks, Calendar, Finance and Feeds ignore scope; widening later is easy,
   narrowing is not.
2. **Space-aware resolution** — the `NodeResolver` change and the labelled
   cross-space picker. This is the fix for the original complaint and it lands
   here, before any canvas exists.
3. **`connections:` widened** — parser, the unique-constraint recreate, and the
   inspector list in Things. At the end of this step the whole of `545bb8ee` can
   be entered and corrected as data, with no diagram anywhere.
4. **The graph view** — Vue Flow, layered layout over the command, pins, layer
   switch, collapse.
5. **`graph_edit` + the diff card** — Syn writing into it, previewed in the
   pane beside the conversation.

**Gate between 3 and 4.** If, after step 3, no second system has been entered —
by hand or through Syn — stop. A modelling tool nobody feeds twice is a tool
with no audience, and step 4 is where the weeks are.

## What would show this is wrong

- The relation vocabulary passes ~15 entries in one space inside a month:
  the "is this new?" question is not working, and queries over relations are
  already unreliable.
- Pins accumulate until every node is pinned: the layout is not good enough to
  be worth keeping, and the honest response is to drop auto-layout, not to
  tune it.
- Step 2 ships and the merged-graph complaint persists: identity, not
  resolution, was the problem, and spaces were the wrong cut.
