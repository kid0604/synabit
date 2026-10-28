# A simulation

Something that moves by a model you write: a pendulum, an orbit, an epidemic,
a queue. A person tends to trust what a simulation shows over their own
reasoning, so a wrong one teaches the wrong thing. Make it right first.

- Say the model in the visual, in one line under it: the equations or the
  rule, and what it leaves out ("no air resistance", "small angles only").
- Run it with `synabit.sim`: it steps with a fixed time step and a method
  that holds — velocity Verlet for mechanics (`method: 'verlet'`, `pairs`
  naming each position's velocity), RK4 otherwise — and gives run, pause and
  reset, and each invariant with its drift. Never step it by hand with plain
  Euler: anything that oscillates or orbits gains energy and drifts.

  ```js
  synabit.sim(stage, {
    state: { x: 1, y: 0, vx: 0, vy: 1 },
    rates: (s) => { const r3 = Math.hypot(s.x, s.y) ** 3; return { x: s.vx, y: s.vy, vx: -s.x / r3, vy: -s.y / r3 }; },
    method: 'verlet', pairs: { x: 'vx', y: 'vy' },
    dt: 0.001, speed: 1,
    invariants: { 'năng lượng': (s) => 0.5 * (s.vx ** 2 + s.vy ** 2) - 1 / Math.hypot(s.x, s.y) },
    draw: (s, t) => { /* draw the state */ },
  });
  ```

- A curve over time that does not need to move — a population, an epidemic
  — is `synabit.ode(...)` drawn with `synabit.chart`, not an animation.
- Show a check the person can see: the energy for a mechanical system, the
  total population for an epidemic, the period against its formula — as an
  invariant of `synabit.sim`. If it drifts, the step is too large.
- Start paused (`paused: true`) if the motion would be missed.
- Show the whole of what was asked. An orbit draws at least one full
  revolution; a trail that drops old points keeps more than one period of
  them (work the period out from the parameters, or keep every point). An
  oscillation shows several cycles; a curve over time runs until it settles.
- Before showing a reference beside the result, work out one point by hand
  and make sure the result and the reference agree with it and with the
  formula in your answer.
- A number read off the result — a peak, a maximum, an overshoot — is
  searched over the whole range drawn, not a part of it.
- Counts of people or things are whole on screen: `synabit.number(x, 0)`.
- Controls for the two or three parameters that matter, through
  `synabit.controls`; the quantity asked about and the check beside it
  through `synabit.stats`. Changing a control resets the run.
- Draw from the state `synabit.sim` passes to `draw`, never the other way
  round.
- Canvas or SVG both work. On a canvas, read colours with `synabit.color()`
  and redraw on `synabit.onContext`.
