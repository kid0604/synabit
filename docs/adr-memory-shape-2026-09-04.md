# ADR — the shape of memory, 2026-09-04

**Status:** accepted; implemented 2026-09-04
**Decides:** how a memory reaches the model, how one gets written, and how one
stops applying.
**Continues** `docs/adr-rag-vs-agentic-2026-09-03.md`, whose conclusion this
document argues was applied to half the app.

> The design below is not a refinement of what exists. The measurement that
> prompted it showed that the memory feature has a working write path and a read
> path that has never once run in production. Everything else here follows from
> taking that seriously.

## The problem

Syn's memory has two routes into the model:

- **pinned** memories are rendered into the system prompt on every message, by
  `memory::pinned_block()`, inside a 3,200-character budget;
- **everything else** is reachable only if the model decides to call the
  `recall` tool.

Fifteen real runs sit in `Syn/runs/`. Between them they made 26 tool calls:

| tool | runs that used it |
| --- | --- |
| `query_nodes` | 8 |
| `update_node` | 4 |
| `get_node` | 2 |
| `remember` | 1 |
| `create_node` | 1 |
| **`recall`** | **0** |

`recall` has never been called. Not once, by any model, on any question.

The consequence is not theoretical. The only memory Syn has ever written —
`SynMemory/Người dùng là dân IT (công nghệ thông tin).md` — carries
`pinned: false`, because that is what `remember` defaults to
(`tools.rs:815`). An unpinned memory reaches the model only through `recall`.
So the single thing Syn has remembered has never reached Syn. The Memory tab
says so in the interface, and nobody read it as a bug: *0 pinned · 0 of 3200
characters*.

This is the ADR of 2026-09-03 repeating itself in a room nobody checked. That
document measured stuffing against letting the model search, and stuffing won.
The finding was applied to vault context. Memory kept the agentic path for
anything not pinned, and the agentic path does not run — not because the tool
is badly described (it is described well, and names habits and preferences
explicitly), but because a model answering an ordinary question does not first
suspect that something it was never told might exist.

## What the numbers say about the scale of the problem

Two measurements, both taken today.

**Memories are tiny.** At the length of the one real memory (43 characters),
`pinned_block()` fits fifty of them inside its 3,200-character budget:

```
 20 memories -> block  1482 chars, 20 of 20 survive
 40 memories -> block  2622 chars, 40 of 40 survive
 60 memories -> block  3249 chars, 50 of 60 survive
```

Below fifty memories, nothing is ever cut. The pinned/unpinned distinction buys
nothing at all until the fiftieth memory, and it is currently costing the entire
feature.

**Reflection is cheap.** The reflection prompt is 3,674 characters against a
chat turn's 37,743 (system prompt 20,940 plus 16,803 of tool schemas) — 10%,
measured at reflection's worst case of forty listed memories and before the chat
turn adds any conversation history.

Both numbers point the same way: **this is a small-data problem wearing
large-data machinery.** A person's standing memory is a few dozen sentences. It
is kilobytes. Every mechanism that ranks, retrieves, scores or decays is
apparatus for a scale this will not reach for years, and each one is a place to
be wrong quietly.

## Decision

### 1. Reading: everything, always, until it does not fit

The prompt carries *all* memories, not the pinned ones. `pinned` stops meaning
"exists as far as the model is concerned" and starts meaning "survives eviction
when the budget is reached". Below fifty memories the flag has no observable
effect, which is correct — it is a tie-breaker, and there is no tie.

Cost: roughly +2,800 characters on a 37,743-character turn, and only once the
user actually has forty memories. Seven per cent for the difference between a
feature and a filing cabinet.

This is necessary and, per the model half below, not sufficient: a memory in the
prompt is still a memory the model may not act on. §8 covers that half.

### 2. The memory section must trim inside itself

Today `SectionKind::Memory` is one section at trim rank 1 (`prompt.rs:390`), and
the trimmer drops whole sections. Under budget pressure Syn therefore forgets
*everything* rather than the least important thing — an all-or-nothing cliff
that gets more likely exactly as memory becomes more valuable.

Memory must trim per-memory: drop the lowest-priority entries until the section
fits, and drop the section only when nothing fits. Priority order: pinned first,
then `instruction` and `preference` (the kinds whose absence is most visible in
an answer), then the rest by `last_confirmed`, newest first.

### 3. `recall` is demoted to the overflow path

It stays, because past fifty memories something has to choose. Its description
must stop presenting it as the way to find out about the user — that framing is
what made its absence invisible — and start saying that everything remembered is
already in the prompt, and this searches the remainder when there is one. Below
the budget, a correct model should never need it.

The relevance ranking added on 2026-09-04 (score first, `pinned` and recency only
breaking ties) is still needed for this path and for the eval. It should be
recorded honestly: it optimised a path that had never run.

### 4. Writing: three doors, and the third is missing

**Explicit.** The user asks; `remember` writes immediately. Default `pinned:
true` — a person who says "remember this" has already made the judgement the
flag encodes.

**Reflection.** Proposes; the user disposes. Unchanged in shape. Accepted
proposals default to `pinned: false`, because a machine's guess should not
outrank a person's instruction when something eventually has to go.

**Correction.** When the user corrects Syn — *"không phải, tao..."* — that is the
highest-signal moment the app ever gets, and today it produces nothing. It is
also rare, so a dedicated trigger is cheap. Hermes creates a skill when "the
user corrected its approach"; the same signal is worth at least a memory
proposal here, flagged as arising from a correction so the tray can show it
differently.

### 5. Forgetting: the half that does not exist

The system can currently only accumulate. Four changes, in order of how much
they hurt:

**A declined proposal must stay declined.** Proven defect: queue a proposal,
decline it, propose the identical body again, and it comes back — `left: 1`.
`proposal::add()` dedups against the queue, and a declined proposal is no longer
in the queue. The doc comment above it promises the opposite: *"the same
conversation happening twice produces the same suggestion twice and the user
should not have to decline it twice."* Keep a bounded list of declined bodies in
`proposals.json`, matched with the same Unicode case folding `add` already uses.
It belongs in the synced file, not a dotfile: a decline is the user's judgement
about themselves and should travel between their machines, unlike a consent
grant, which is a judgement about one device.

**Supersession must be proposed, not inferred.** `supersedes` is accepted by
`remember` and nothing ever sets it. `memory::conflicting()` already runs at
write time (`tools.rs:832`) and reports clashes of the same kind and subject back
to the model. Reflection should be given the same information and required to
name what a proposal replaces when it conflicts. Accepting such a proposal
retires the old memory rather than leaving two contradictory sentences in the
prompt.

**Review must surface, not run.** `review_after` exists with no flow behind it.
It does not need a background job: the Memory tab already has a *Still true?*
button. A memory past its review date sorts to the top of the tab with the
question showing. The user answers it when they are already looking.

**Decay demotes; it never deletes.** A memory not confirmed in months and never
pinned loses its place in the trim order — nothing more. Files in the user's
vault are not deleted by a heuristic. This is safe, reversible, and self-
correcting, and it is the only decay this design has.

### 6. Cadence stays where it is, for now

Reflection continues to run after every completed run. At 10% of a turn it is
defensible, and the argument for batching it over several exchanges is about
evidence quality — a standing preference usually shows across turns, not in one
— rather than cost. That argument deserves a measurement before it gets a
rewrite. The logging added on 2026-09-04 produces exactly that measurement: the
`all already in the queue` line counts how often reflection re-proposes what is
already pending, which is the duplicate rate that would justify the change.

Deciding cadence now, before that number exists, would repeat the mistake this
document is about.

### 7. Provenance belongs in the interface

Every memory already carries `source_run`. The Memory tab should link it, so
"why do you believe this about me?" is one click rather than an argument. This
is the cheapest trust feature available and it is already paid for.

## What this rejects

**Embeddings or vector search for memory.** At fifty items, word matching is
exact, free and debuggable. Embeddings add a model dependency, a migration, an
index to keep coherent with the vault, and a failure mode that produces plausible
wrong answers — to solve a problem that starts somewhere past five hundred
items.

**Importance scoring and decay arithmetic.** The Generative Agents design
accumulates importance and reflects on a threshold. It is a good design for an
agent with thousands of observations per day. Syn has forty sentences.

**A separate memory store.** Memories are vault files. They sync, they are
editable in any editor, the user can delete one with `rm`. That is the app's
whole thesis and memory should not be the exception.

**Automatic deletion of anything.** See decay, above.

**Changing reflection cadence in this change.** See §6.

## What this breaks

Making the prompt carry every memory changes rendered prompts, so the byte-exact
prompt snapshots will fail. They must be re-blessed deliberately, with the diff
read first — `SYN_BLESS_SNAPSHOTS=1` is the mechanism, not the decision.

`only_the_pinned_ones_ride_in_every_prompt` asserts today's contract exactly and
must be rewritten. Its replacement is the statement of the new contract: every
memory rides in every prompt until the budget is reached, and then pinned ones
are the last to go. The rewrite is the interesting part of this change; if it is
hard to state, the design is wrong.

## What the model half found

The P2 gate's model half ran to completion on 2026-09-04: twenty cases, two of
them controls, forty API calls, no errors. Each case is asked twice — once with
the memory in play and once without — and the pair is read by a person, because
the ADR of 2026-09-03 established that a scorer which is confidently wrong does
more damage than no scorer.

| group | cases | `with` better | `with` no different | `with` failed the case |
| --- | --- | --- | --- | --- |
| pinned (in the prompt today) | 14 | 9 | 3 | 2 |
| unpinned (via `recall`) | 4 | 4 | 0 | 0 |
| controls (must not change) | 2 | — | 2 | 0 |

The wins are not subtle. Asked to book 17:00 with someone who does not meet
after 16:00, `without` says *"Được"*; `with` refuses and offers a slot before
16:00. Asked what to prepare for Friday's review, `without` says it has no
information; `with` says a working demo, not slides. Asked to suggest a laptop,
`without` spends one of its five questions asking which operating system —
which the memory already answers — while `with` opens with MacBooks. Asked what
to train today, `without` prescribes squats, lunges and Romanian deadlifts to
someone with a right knee injury.

**The four `recall` cases all won, and none of them can happen today.** Their
memory reached the model because the harness called `recall` directly. In
production nothing calls it. Those four are answers Syn is capable of and does
not give — which is the case for §1, now with model evidence behind it.

### The correction this forces

§1 was written on the premise that reaching the model is the bottleneck. The
pinned group says reaching is **necessary and not sufficient**. Three of its
fourteen cases had the memory sitting in the prompt and did not act on it:

- *"Draft a short note to the team"*, against a memory reading **always write to
  the team in Vietnamese, even when asked in English**. The reply stayed in
  English. The memory was an instruction that contradicted the surface framing
  of the request, and the framing won.
- *"Đặt bàn tối thứ Bảy cho 4 người"*, against **wife has a severe seafood
  allergy**. Neither reply mentioned it.

  This case also carried a mistake of mine, corrected here. I first recorded it
  as the one place `with` came out behind, because `without` worked out that
  Saturday was 07/09/2026 and `with` did not. 07/09/2026 is a Monday. `without`
  was wrong and `with` had declined to repeat a wrong date, so the gate's
  "worse on none" never had a counterexample. I marked an answer as evidence
  without checking it — the same failure the 2026-09-03 ADR records four times
  over, running in the other direction.
- *"Sắp xếp chỗ ăn trưa"*, against **vegetarian since January 2026**. Both
  replies asked where the office is; neither said it would look for somewhere
  vegetarian.

That is a different failure from the one this document was written about, and it
does not have a retrieval fix. It gets §8.

Two more cases produced no signal, and both are defects in the eval rather than
in the app. The emoji case asks for a product caption with no product, so the
model asks for details and never writes a caption — the constraint under test is
never exercised. The Dijkstra case asks a technical question, which draws a
technical answer with or without the memory that says to be technical; the only
difference was that `with` wrote `relax` where `without` wrote the mistranslated
`thư giãn`. Both cases need rewriting before the next run, and are recorded here
so the twenty is not read as twenty pieces of evidence.

## Decision 8 — the memory section must instruct, not only list

Memories are rendered under a heading and left to speak for themselves. For a
fact that is enough: a model that sees *seat 14A* uses it when asked about the
seat. For an instruction that contradicts what the request seems to ask for, it
is not — the request is in the last message and the instruction is in a list
several thousand characters earlier, and the last message wins.

So the section states what to do with its contents: check them against the
question before answering, and treat a memory of kind `instruction` as binding
even when the request implies otherwise. Instruction-kind memories render as
directives rather than as bullet points of trivia, which is what they are.

This is cheap — it is prompt text — and it is measurable by the same twenty
cases, which is the point. If the language case does not flip, the wording is
wrong and should be changed again rather than defended.

## §8, implemented and re-measured

> Every other decision here was implemented the same day. What follows this
> section records the measurement §8 was judged on; the implementation notes for
> the rest are at the end.

`pinned_block()` now instructs and sorts. The block opens by saying what to do
with its contents; memories of kind `instruction` render as directives under a
heading that says they hold even when the request is worded as though they do
not; and instructions are budgeted before facts, so a fact can never crowd one
out. Two tests cover it, and each was checked by breaking the code to confirm
it fails — the second one had to be rewritten after it passed against the very
bug it named.

The same twenty cases, re-run against the new block:

| | before §8 | after §8 |
| --- | --- | --- |
| `with` better | 13 | **16** |
| `with` failed the case | 2 | **0** |
| no signal (defective cases) | 3 | 2 |
| controls unchanged | 2 | 2 |

All three failures flipped — and a third run, after the rest of the design was
implemented, showed that claim was worth less than it looked. See *What a third
run showed* below.

- The team note came back in Vietnamese. This was the case §8 was written for,
  and the ADR said in advance that if it did not flip the wording was wrong.
- Lunch: *"tao nhớ mày ăn chay nên sẽ ưu tiên quán không thịt, không cá"*.
- The restaurant booking: *"Vợ bạn bị dị ứng hải sản nặng, nên mình cũng sẽ ghi
  chú yêu cầu tránh hải sản khi đặt"* — with Saturday correctly dated 05/09.

Two cases got visibly stronger without being aimed at. The onion memory now
attaches its constraint to every dish in the list rather than as one closing
remark, and the Tokyo case commits to a departure time instead of asking for
the date first.

The two remaining no-signal cases are the defective ones already described. They
are not evidence that §8 fell short; they are cases that cannot report either
way until they are rewritten.

## What this still does not measure

How often a person's memories actually conflict, which is the assumption under
§5's supersession work. If conflicts turn out to be rare, that section is
over-built and should shrink.

Whether the gains survive contact with a real vault. Every case here was run
against an empty vault, so the memory block had no vault context competing with
it for the model's attention — and `SectionKind::VaultContext` is precisely what
sits next to it in a real prompt.

## What was built

Every decision above is in the code. Notes on where each one landed, and on the
two places the implementation departed from what was written.

**§1, §2, §8 — `syn/memory.rs`.** `pinned_block` became `memory_block` and takes
every memory. `PINNED_BUDGET_CHARS` became `MEMORY_BUDGET_CHARS`, since it no
longer budgets a subset. `shrink_block` lets the prompt trimmer take entries off
a block instead of removing the section, and `prompt.rs`'s `fit()` calls it
before it will drop memory.

Four tests asserted the old contract exactly and had to be rewritten. Their
replacements are the new contract, which is what §"What this breaks" said would
be the interesting part: *every memory rides in every prompt, and pinned ones
are the last to go*. One of them —
`what_is_pinned_reaches_the_prompt_and_the_rest_waits_to_be_recalled` — had been
passing while protecting the defect this document is about.

**§3 — `syn/tools.rs`.** `recall`'s description now says everything remembered
is already in the prompt, and that the block ends with a note when there is more
than fits. It reads as an overflow tool, because that is what it is.

**§4 — `syn/tools.rs`, `commands/syn.rs`, `syn/reflect.rs`.** `remember` pins by
default; accepting a proposal passes `pinned: false` explicitly. The correction
door is in the reflection prompt rather than in a detector: the reflector is told
that a correction is the strongest evidence the app gets and asked to set
`from_correction`. No keyword matching — this app's users write Vietnamese and
English in the same sentence, and a regex for "no, actually" would find one of
them.

**§5 — `syn/proposal.rs`, `commands/syn.rs`.** Declines are recorded and
consulted, which closes the proven defect. Supersession is proposed by the
reflector, naming the entry it replaces *by its text*, since text is what the
reflector is shown; the id is resolved when the user accepts, and the retired
memory is trashed rather than deleted. A claim that names nothing real is
dropped at the point of proposal — otherwise the accept path finds nothing to
retire, writes the new memory anyway, and the user is told one thing replaced
another while keeping both.

**Not built: a `forget` tool.** The roadmap asks for one as a friendlier name
for `trash_node`. The tool inventory test already refuses it, in writing, and
its reason is better than the roadmap's: memories are nodes, `trash_node`
removes one and `restore_node` brings it back, and two tools doing one job is
what the collapse from twenty tools to twelve was for. Every entry costs tokens
on every turn of every conversation. What the roadmap actually wanted — that the
model know it can forget — is served by the last sentence of `remember`'s own
description: *"To stop remembering something, trash_node its id."*

**§5c, §7 — `RunInspector.vue`, `useSynMemory.ts`.** A memory past its review
date sorts to the top of the list with the question showing. `source_run` is a
button. The ordering is a pure exported function rather than a comparator inside
the component, so it is tested without mounting anything.

### Where this departed from the design

**Declines went in their own file, not in `proposals.json`.** §5 says to keep
them in the queue file. The queue is truncated to its newest forty and a decline
must outlive the proposal it killed, so the two want different lifetimes; and
adding a shape to a file already in people's vaults means a migration for no
gain over a file beside it. `Syn/declined.json`, and still not a dotfile — a
decline is the user's judgement about themselves and should hold on every
machine they use.

**The eval's two routes became one.** The model half tested pinned memories
through the prompt and unpinned ones through `recall`. After §1 that is not two
journeys, and keeping the split would have measured a world that no longer
exists.

### One defect this work introduced and caught

§8 stopped rendering `[instruction]` on instruction lines. `syn_memory_budget`
counted memories by counting lines starting with `- [`, so from that moment it
undercounted every instruction and reported memories as dropped that were not.
Nothing failed; the number just drifted. The count now lives in
`memory::lines_shown`, beside the code that decides what a line looks like.

## What a third run showed, and the claim it takes back

The twenty cases were run a third time against the finished implementation.
Fifteen better, one **worse**, two with no signal, both controls clean.

The one worse is *"Đặt bàn tối thứ Bảy cho 4 người"*, and it is worth reading
carefully:

- Without memory, the model worked out that today is 04/09/2026 and the coming
  Saturday is 05/09/2026, and asked which restaurant and what time.
- With memory, it asked which restaurant, what time, and for a booking name —
  **and mentioned neither the seafood allergy nor the date**.

So on this run `with` is behind on a case where `with` was ahead an hour
earlier. That case has now come out fail, then win, then worse across three
runs, and nothing about it changed between the second and third — the prompt a
pinned memory produces is the same before and after §1, because §1 only changes
which memories are in it.

**That is variance, and it means "all three failures flipped" claimed more than
one run can support.** Two of those three — the seafood allergy and the
vegetarian lunch — turn on whether a single clause appears in a clarifying
question, which is exactly the kind of thing a model at temperature does
sometimes. The third, the team note, is a whole reply changing language, and it
has now come out Vietnamese twice with §8 and English once without; that one is
categorical, large, and flipped in the direction this document predicted in
advance, which is the most that can be said of it.

The eval as it stands cannot settle a marginal case. One run per configuration,
at the temperature the user's own settings carry, is enough for a change that
moves a whole reply and not enough for a change that moves a clause. Fixing it
does not need a scorer — the 2026-09-03 ADR is emphatic that a confidently wrong
scorer is worse than none — it needs the same twenty cases run several times per
configuration.

**That is now what it does.** Each arm of each case runs three times by default
(`SYN_EVAL_RUNS` overrides), and every answer is printed. After each arm the
eval prints a stability line — how many of the runs were distinct, and their
length range — which says how much the arm moved on its own and deliberately
says nothing about whether any of it was good. The closing note now tells the
reader to judge a case only where the runs within an arm agree, because a case
whose own arm disagrees with itself is reporting the model's temperature rather
than anything about memory.

It cost three times as much and it caught something no single run could. Nothing
here scores an answer; repetition just stops a coin toss from reading as a
result.

The two no-signal cases changed shape rather than resolving. The caption case
finally produced a caption, and it had no emoji in it — but the run without
memory declined to write one at all, so there is still nothing to compare. It
needs the rewrite it was already down for.

## What three runs per arm actually settled

120 calls, three runs of each arm of each case. It changed two conclusions, one
in each direction.

**"One case worse" was wrong.** Read across three runs, the restaurant booking
is *no change*, not worse. The single run that made it look worse was one where
the memory-free arm happened to work out the date correctly; across three runs
that arm gets the date right once, wrong once, and does not attempt it once.
Nothing was worse with memory in any case. **The gate's "worse on none" holds,
and with twelve or more cases better, gate criterion 1 is met.**

**But the seafood case is a real defect, not noise.** Its `with` arm returned
the *identical* answer three times — *"Bạn muốn đặt ở nhà hàng nào và lúc mấy
giờ tối thứ Bảy (12/09/2026)?"* — and none of the three mentions the allergy. A
pinned memory, in the prompt, consistently ignored. That is the opposite of a
coin toss and it cannot be explained away.

Worth noticing about that case: the memory-free arm varied across all three runs
and the memory arm did not move at all. The block may be narrowing answers as
well as informing them, and here it narrowed to one that drops the thing it was
supposed to carry.

A hypothesis this eval cannot yet test, written down so it can be wrong: §8
elevated `instruction` and left `fact` as a labelled bullet. *"Vợ bị dị ứng hải
sản nặng"* is a `fact`, and it is a fact about somebody else. The knee injury —
also a `fact`, but about the user, and answering a question that is directly
about the user's body — works every time. If facts about third parties are the
weak class, the fix is not more prompt text but the `subject` field earning its
keep in the rendering.

**The stability lines earned their cost immediately.** They are what separated
"this case is noisy" from "this case is reliably wrong", and those two need
opposite responses. Two other cases moved the other way: the vegetarian lunch
mentions the constraint in two runs of three, so it is *usually* better rather
than reliably so; and the caption case wrote a caption exactly once out of three
and asked for product details the other twice, which confirms it is a broken
case rather than a marginal one.
