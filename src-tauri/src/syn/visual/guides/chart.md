# A chart

Numbers the person's data holds, drawn with d3 into an `<svg>`. The data comes
from a tool result in this conversation — never estimated, never invented —
written into the visual as a literal array.

- Pick the form by what the reader decides: compare amounts → bars from zero;
  change over time → a line; parts of a whole → a stacked bar, rarely a pie;
  one value → a large number (`.stat`), not a chart.
- One series in `var(--c1)`; several in `--c1`, `--c2`, … in order. If one
  series is the point, it gets `--c1` and the rest `var(--muted)`.
- Gridlines `var(--grid)`, axes `var(--muted)`, labels `var(--text-2)`, 12px.
- Label the few values the point rests on, not all of them. Units in the axis
  title. Every label through `synabit.number`. Dates and month names in the
  person's language: `new Date(d).toLocaleDateString(synabit.locale, …)`.
- A tooltip is a `<title>` inside the mark: the label, the value, the unit.
- Sum or average in your answer with the tool that computed it (`crunch`,
  `compute`), not in the chart's script, unless the chart needs it to draw.
