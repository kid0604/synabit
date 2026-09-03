# ADR — what to do about pre-fetched retrieval, 2026-09-03

**Status:** accepted
**Decides:** whether Syn keeps stuffing retrieved context into the system
prompt, and what the prompt budget is set from.
**Continues** the question `syn/rag.rs` asks of itself: *does this pipeline
still earn its 1,777 lines?*

> **This document was rewritten after the measurement it was waiting for.** The
> first version predicted that stuffing should be reduced, on two grounds: that
> it costs a large share of the context window, and that it hands the model
> misleading context. Both grounds turned out to be wrong when measured. The
> prediction was written down so that it could be wrong, and it was.

## The problem

Syn stuffs retrieved vault context into the system prompt before every message.
That pipeline was written when the assistant could not go and look for itself.
It can now: `query_nodes` reads the same index the app's search bar reads,
`list_schemas` describes the vault, and the context window belongs to whichever
model the user configured.

So the question is whether *pre-fetched* retrieval adds anything on top of
tools, and what it costs when it does not.

## What retrieval finds, with no model involved

Five questions against a seeded vault
(`cargo test --lib what_retrieval_finds_for_each_question -- --nocapture`):

| question | hit | miss | misled | best score |
| --- | ---: | ---: | ---: | ---: |
| What is the wifi password in the Hanoi office? | 1 | 0 | 0 | 11.65 |
| How many tasks are not done? | – | – | 0 | 0.46 |
| Which book did I rate highest? | 0 | 1 | **1** | 1.71 |
| What did I decide about pricing, and who disagreed? | 2 | 0 | 0 | 4.14 |
| Do I have any notes about the Ha Long trip? | – | – | **1** | 1.63 |

**3 of 4 retrievable facts found; 2 of 5 questions handed context that points
the wrong way.** The old measurement counted only how many questions came back
empty — on which the pipeline reads as fixed, because nothing comes back empty
any more. "Not empty" and "right" are different claims.

- *Which book did I rate highest* retrieves **"Book the venue"**, a task whose
  title starts with the word. The real books are unreachable: their bodies are
  empty and the answer lives in a `rating` field.
- *Ha Long trip* retrieves the **Hanoi office** note, because FTS5 tokenises
  `ha-noi-2026` into `ha`/`noi`/`2026`. The vault has no Ha Long note.

### Why no retrieval filter was tuned

Two instruments were tried and both were rejected *by* the numbers.

**An absolute score floor.** Correct hits score 11.65, 4.14, 2.87 and — for a
one-word question — 1.34. The wrong ones score 1.71 and 1.63. Any floor that
keeps 1.34 keeps 1.71. This is the same instrument that was already removed for
failing in the other direction.

**A coverage floor**, on the idea that right answers match several of the
question's words. FTS5's `snippet()` marks matched terms, so the count is free
(`cargo test --lib how_much_of_the_question_each_hit_matched -- --nocapture`):

| hit | terms | score | matched | verdict |
| --- | ---: | ---: | ---: | --- |
| Hanoi office (wifi) | 4 | 11.65 | 4 | right |
| Pricing pushback | 3 | 4.14 | 2 | right |
| **Pricing decision** | 3 | 2.87 | **1** | **right** |
| Book the venue | 3 | 1.71 | 1 | wrong |
| Hanoi office (Ha Long) | 4 | 1.63 | 1 | wrong |

"Pricing decision" is the note containing "per-seat" and matched one term, the
same as both wrong answers. A two-term floor removes two wrong hits by removing
a right one.

## What the model does with it

`gpt-5.6-luna` via the OpenAI-compatible provider, five questions, three trials
each, both arms keeping every tool and differing only in the system prompt
(`SYN_EVAL_TRIALS=3 cargo test --lib rag_vs_agentic -- --ignored --nocapture`):

| arm | correct | tool calls | wall clock | system prompt | runs cut short |
| --- | ---: | ---: | ---: | ---: | ---: |
| stuffed | **15/15** | 13 | 36.7s | 88,938 ch (5,929 avg) | 0 |
| agentic | **15/15** | 30 | 52.1s | 78,075 ch (5,205 avg) | 0 |

**Accuracy is a tie. Cost is not.** Stuffing spends 724 characters of prompt per
question — about 180 tokens — and buys back 1.1 tool calls and roughly a second
per question. A tool call is a whole extra round trip whose result is itself
several hundred tokens, so the trade is strongly in stuffing's favour.

Both of the retrievals that point the wrong way were **recovered from, in every
trial**. Asked which book scored highest, the model was handed "Book the venue"
and went and queried the books anyway. Asked about Ha Long, it was handed the
Hanoi note and still answered that there was nothing. The prompt tells it the
context is a sample it may need to search past, and on this model it obeys.

### The same measurement on a local model

`gemma4:e4b` (8B) through Ollama, same five questions, three trials, same
settings but for provider and model:

| arm | correct | tool calls | wall clock | runs cut short |
| --- | ---: | ---: | ---: | ---: |
| stuffed | 13/15 | 21 | 324.5s | 0 |
| agentic | 12/15 | 24 | 275.3s | 0 |

**Accuracy is a tie here too**, and a point apart in each direction across two
runs of the same build — which is the noise floor, not a result. What is not
noise is the gap to the hosted model: **7.6× slower** and three points down.

The three points are not spread out. **Four of the five failures are the same
question** — which book was rated highest — and every one has the same shape:

```
stuffed  FAIL   1 call(s)  2 round(s)   missing ["Sapiens", "5"]
   → I couldn't find any books in your vault, so I can't tell you which one
     you rated highest.
```

One search, no result, give up. The answer lives in a `rating` frontmatter
field and needs `type:book sort:-rating`; the model made one text search and
stopped. It did this **with the answer's neighbourhood already in its context**
in the stuffed arm, so this is not a retrieval failure. `gpt-5.6-luna` answered
the same question 3/3 in both arms, using two to three calls.

That is the capability line between the tiers, and it is narrower than "big
model good": not language quality, not knowledge, but **whether a model composes
a second, structured query after the first one comes back empty.**

### A prompt fix for it, proposed and rejected

The obvious response is to say it in the prompt, so it was written, measured and
thrown away rather than shipped on the strength of sounding right:

> An empty result is not an answer. If a search returns nothing, try again with
> fewer or different words, or call `list_schemas` to see what this vault calls
> things, before telling the user there is nothing there.

| | correct | tool calls | system prompt |
| --- | ---: | ---: | ---: |
| before | 13/15 · 12/15 | 21 · 24 | 88,938 ch |
| after | 13/15 · 12/15 | 24 · 27 | 92,148 ch |

Identical scores. The same four failures on the same question. And on that
question, before and after, every failing run reads `1 call(s) 2 round(s)` —
byte for byte the same shape. The model read the instruction, searched slightly
more elsewhere (+3 calls per arm), and on the one case the sentence was written
for it changed nothing at all.

**Reverted.** The problem is not that the instruction was unclear; it is that
this model does not act on "try again" instructions. More prompt does not fix a
model that is not following the prompt, and 214 characters on every conversation
forever is not worth an improvement that cannot be measured.

The snapshot test caught the change, the re-blessing was deliberate, and the
revert restored the snapshots byte for byte. That machinery working is the
reason this could be tried at all.

### Two corrections to the first draft of this document

**The window cost was overstated by an order of magnitude.** The first version
said the prompt reaches 17,190 characters and eats more than half of an
8,192-token window. That is the *cap* — `max_tool_iterations`-style arithmetic
on `max_context_chars` — not the measurement. Retrieval actually contributed
between 0 and 941 characters per question. The cap only binds on a vault with
much more to find; see *What this does not measure*.

**"Misleading context" was a real defect of retrieval and not a real cost to the
user**, at least on this model. It is still worth fixing, and it is still the
reason `a_question_about_something_absent_retrieves_nothing` is written down as
a rule. It is not a reason to remove stuffing.

## The bug this exercise found

The agentic arm failed one question on the first run: asked what was decided
about pricing and who disagreed, it searched twice and answered *"I couldn't
find any vault notes mentioning pricing"* — from a vault holding two notes that
are entirely about it.

The cause was not the model. `retrieve_context` sets `match_any`, so retrieval
matches **any** of a question's words; the doc comment on that line names this
exact question as the reason it was changed. `tool_query_nodes` called
`parse_query` and did not, so **the assistant searched with `AND` while the
pipeline beside it searched with `OR`** — the fix had been applied to one half
of the app and never to the other.

Measured, offline and free
(`cargo test --lib the_assistants_own_search -- --nocapture`):

| query the model plausibly writes | matched, before | after |
| --- | ---: | ---: |
| `pricing` | 2 | 2 |
| `pricing decision` | 1 | 1 |
| `pricing disagreed` | 1 | 1 |
| `decide pricing disagreed` | **0** | 2 |
| `pricing decision disagreed` | **0** | 2 |

`query_nodes` now **falls back** rather than switching: a query that matched
something is answered exactly as before, and only a query that matched nothing
is asked a second, looser way. The two failures are not symmetric — too many
results is visible and recoverable, because the model reads `total_matches` and
narrows, while zero results reads as an empty vault and there is nothing to
narrow. Widened results carry `matched_any_word: true` and a note saying so, so
the model is not handed a loose match dressed as an exact one.

With that fix, the agentic arm went from 4/5 to 15/15.

**The fallback has a cost, and it was measured rather than assumed**
(`cargo test --lib what_the_widening_costs -- --nocapture`): `ha long trip`
widens to the Hanoi note, importing retrieval's manufactured-evidence problem
into the tool path. It did not change an answer in any of the three trials —
the model read the flag and still said no — but it is recorded, not waved away.
A related interaction: a bare `notes` or `tasks` matches every node of that type,
because the type name is in the index, so a widened "notes about X" can sweep a
whole type. `filter_vault_terms` already solves this shape of problem for feeds
and finance and is the obvious place to start if it ever bites.

## Decision

**1. Stuffing stays.** It is accuracy-neutral and cost-negative on the evidence
there is. Removing it would have cost 17 extra tool calls across 15 questions to
buy back 180 tokens each.

**2. No retrieval filter is tuned.** Five questions from one seeded vault is not
enough to fit a threshold to, and this pipeline has already been burned once by
a threshold chosen that way.

**3. The remaining retrieval defect is recorded as the rule it breaks.**
`a_question_about_something_absent_retrieves_nothing` is `#[ignore]`d with a
reason pointing here.

**4. `query_nodes` falls back to matching any word when matching all of them
found nothing.** This is the one behaviour change, and it is the finding the
whole exercise paid for.

**5. The prompt budget is derived from measurement, in named parts.**
`DEFAULT_BUDGET_CHARS` was a round 64,000 that could never bind. It is now
`FIXED_SECTIONS_CHARS + DEFAULT_CONTEXT_CHARS + HEADROOM_CHARS`, with
`the_fixed_sections_still_cost_what_the_budget_assumes` failing if the premise
drifts.

## What this does not measure

**The seeded vault has fourteen nodes.** That is why retrieval contributed a few
hundred characters rather than the 12,000 it is allowed. On a vault with
thousands of notes the stuffed arm's prompt would be far larger, and the cost
comparison could invert. **This is the single biggest limitation of the result
above**, and the decision should be revisited against a realistically sized
vault before it is treated as settled.

**Both arms scored 15/15**, so the question set no longer separates them. It has
a ceiling effect and needs harder questions — ones with several hops, or
contradictory notes — before another A/B is worth paying for.

**Two models, and only one of them is local.** `gemma4:e4b` was measured and is
reported above. `qwen3:14b` was started and abandoned — it was running an older
build, and finishing it would have produced numbers that could not be compared
with the rest. It is the obvious next measurement and costs nothing but time.

**The eval was wrong three times before it was right.** Every one of them
produced a plausible number:

1. The offline measure counted how many questions came back *empty*, on which
   the pipeline looked fixed — while two of five were being handed context that
   pointed the wrong way.
2. The honest-no question asked for the substrings `no` and `not`. Its own text
   is "Do I have any **notes**…", and `notes` contains both, so any reply that
   used the word at all scored correct whatever it went on to claim.
3. Those substrings are English. A correct Vietnamese answer — *"Tôi không tìm
   thấy ghi chú nào…"* — was recorded as the model failing, in an app whose
   assistant answers in the language it is asked in.
4. The marker list then held `n't`, and the model writes `couldn't` with U+2019.
   Three more correct answers were scored as failures.

Each was found by reading a reply that had been marked wrong and disagreeing
with the mark. None would have been found by reading the totals. The scorer now
normalises punctuation, takes bilingual alternatives, and has tests carrying the
exact strings that fooled it.

**The round ceiling never bound.** `max_tool_iterations` is 5 in the vault these
settings came from; the deepest run used 4 rounds. Worth re-checking on harder
questions, because the agentic arm spends rounds the stuffed arm does not, and a
ceiling set for one arm penalises the other.
