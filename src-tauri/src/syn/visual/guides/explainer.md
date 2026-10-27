# An explainer

A concept the person can move: a formula with a slider for each quantity, and
a picture or a curve that follows. The explanation is in your answer; the
visual is where they try it.

- One to three controls, each a `<label>` and an `input type="range"` with a
  sensible `min`, `max` and `step`, and the current value shown beside it
  through `synabit.number`.
- Show the formula with KaTeX (`katex.render(tex, element)`) and the value it
  gives, updated as the sliders move.
- Start at values that show the interesting case, not at zero.
- If there is a "what if" worth asking, one button with `synabit.ask(...)`.
- A reference beside the result — the exact curve an approximation tends to,
  the target shape — comes from the same definition and variables as the
  result, never from coordinates written by hand. Check one point by hand.
- Keep the arithmetic in one function the controls call; redraw everything
  from it on `input`.
