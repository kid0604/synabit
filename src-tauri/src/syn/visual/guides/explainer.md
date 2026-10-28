# An explainer

A concept the person can move: a formula with a slider for each quantity, and
a picture or a curve that follows. The explanation is in your answer; the
visual is where they try it.

- One to three controls, through `synabit.controls`, with a sensible `min`,
  `max` and `step`, and a unit.
- Show the formula with KaTeX (`katex.render(tex, element)`) and the value it
  gives, updated as the sliders move.
- Start at values that show the interesting case, not at zero.
- If there is a "what if" worth asking, one button with `synabit.ask(...)`.
- Keep the arithmetic in one function the controls call; redraw everything
  from it on `input`.
