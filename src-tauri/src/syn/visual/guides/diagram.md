# A diagram

Boxes, arrows, a structure, a cycle: an inline `<svg>` with a `viewBox`,
drawn by hand. For a plain flowchart or a tree the person may want to edit,
`draw_board` is better — a visual is for what a board cannot show: something
that highlights as you hover, steps that play in order, a part that moves.

- Nodes and edges — a network, a state machine, a tree of choices — are
  `synabit.graph`: it places them, keeps weights off the lines and names the
  states in a legend. Draw by hand only what is not a graph.
- Lay it out on a grid of 8: boxes `rx="8"`, at least 16 apart, labels centred.
- A box is as wide as its longest label plus 24. SVG does not wrap: break a
  long label into two `<tspan>` lines yourself.
- Arrows start and end on a box edge and never cross a label. One
  `<marker>` in `<defs>`, used by every arrow.
- Strokes `var(--border)` or `var(--muted)`, 1 to 1.5 wide; text
  `var(--text)`; one accent box at most, `var(--accent)` stroke with the same
  colour at low `fill-opacity`.
- The title of the diagram is the point it makes, not its topic.
