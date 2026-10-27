# A simulation

Something that moves by a model you write: a pendulum, an orbit, an epidemic,
a queue. A person tends to trust what a simulation shows over their own
reasoning, so a wrong one teaches the wrong thing. Make it right first.

- Say the model in the visual, in one line under it: the equations or the
  rule, and what it leaves out ("no air resistance", "small angles only").
- Step with a fixed time step and a method that holds: semi-implicit
  (symplectic) Euler or velocity Verlet for mechanics, RK4 for other ODEs.
  Never plain explicit Euler for anything that oscillates or orbits — it gains
  energy and drifts.
- Show a check the person can see: the energy for a mechanical system, the
  total population for an epidemic, the period against its formula. If it
  drifts, the step is too large.
- Draw with `requestAnimationFrame`, several physics steps per frame if
  needed. A play / pause button and a reset; start paused if the motion would
  be missed.
- Show the whole of what was asked. An orbit draws at least one full
  revolution; a trail that drops old points keeps more than one period of
  them (work the period out from the parameters, or keep every point). An
  oscillation shows several cycles; a curve over time runs until it settles.
- A reference drawn beside the result — the exact solution, the target shape,
  the curve the approximation tends to — is computed from the same definition
  and the same variables as the result, never drawn from coordinates written
  by hand. Before showing it, work out one point by hand and make sure the
  result and the reference agree with it and with the formula in your answer.
- A number read off the result — a peak, a maximum, an overshoot — is
  searched over the whole range drawn, not a part of it.
- Counts of people or things are whole on screen: `synabit.number(x, 0)`.
- Controls for the two or three parameters that matter, through
  `synabit.number` beside them. Changing one resets the run.
- Keep the state in plain variables and one `step(dt)` function; draw from the
  state, never the other way round.
- Canvas or SVG both work. On a canvas, read colours with `synabit.color()`
  and redraw on `synabit.onContext`.
