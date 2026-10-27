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
- Controls for the two or three parameters that matter, through
  `synabit.number` beside them. Changing one resets the run.
- Keep the state in plain variables and one `step(dt)` function; draw from the
  state, never the other way round.
- Canvas or SVG both work. On a canvas, read colours with `synabit.color()`
  and redraw on `synabit.onContext`.
