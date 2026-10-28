# A chart

Numbers the person's data holds, drawn with d3 into an `<svg>`. The data comes
from a tool result in this conversation — never estimated, never invented —
written into the visual as a literal array.

- Draw it with `synabit.chart`: it sets the axes, ticks, margins and legend.
  Pick the form by what the reader decides: compare amounts → bars from zero
  (`x: { categories }`); change over time → a line; parts of a whole → a
  stacked bar drawn by hand, rarely a pie; one value → `synabit.stats`, not a
  chart.
- One series in `c1`; several in `c1`, `c2`, … in order. If one series is the
  point, it gets `c1` and the rest `muted`.
- Two measures — an amount and a rate, a balance and a payment — are two
  `panels`, never two series on one axis.
- Label the few values the point rests on, as `marks`, not all of them. Units
  in the axis (`unit`). Dates and month names in the person's language as
  `categories`: `new Date(d).toLocaleDateString(synabit.locale, …)`.
- Sum or average in your answer with the tool that computed it (`crunch`,
  `compute`), not in the chart's script, unless the chart needs it to draw.
