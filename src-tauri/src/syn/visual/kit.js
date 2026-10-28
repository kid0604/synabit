// The kit every Syn visual is given: the bridge to the app around it.
//
// A visual runs in a sandboxed frame on `synvisual://` with no network, so
// the only way anything leaves it is `postMessage` to the frame's parent.
// What it says there is JSON-RPC 2.0 with the message names of the MCP Apps
// standard (SEP-1865) — `ui/initialize`, `ui/notifications/size-changed`,
// `ui/notifications/host-context-changed`, `ui/message` — so the host that
// shows these can one day show an MCP App's view as well. Two are Synabit's
// own: `synabit/error`, what broke inside the frame; `synabit/selftest`,
// which only the step-0 page sends; `synabit/lint`, what `synabit.lint()`
// found wrong with the visual as drawn.
//
// The host is the app: it answers `ui/initialize` with the theme and the
// locale, resizes the frame to the height reported here, and puts a question
// sent with `synabit.ask` in the chat box for the person to send — it never
// sends one itself (decided 2026-10-26, plan §7).
(() => {
  'use strict';
  const html = document.documentElement;
  const listeners = [];
  const pending = new Map();
  let nextId = 1;
  let context = { theme: html.dataset.theme === 'dark' ? 'dark' : 'light', locale: html.lang || 'vi' };

  // A sandboxed frame's origin is opaque, so `*` is the only target there
  // is; the message still goes to one window, the parent.
  const send = (message) => parent.postMessage(Object.assign({ jsonrpc: '2.0' }, message), '*');
  const notify = (method, params) => send({ method, params: params || {} });
  const request = (method, params) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      send({ id, method, params: params || {} });
    });

  const apply = (next) => {
    if (next && (next.theme === 'dark' || next.theme === 'light')) {
      context.theme = next.theme;
      html.dataset.theme = next.theme;
    }
    if (next && typeof next.locale === 'string' && next.locale) context.locale = next.locale;
    listeners.forEach((listener) => {
      try {
        listener(Object.assign({}, context));
      } catch (error) {
        fail('error', error && error.message, 'a theme listener');
      }
    });
  };

  window.addEventListener('message', (event) => {
    if (event.source !== parent) return;
    const message = event.data;
    if (!message || message.jsonrpc !== '2.0') return;
    if (message.id != null && pending.has(message.id)) {
      const waiting = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) waiting.reject(message.error);
      else waiting.resolve(message.result);
      return;
    }
    if (message.method === 'ui/notifications/host-context-changed') apply(message.params);
  });

  // What broke, for the host to show beside the frame — a visual that throws
  // otherwise just sits there blank.
  const errors = [];
  const fail = (kind, message, where) => {
    const error = { kind, message: String(message || '').slice(0, 500), where: String(where || '') };
    if (errors.length < 20) errors.push(error);
    notify('synabit/error', error);
  };
  window.addEventListener('error', (event) =>
    fail('error', event.message, event.lineno ? `${event.lineno}:${event.colno}` : ''),
  );
  window.addEventListener('unhandledrejection', (event) =>
    fail('rejection', (event.reason && event.reason.message) || event.reason),
  );
  document.addEventListener('securitypolicyviolation', (event) =>
    fail('policy', `${event.effectiveDirective} ${event.blockedURI}`),
  );

  // The frame is as tall as what is in it; the host caps it.
  let sized = 0;
  const reportHeight = () => {
    const height = Math.ceil(Math.max(document.body ? document.body.scrollHeight : 0, html.scrollHeight));
    if (height && height !== sized) {
      sized = height;
      notify('ui/notifications/size-changed', { height });
    }
  };
  document.addEventListener('DOMContentLoaded', () => {
    new ResizeObserver(reportHeight).observe(document.body);
    reportHeight();
  });

  // Opened on its own — a visual saved as a file and opened in a browser —
  // there is no host to answer: the page's own theme and locale stand.
  const framed = window.parent !== window;
  const ready = !framed ? Promise.resolve(Object.assign({}, context)) : request('ui/initialize', {
    protocolVersion: '2026-01-26',
    appInfo: { name: 'synabit-visual', version: '1' },
    appCapabilities: {},
  })
    .then((result) => {
      if (result && result.hostContext) apply(result.hostContext);
      notify('ui/notifications/initialized');
      reportHeight();
      return Object.assign({}, context);
    })
    .catch(() => Object.assign({}, context));

  // ── What a visual says about itself, and what is looked at in it ──
  //
  // A model writes a visual and never sees it drawn. `expect` lets the visual
  // claim something checkable — a derived number substituted back into the
  // equation it came from; `report`, the numbers it shows. `lint` looks at
  // what was drawn for faults that need no question to see: the ones the
  // visuals of 2026-10-27/28 had (`docs/syn-visuals-quality-plan-2026-10-28.md`
  // §2.3). Each check has a page in `e2e/visual-lint/` built from a visual
  // that had the fault.
  const expects = new Map();
  const reported = {};

  const frame = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const settle = (ms) => frame().then(() => new Promise((resolve) => setTimeout(resolve, ms)));
  const textOf = (node) => (node.textContent || '').replace(/\s+/g, ' ').trim();
  const quote = (text) => `“${String(text).slice(0, 40)}”`;
  // Text a person reads: not a script, not a style, not KaTeX's own markup.
  const shownText = () => {
    const out = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const parent = node.parentElement;
      if (!parent || parent.closest('script, style, .katex')) continue;
      const text = node.textContent.trim();
      if (text) out.push(text);
    }
    return out;
  };
  const area = (r) => Math.max(0, r.width) * Math.max(0, r.height);
  const overlapArea = (a, b) =>
    Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left)) *
    Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
  // How many points a path or polyline is drawn through.
  const vertices = (element) =>
    element.tagName === 'path'
      ? ((element.getAttribute('d') || '').match(/[MLHVCSQTA]/gi) || []).length
      : ((element.getAttribute('points') || '').trim().match(/-?[\d.]+[ ,]+-?[\d.]+/g) || []).length;
  // A stroke wide enough to be data or an edge, not a gridline or an axis.
  const stroked = (element) => {
    const style = getComputedStyle(element);
    return style.stroke && style.stroke !== 'none' && parseFloat(style.strokeWidth) >= 1.5;
  };
  // Whether a stroke shows at a point, or something painted after it — a
  // label's background box — hides it there.
  const covers = (element) => {
    if (element instanceof SVGTextElement || element.closest('text')) return false;
    const style = getComputedStyle(element);
    if (style.opacity === '0' || style.visibility === 'hidden') return false;
    if (element instanceof SVGElement) return style.fill !== 'none' && style.fillOpacity !== '0';
    return style.backgroundColor !== 'rgba(0, 0, 0, 0)' && style.backgroundColor !== 'transparent';
  };
  const seen = (element, p) => {
    const stack = document.elementsFromPoint(p.x, p.y);
    const at = stack.indexOf(element);
    return at >= 0 && !stack.slice(0, at).some(covers);
  };
  const NOT_A_COLOUR = /^(none|transparent|currentcolor|inherit|initial|unset)$/i;
  const LITERAL_COLOUR = /^(#[0-9a-f]{3,8}|rgba?\(.*\)|hsla?\(.*\)|[a-z]+)$/i;

  const checks = {
    // Nothing on the page has a size: no markup, or a script that stopped
    // before drawing into it. One page kept on 2026-10-28 was "[]".
    empty: () => {
      const drawn = [...document.body.querySelectorAll('*')].some((element) => {
        if (element.closest('script, style, link, template')) return false;
        const box = element.getBoundingClientRect();
        return box.width > 0 && box.height > 0;
      });
      return drawn ? [] : ['nothing on the page has a size: there is no visual'];
    },

    // Something threw.
    error: () => errors.map((e) => `${e.kind}: ${e.message}${e.where ? ` (${e.where})` : ''}`),

    // NaN or Infinity reached an attribute or the screen.
    nan: () => {
      const found = [];
      for (const element of document.body.querySelectorAll('*')) {
        if (element.closest('script, style')) continue;
        for (const attribute of element.attributes) {
          if (/\bNaN\b|Infinity/.test(attribute.value)) {
            found.push(`<${element.tagName.toLowerCase()} ${attribute.name}="${attribute.value.slice(0, 40)}">`);
          }
        }
      }
      for (const text of shownText()) if (/\bNaN\b|Infinity/.test(text)) found.push(quote(text));
      return found.slice(0, 5);
    },

    // Float noise on screen: a number written without synabit.number.
    'long-decimal': () =>
      shownText()
        .filter((text) => /\d[.,]\d{5,}/.test(text))
        .map((text) => `${quote(text)}: write numbers through synabit.number`)
        .slice(0, 5),

    // Mathematics left as its source: nothing rendered it.
    'raw-math': () =>
      shownText()
        .filter((text) => /\$[^\s$][^$]*\$|\\[()[\]]|\\(frac|sqrt|sum|int|pi|alpha|theta|Delta|cdot|times)\b/.test(text))
        .map((text) => `${quote(text)}: render it with katex.render, or write it as plain text`)
        .slice(0, 5),

    // Every line in a plot lies flat: the data was drawn against a scale it
    // does not fit — millions against an axis in đồng.
    'flat-plot': () => {
      const found = [];
      for (const svg of document.querySelectorAll('svg')) {
        const lines = [...svg.querySelectorAll('path, polyline')].filter((element) => vertices(element) >= 8);
        if (!lines.length) continue;
        const flat = lines.every((element) => {
          const box = element.getBBox();
          return box.height < 0.5 && box.width > 20;
        });
        if (flat) {
          found.push(`${quote(svg.getAttribute('aria-label') || 'a plot')}: all ${lines.length} of its lines are flat — check the scale each is drawn against`);
        }
      }
      return found;
    },

    // Text cut off at the edge of its drawing, or the frame scrolling sideways.
    overflow: () => {
      const found = [];
      for (const svg of document.querySelectorAll('svg')) {
        const bounds = svg.getBoundingClientRect();
        for (const text of svg.querySelectorAll('text')) {
          const r = text.getBoundingClientRect();
          if (!area(r)) continue;
          if (r.left < bounds.left - 1 || r.right > bounds.right + 1 || r.top < bounds.top - 1 || r.bottom > bounds.bottom + 1) {
            found.push(`${quote(textOf(text))} runs past the edge of its drawing`);
          }
        }
      }
      if (html.scrollWidth > html.clientWidth + 1) {
        found.push(`the visual is ${html.scrollWidth}px wide in a ${html.clientWidth}px frame`);
      }
      return found.slice(0, 5);
    },

    // Labels drawn over each other, or a line drawn through one.
    overlap: () => {
      const found = [];
      for (const svg of document.querySelectorAll('svg')) {
        const texts = [...svg.querySelectorAll('text')].filter((t) => textOf(t) && area(t.getBoundingClientRect()));
        const rects = texts.map((t) => t.getBoundingClientRect());
        for (let i = 0; i < texts.length; i++) {
          for (let j = i + 1; j < texts.length; j++) {
            if (texts[i].contains(texts[j]) || texts[j].contains(texts[i])) continue;
            if (overlapArea(rects[i], rects[j]) > 0.25 * Math.min(area(rects[i]), area(rects[j]))) {
              found.push(`${quote(textOf(texts[i]))} and ${quote(textOf(texts[j]))} are drawn over each other`);
            }
          }
        }
        const strokes = [...svg.querySelectorAll('line, path, polyline')].filter(stroked);
        texts.forEach((text, i) => {
          // The glyphs, not the line box: a text's box runs a fifth of its
          // height above and below the letters, and a line passing there
          // passes beside the label, not through it.
          const box = rects[i];
          const r = { left: box.left, right: box.right, top: box.top + box.height * 0.22, bottom: box.bottom - box.height * 0.18 };
          const crossed = strokes.some((element) => {
            const matrix = element.getScreenCTM();
            if (!matrix || typeof element.getTotalLength !== 'function') return false;
            try {
              const length = element.getTotalLength();
              let inside = 0;
              for (let at = 0; at <= length; at += 2) {
                const p = element.getPointAtLength(at).matrixTransform(matrix);
                if (p.x > r.left + 1 && p.x < r.right - 1 && p.y > r.top + 1 && p.y < r.bottom - 1 && seen(element, p) && ++inside >= 2) return true;
              }
            } catch (_) {
              // A path drawn through NaN has no length to walk; `nan` says so.
            }
            return false;
          });
          if (crossed) found.push(`a line is drawn through ${quote(textOf(text))}`);
        });
      }
      return [...new Set(found)].slice(0, 8);
    },

    // A colour written into the drawing: it stays put when the theme changes.
    'fixed-colour': () => {
      const found = [];
      for (const element of document.querySelectorAll('svg *')) {
        for (const property of ['fill', 'stroke']) {
          const value = (element.getAttribute(property) || element.style[property] || '').trim();
          if (value && !NOT_A_COLOUR.test(value) && !/^(var|url)\(/i.test(value) && LITERAL_COLOUR.test(value)) {
            found.push(`<${element.tagName} ${property}="${value}">: use var(--…) so it follows the theme`);
          }
        }
      }
      return [...new Set(found)].slice(0, 5);
    },

    // What the visual claimed of itself and does not hold.
    expect: () =>
      [...expects.values()]
        .filter((e) => !e.ok)
        .map((e) => `${e.name}: ${JSON.stringify(e.actual)}, expected ${JSON.stringify(e.expected)}${e.tolerance != null ? ` ± ${e.tolerance}` : ''}`),
  };

  // Every control, used: one that changes nothing on screen is dead. A
  // control that did nothing is tried again after each of the others, so
  // "back" on the first step or "reset" at the start is not called dead.
  const signature = () =>
    document.body.innerHTML +
    [...document.querySelectorAll('canvas')]
      .map((canvas) => {
        try {
          return canvas.toDataURL();
        } catch (_) {
          return '';
        }
      })
      .join('');
  const controlsOf = () =>
    [...document.querySelectorAll('input, select, button')].filter(
      (control) => !control.disabled && control.type !== 'hidden' && control.getClientRects().length,
    );
  const use = (control) => {
    if (control.tagName === 'BUTTON' || ['checkbox', 'radio', 'button', 'submit'].includes(control.type)) {
      control.click();
      return true;
    }
    if (control.tagName === 'SELECT') {
      if (control.options.length < 2) return false;
      control.selectedIndex = (control.selectedIndex + 1) % control.options.length;
    } else if (control.type === 'range' || control.type === 'number') {
      const min = parseFloat(control.min || '0');
      const max = parseFloat(control.max || '100');
      const value = parseFloat(control.value);
      control.value = String(Math.abs(value - min) > Math.abs(max - value) ? min : max);
    } else {
      return false;
    }
    control.dispatchEvent(new Event('input', { bubbles: true }));
    control.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  };
  const middle = (control) => {
    const min = parseFloat(control.min || '0');
    const max = parseFloat(control.max || '100');
    const step = parseFloat(control.step) || 1;
    control.value = String(min + Math.round((max - min) / 2 / step) * step);
    control.dispatchEvent(new Event('input', { bubbles: true }));
    control.dispatchEvent(new Event('change', { bubbles: true }));
  };
  const changes = async (control) => {
    const before = signature();
    if (!use(control)) return true;
    await settle(120);
    return signature() !== before;
  };
  const nameOf = (control) =>
    quote(
      control.textContent.trim() ||
        control.getAttribute('aria-label') ||
        (control.labels && control.labels[0] && textOf(control.labels[0])) ||
        control.id ||
        control.type,
    );
  const deadControls = async () => {
    const controls = controlsOf();
    const dead = [];
    for (const control of controls) if (!(await changes(control))) dead.push(control);
    const alive = controls.filter((control) => !dead.includes(control));
    const still = [];
    for (const control of dead) {
      let moved = false;
      for (const other of alive) {
        // A slider moved to an end would leave a step button at the end it
        // cannot pass ("next" at the last month), every time; the middle
        // leaves room both ways.
        if (other.type === 'range' || other.type === 'number') middle(other);
        else use(other);
        await settle(60);
        if (await changes(control)) {
          moved = true;
          break;
        }
      }
      if (!moved) still.push(control);
    }
    return still.map((control) => `${nameOf(control)} changes nothing on screen`);
  };

  // A number the answer rests on (the brief's proof) that the visual does
  // not report, or reports as something else.
  const unproved = (proof) =>
    (Array.isArray(proof) ? proof : []).flatMap((p) => {
      if (!(p.name in reported)) return [`${p.name}: the visual does not report it (synabit.report)`];
      const shown = Number(reported[p.name]);
      const tolerance = p.tolerance != null ? p.tolerance : Math.max(Math.abs(p.value) * 0.005, 1e-9);
      return Math.abs(shown - Number(p.value)) <= tolerance ? [] : [`${p.name}: the visual reports ${shown}, worked out as ${p.value}`];
    });

  const lint = async (options) => {
    const opts = options || {};
    await ready;
    await settle(150);
    const problems = [];
    const add = (check, found) => found.forEach((detail) => problems.push({ check, detail: String(detail) }));
    const look = () => {
      for (const [check, find] of Object.entries(checks)) {
        try {
          add(check, find());
        } catch (error) {
          problems.push({ check: 'lint', detail: `${check} could not run: ${error && error.message}` });
        }
      }
    };
    look();
    add('proof', unproved(opts.proof));
    if (opts.controls) {
      add('dead-control', await deadControls());
      // Where the controls left it — a slider at its end, a step past the
      // last — is looked at too: NaN from a term of zero years shows there.
      look();
    }
    const seenOnce = new Set();
    const unique = problems.filter((p) => !seenOnce.has(p.check + p.detail) && seenOnce.add(p.check + p.detail));
    const result = { problems: unique, expects: [...expects.values()], reported: Object.assign({}, reported) };
    notify('synabit/lint', result);
    return result;
  };

  window.synabit = Object.freeze({
    /** Resolves once the host has said the theme and the locale. */
    ready,
    get theme() {
      return context.theme;
    },
    get locale() {
      return context.locale;
    },
    /** Called with `{ theme, locale }` whenever the host changes either. */
    onContext(listener) {
      if (typeof listener === 'function') listeners.push(listener);
    },
    /** A follow-up question, put in the chat box for the person to send. */
    ask(text) {
      return request('ui/message', { role: 'user', content: { type: 'text', text: String(text).slice(0, 2000) } });
    },
    /** A number as the person's locale writes it, rounded: float noise never reaches the screen. */
    number(value, digits) {
      const places = digits == null ? 2 : digits;
      // A drift of -0.00001 rounds to "-0", which reads as a sign that means something.
      const n = Number(value);
      const shown = Math.abs(n) < 0.5 * 10 ** -places ? 0 : n;
      return new Intl.NumberFormat(context.locale, { maximumFractionDigits: places }).format(shown);
    },
    /** A theme token's current value, for a canvas that cannot read CSS variables. */
    color(name) {
      return getComputedStyle(html).getPropertyValue(`--${name}`).trim();
    },
    /**
     * A claim the visual makes about itself, checked: a derived number put
     * back into the equation it came from. `tolerance` is absolute; without
     * one, numbers must agree to a part in a billion. Called again with the
     * same name, the last call stands, so it can sit inside a redraw.
     */
    expect(name, actual, expected, tolerance) {
      const key = String(name).slice(0, 80);
      let ok;
      if (typeof expected === 'number' || typeof actual === 'number') {
        const a = Number(actual);
        const e = Number(expected);
        const t = tolerance != null ? Number(tolerance) : 1e-9 * Math.max(1, Math.abs(e));
        ok = Number.isFinite(a) && Number.isFinite(e) && Math.abs(a - e) <= t;
      } else {
        ok = JSON.stringify(actual) === JSON.stringify(expected);
      }
      if (expects.size < 100 || expects.has(key)) {
        expects.set(key, { name: key, actual, expected, tolerance: tolerance == null ? null : tolerance, ok });
      }
      return ok;
    },
    /** The numbers this visual shows, by name: what an answer about it may quote. */
    report(values) {
      for (const [name, value] of Object.entries(values || {})) {
        const key = String(name).slice(0, 80);
        if (Object.keys(reported).length >= 50 && !(key in reported)) break;
        if (typeof value === 'number' || typeof value === 'string') reported[key] = value;
      }
    },
    /**
     * Looks at the visual as drawn and says what is wrong with it; also sent
     * to the host as `synabit/lint`. `{ controls: true }` uses every control
     * once, so only where nobody is looking: it moves them. `{ proof }` names
     * the numbers the visual must report, `[{ name, value, tolerance }]`.
     */
    lint,
  });
})();
