# ADR — how we find out whether anyone climbs the slope, 2026-08-29

**Status:** accepted
**Decides:** what P2 ships alongside pinned filters, and what the gate before
P4 reads.

## The problem

The roadmap has a gate before the schema/manifest work: *does anyone actually
use the thing we just built?* If nobody pins a saved filter to the sidebar,
nobody is going to write an app manifest either, and the several weeks after
that gate would be spent on a feature with no audience. Ink & Switch say as
much about their own prototypes — the Request Tracker in Patchwork "didn't end
up getting used very much."

So the gate needs a number. And Synabit has no way to get one, on purpose:

> Zero telemetry, no forced cloud account, no vendor lock-in.

That is not an oversight to route around. It is load-bearing — it is why
somebody chooses this over the alternatives, and the whole security posture of
the product is built to make it true (no analytics, no crash reporting, no
phone-home; verified by grep, not by intent).

Two real things are in tension, and the resolution has to keep both.

## Decision

**Count locally, and show the count to the user.** Nothing is transmitted.

Settings grows one line under the vault section, along the lines of:

> You have 3 saved filters. 1 is pinned to the sidebar.

That is the entire mechanism.

## Why this one

**It does not need permission, because nothing leaves.** No consent dialog, no
privacy policy change, no collection infrastructure, no promise renegotiated.
The number is computed from nodes already in the vault — `getNodeSummaries('filter')`
returns them today — and rendered.

**The measurement is also the feature.** A user who reads "You have 3 saved
filters. 0 are pinned" has just been told that pinning exists. The slope's real
problem is not that the rungs are missing — rungs 2, 3 and 4 have shipped
already — it is that nothing tells anyone they are standing on one. A line in
Settings is a small piece of that telling, and it costs the same as the
measurement would have anyway.

**The number reaches us the honest way.** Through support threads, beta
conversations and bug reports, where a person says "I've got eleven of these"
or asks what the line means. That is a slow, biased channel. It is also the
only one this product is allowed to have, and it carries something a counter
never does: *why*.

## What was rejected, and what would reopen it

**A one-time prompt after P2** ("Do you use pinned filters? Yes / No / What are
those?"). The third answer is the most valuable data in this whole exercise,
and it is the one a local counter cannot produce. Rejected because collecting
the reply means building a transmission path — which is telemetry with a
consent screen in front of it, and the first exception to a promise is the
expensive one.

**Opt-in analytics, default off.** Gives the truest numbers. Rejected for
what it costs around the number: a collection endpoint to run, a privacy policy
to write and keep accurate, a data-retention answer, and a sales page that has
to explain a nuance instead of stating a fact.

**Counting only in our own and beta testers' vaults.** Not rejected — this
happens regardless, and it is where the qualitative signal comes from. It is
just not sufficient on its own, because the people who agreed to beta test a
malleable-software feature are the people most likely to use it.

**What would reopen this:** the gate before P4 arriving with genuinely no
signal — no support threads either way, no beta feedback, nothing. At that
point the choice is between shipping P4 blind and asking users a question, and
asking is the lesser harm. Reopen it then, not before.

## Consequences

- P2 ships a Settings line, not a metrics pipeline. Roughly an afternoon.
- The gate before P4 is read from conversations, not a dashboard. It will be
  qualitative, and the criterion should be written as such: *have real users,
  unprompted, talked about pinned filters?*
- The counter must never be the reason a node is loaded. If showing it means
  scanning the vault on every Settings open, it is not worth having.
