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
      // A chart the kit drew took its scale from its data: a flat line there
      // is a value that does not change — a fixed monthly payment.
      for (const svg of document.querySelectorAll('svg:not([data-synabit="chart"])')) {
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

  // A drawing that moves when it is opened and stands still once a control
  // is used: the pendulum of 2026-10-28 kept moving the drawing its slider
  // had replaced. Only a page that moves pays for the look.
  const moving = async () => {
    const before = signature();
    await settle(200);
    return signature() !== before;
  };
  const frozen = async () => {
    if (!(await moving())) return [];
    const control = controlsOf().find((c) => c.type === 'range' || c.type === 'number' || c.tagName === 'SELECT');
    if (!control) return [];
    if (control.tagName === 'SELECT') use(control);
    else middle(control);
    await settle(60);
    if (await moving()) return [];
    return [`it moves until ${nameOf(control)} is changed, then stands still: draw each frame from the current values (synabit.sim keeps them)`];
  };

  // A number the answer rests on (the brief's proof) that the visual does
  // not report, or reports as something else.
  // A number the brief names that the visual reports under no such name is
  // a slip of bookkeeping, told apart from one it reports as something else.
  const unreported = (proof) =>
    (Array.isArray(proof) ? proof : [])
      .filter((p) => !(p.name in reported))
      .map((p) => `${p.name}: the visual does not report it (synabit.report, under this name)`);
  const unproved = (proof) =>
    (Array.isArray(proof) ? proof : []).flatMap((p) => {
      if (!(p.name in reported)) return [];
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
    add('unreported', unreported(opts.proof));
    if (opts.controls) {
      add('frozen', await frozen());
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

  // ── Components: the parts a visual is built from ──
  //
  // Syn's visuals broke their presentation every time, each time a new way:
  // two measures on one axis, a scale with no axis, a label cut at the edge, a
  // slider with no value, a large number that repeats the slider
  // (`docs/syn-visuals-quality-plan-2026-10-28.md` §2.4). Rules in a guide did
  // not hold; parts that cannot be put together wrong do. Each is called with
  // the element to draw into and draws it whole; called again, it draws it
  // again from what it is given, so a visual redraws by calling it on `input`.

  const formatNumber = (value, digits) => {
    const places = digits == null ? 2 : digits;
    // A drift of -0.00001 rounds to "-0", which reads as a sign that means something.
    const n = Number(value);
    const shown = Math.abs(n) < 0.5 * 10 ** -places ? 0 : n;
    return new Intl.NumberFormat(context.locale, { maximumFractionDigits: places }).format(shown);
  };

  const SVG = 'http://www.w3.org/2000/svg';
  const svgEl = (tag, attrs, parent, text) => {
    const e = document.createElementNS(SVG, tag);
    for (const k in attrs) if (attrs[k] != null) e.setAttribute(k, attrs[k]);
    // kit.css sets every `svg text` to the text colour, and a stylesheet wins
    // over an attribute: a text's own colour goes in its style.
    if (tag === 'text' && attrs && attrs.fill) e.style.fill = attrs.fill;
    if (text != null) e.textContent = text;
    if (parent) parent.appendChild(e);
    return e;
  };
  const htmlEl = (tag, style, parent, text) => {
    const e = document.createElement(tag);
    if (style) e.setAttribute('style', style);
    if (text != null) e.textContent = text;
    if (parent) parent.appendChild(e);
    return e;
  };
  const target = (el) => (typeof el === 'string' ? document.querySelector(el) : el);
  const colourOf = (name, i) => {
    const c = name || `c${(i % 6) + 1}`;
    return /^(c[1-6]|accent|muted|danger|success|warning|text|text-2)$/.test(c) ? `var(--${c})` : 'var(--c1)';
  };
  // How wide a label is drawn, in the drawing's units: measured where the
  // page can, estimated where it cannot.
  const widthOf = (svg, text, size) => {
    const probe = svgEl('text', { x: -9999, y: -9999, 'font-size': size }, svg, text);
    let w = 0;
    try {
      w = probe.getComputedTextLength();
    } catch (_) {
      w = 0;
    }
    probe.remove();
    return w > 0 ? w : String(text).length * size * 0.6;
  };
  const unitOf = (axis) => (axis && axis.unit ? ` (${axis.unit})` : '');

  // Round ticks: steps of 1, 2 or 5 times a power of ten, about `count` of them.
  const niceTicks = (min, max, count) => {
    if (!(max > min)) {
      const pad = Math.abs(min) * 0.1 || 1;
      min -= pad;
      max += pad;
    }
    const raw = (max - min) / Math.max(1, count);
    const power = 10 ** Math.floor(Math.log10(raw));
    const step = [1, 2, 2.5, 5, 10].map((m) => m * power).find((s) => s >= raw) || 10 * power;
    const start = Math.floor(min / step + 1e-9) * step;
    const end = Math.ceil(max / step - 1e-9) * step;
    const ticks = [];
    for (let v = start; v <= end + step / 2; v += step) ticks.push(Math.abs(v) < step * 1e-9 ? 0 : v);
    const digits = Math.max(0, -Math.floor(Math.log10(step) + 1e-9) + (String(step / power).includes('.') ? 1 : 0));
    return { ticks, min: start, max: end, digits };
  };

  // A label on a box of the surface's colour: a line passing under it does
  // not run through its letters.
  const boxedLabel = (svg, x, y, text, opts) => {
    const size = (opts && opts.size) || 12;
    const g = svgEl('g', {}, svg);
    const w = widthOf(svg, text, size) + 8;
    const anchor = (opts && opts.anchor) || 'middle';
    const left = anchor === 'middle' ? x - w / 2 : anchor === 'end' ? x - w : x;
    svgEl('rect', { x: left, y: y - size + 1, width: w, height: size + 6, rx: 3, fill: 'var(--surface)', 'fill-opacity': 0.9 }, g);
    svgEl('text', { x: left + 4, y: y + 2, 'font-size': size, fill: (opts && opts.colour) || 'var(--text)', 'font-weight': (opts && opts.weight) || null }, g, text);
    return g;
  };

  // A point is `[x, y]` or `{ x, y }`. The genetic drift of 2026-10-28 gave
  // `{ x, y }`, every point was dropped as not a number, and its chart stood
  // empty with nothing said.
  const pointOf = (pt) => (Array.isArray(pt) ? pt : pt && typeof pt === 'object' ? [pt.x, pt.y] : [NaN, NaN]);

  /**
   * A chart: one or more panels stacked on one x axis, each with its own y
   * axis — one measure per panel. Lines from points or from a function, areas
   * between two lines (or a line and zero) with their amount written inside,
   * marked points, or bars over categories.
   *
   *   synabit.chart(el, {
   *     title: 'What it shows',
   *     x: { label: 'thời gian', unit: 's', min: 0, max: 10 },        // or { categories: ['A', 'B'] }
   *     panels: [{
   *       y: { label: 'vận tốc', unit: 'm/s', min: 0 },
   *       series: [{ name: 'v', fn: (t) => v(t) }, { name: 'đo', points: [[0, 0], [1, 2.4]] }],   // or [{ x: 0, y: 0 }, …]
   *       areas: [{ name: 'quãng đường', below: 'v', from: 0, to: 4, label: '20 m' }],
   *       marks: [{ x: 4, y: 10, label: 'đạt 10 m/s' }],
   *     }],
   *   });
   *
   * A series of bars is `{ name, values: [...] }` with `x.categories`. An
   * area is `below` one series, down to `above` another or to zero; either
   * may be a number instead, a level such as a price. Colours
   * are `c1`…`c6`, given in order unless named. Returns `{ x, y, svgs }`: the
   * scales, `y[i]` for panel `i`, and each panel's `<svg>`, to draw anything
   * else into.
   */
  const chart = (el, opts) => {
    const root = target(el);
    if (!root) throw new Error('synabit.chart: no element to draw into');
    root.replaceChildren();
    const o = opts || {};
    const xAxis = o.x || {};
    const panels = (o.panels || [{ y: o.y, series: o.series, areas: o.areas, marks: o.marks }]).map((p) => ({
      ...p,
      y: p.y || {},
      series: (p.series || []).map((s) => (Array.isArray(s.points) ? { ...s, points: s.points.map(pointOf) } : s)),
      areas: p.areas || [],
      marks: p.marks || [],
    }));
    const categories = Array.isArray(xAxis.categories) ? xAxis.categories : null;
    // As wide as where it is drawn, so its text is drawn at the size it is
    // set: a 640-wide drawing squeezed into two thirds of the frame made
    // 11px ticks into 7px ones.
    const W = Math.round(Math.min(640, Math.max(280, root.clientWidth || 640)));

    // The x domain: given, or from every point drawn.
    let xs = [];
    for (const p of panels) {
      for (const s of p.series) if (Array.isArray(s.points)) xs.push(...s.points.map((pt) => pt[0]));
      for (const m of p.marks) xs.push(m.x);
    }
    xs = xs.filter(Number.isFinite);
    const xMin = xAxis.min != null ? xAxis.min : Math.min(...xs);
    const xMax = xAxis.max != null ? xAxis.max : Math.max(...xs);
    const sampled = (s) => {
      if (Array.isArray(s.points)) {
        const pts = s.points.filter((pt) => Number.isFinite(pt[0]) && Number.isFinite(pt[1]));
        if (s.points.length && !pts.length) {
          throw new Error(`synabit.chart: none of the ${s.points.length} points of ${quote(s.name || 'a series')} is [x, y] with two numbers, so it draws nothing`);
        }
        return pts;
      }
      if (typeof s.fn !== 'function') return [];
      const out = [];
      const n = s.samples || 200;
      for (let i = 0; i <= n; i++) {
        const x = xMin + ((xMax - xMin) * i) / n;
        const y = s.fn(x);
        if (Number.isFinite(y)) out.push([x, y]);
      }
      return out;
    };

    // The legend: every named series and area, with its colour.
    const entries = [];
    let colourAt = 0;
    for (const p of panels) {
      for (const s of p.series) {
        s.colourVar = colourOf(s.colour, colourAt++);
        if (s.name) entries.push({ name: s.name, colour: s.colourVar, area: false });
      }
      for (const a of p.areas) {
        a.colourVar = colourOf(a.colour, colourAt++);
        if (a.name) entries.push({ name: a.name, colour: a.colourVar, area: true });
      }
    }
    if (o.title) htmlEl('div', 'font-weight:600;margin-bottom:4px', root, o.title);
    if (entries.length > 1 || entries.some((e) => e.area)) {
      const legend = htmlEl('div', 'display:flex;flex-wrap:wrap;gap:4px 14px;font-size:12px;color:var(--text-2);margin-bottom:4px', root);
      for (const e of entries) {
        const item = htmlEl('span', 'display:inline-flex;align-items:center;gap:6px', legend);
        htmlEl('span', `display:inline-block;width:12px;height:${e.area ? 10 : 3}px;border-radius:2px;background:${e.colour};${e.area ? 'opacity:0.35' : ''}`, item);
        htmlEl('span', '', item, e.name);
      }
    }

    const scales = { x: null, y: [], svgs: [] };
    panels.forEach((p, index) => {
      const last = index === panels.length - 1;
      const lines = p.series.filter((s) => !Array.isArray(s.values)).map((s) => ({ s, pts: sampled(s) }));
      const bars = p.series.filter((s) => Array.isArray(s.values));
      // The y domain: given, or every value drawn, and zero unless told not to.
      let ys = [];
      for (const l of lines) ys.push(...l.pts.map((pt) => pt[1]));
      for (const b of bars) ys.push(...b.values.filter(Number.isFinite));
      for (const m of p.marks) ys.push(m.y);
      ys = ys.filter(Number.isFinite);
      if (p.y.zero !== false || bars.length || p.areas.length) ys.push(0);
      const yTicks = niceTicks(p.y.min != null ? p.y.min : Math.min(...ys), p.y.max != null ? p.y.max : Math.max(...ys), p.y.ticks || 5);
      const yLo = p.y.min != null ? p.y.min : yTicks.min;
      const yHi = p.y.max != null ? p.y.max : yTicks.max;
      const yFormat = p.y.format || ((v) => formatNumber(v, yTicks.digits));

      const H = p.height || (panels.length > 1 ? 180 : 240);
      const svg = svgEl('svg', { viewBox: `0 0 ${W} ${H}`, role: 'img', 'aria-label': p.y.label ? `${p.y.label}${unitOf(p.y)}` : o.title || 'chart', 'data-synabit': 'chart' }, root);
      svg.style.width = '100%';
      svg.style.display = 'block';
      const shownY = yTicks.ticks.filter((v) => v >= yLo - 1e-9 && v <= yHi + 1e-9);
      const L = Math.max(28, ...shownY.map((v) => widthOf(svg, yFormat(v), 11))) + 10;
      const R = 14;
      const T = 22;
      const B = last ? 40 : 22;
      const plotW = W - L - R;
      const plotH = H - T - B;
      const y = (v) => T + (1 - (v - yLo) / (yHi - yLo || 1)) * plotH;

      // The x scale: categories in bands, or numbers.
      let x;
      let xTickList;
      let band = 0;
      if (categories) {
        band = plotW / categories.length;
        x = (i) => L + band * (i + 0.5);
        xTickList = categories.map((c, i) => ({ at: x(i), text: String(c) }));
      } else {
        x = (v) => L + ((v - xMin) / (xMax - xMin || 1)) * plotW;
        const widest = Math.max(widthOf(svg, formatNumber(xMax, 2), 11), widthOf(svg, formatNumber(xMin, 2), 11));
        const count = Math.max(2, Math.min(xAxis.ticks || 6, Math.floor(plotW / (widest + 24))));
        const xt = niceTicks(xMin, xMax, count);
        const xFormat = xAxis.format || ((v) => formatNumber(v, xt.digits));
        xTickList = xt.ticks.filter((v) => v >= xMin - 1e-9 && v <= xMax + 1e-9).map((v) => ({ at: x(v), text: xFormat(v) }));
      }
      scales.x = x;
      scales.y.push(y);
      scales.svgs.push(svg);

      // The y title above the axis, level, so it never crosses a tick.
      svgEl('text', { x: L, y: 12, 'font-size': 12, fill: 'var(--text-2)' }, svg, `${p.y.label || ''}${unitOf(p.y)}`);
      for (const v of shownY) {
        svgEl('line', { x1: L, x2: W - R, y1: y(v), y2: y(v), stroke: 'var(--grid)', 'stroke-width': 1 }, svg);
        svgEl('text', { x: L - 6, y: y(v) + 4, 'text-anchor': 'end', 'font-size': 11, fill: 'var(--muted)' }, svg, yFormat(v));
      }
      xTickList.forEach((t, i) => {
        // The first and last are kept inside the drawing.
        const anchor = !categories && i === 0 && t.at - L < 12 ? 'start' : !categories && i === xTickList.length - 1 && W - R - t.at < 12 ? 'end' : 'middle';
        svgEl('text', { x: t.at, y: T + plotH + 16, 'text-anchor': anchor, 'font-size': 11, fill: 'var(--muted)' }, svg, t.text);
      });
      if (last && (xAxis.label || xAxis.unit)) {
        svgEl('text', { x: W - R, y: H - 4, 'text-anchor': 'end', 'font-size': 12, fill: 'var(--text-2)' }, svg, `${xAxis.label || ''}${unitOf(xAxis)}`);
      }

      // Areas first, under the lines that bound them. A bound is a series by
      // its name, or a level: the tax of 2026-10-28 had no line at the
      // buyer's price, and shaded its revenue between demand and supply.
      const lineOf = (bound) => (typeof bound === 'number' && Number.isFinite(bound) ? { level: bound } : lines.find((l) => l.s.name === bound));
      const valueAt = (l, v) => {
        if (!l) return 0;
        if (l.level != null) return l.level;
        if (typeof l.s.fn === 'function') return l.s.fn(v);
        const pts = l.pts;
        for (let i = 1; i < pts.length; i++) {
          if (pts[i][0] >= v) {
            const [x0, y0] = pts[i - 1];
            const [x1, y1] = pts[i];
            return x1 === x0 ? y1 : y0 + ((y1 - y0) * (v - x0)) / (x1 - x0);
          }
        }
        return pts.length ? pts[pts.length - 1][1] : 0;
      };
      for (const a of p.areas) {
        const top = lineOf(a.below);
        const bottom = a.above != null ? lineOf(a.above) : null;
        const from = a.from != null ? a.from : xMin;
        const to = a.to != null ? a.to : xMax;
        const upper = [];
        const lower = [];
        for (let i = 0; i <= 80; i++) {
          const v = from + ((to - from) * i) / 80;
          upper.push(`${x(v)},${y(valueAt(top, v))}`);
          lower.unshift(`${x(v)},${y(bottom ? valueAt(bottom, v) : 0)}`);
        }
        svgEl('polygon', { points: [...upper, ...lower].join(' '), fill: a.colourVar, 'fill-opacity': 0.22 }, svg);
        if (a.label != null) {
          const mid = (from + to) / 2;
          const yMid = (valueAt(top, mid) + (bottom ? valueAt(bottom, mid) : 0)) / 2;
          boxedLabel(svg, x(mid), y(yMid) + 4, String(a.label), { weight: 600 });
        }
      }
      // Bars, grouped in each category.
      bars.forEach((b, k) => {
        const w = (band * 0.7) / bars.length;
        b.values.forEach((v, i) => {
          if (!Number.isFinite(v)) return;
          const left = L + band * i + band * 0.15 + w * k;
          const top = Math.min(y(v), y(0));
          const r = svgEl('rect', { x: left, y: top, width: Math.max(1, w - 2), height: Math.abs(y(v) - y(0)), rx: 2, fill: b.colourVar }, svg);
          svgEl('title', {}, r, `${b.name ? `${b.name}, ` : ''}${categories[i]}: ${yFormat(v)}${p.y.unit ? ` ${p.y.unit}` : ''}`);
        });
      });
      for (const l of lines) {
        if (l.pts.length < 2) continue;
        svgEl('polyline', {
          points: l.pts.map((pt) => `${x(pt[0])},${y(pt[1])}`).join(' '),
          fill: 'none',
          stroke: l.s.colourVar,
          'stroke-width': l.s.width || 2,
          'stroke-dasharray': l.s.dashed ? '5 4' : null,
        }, svg);
      }
      for (const m of p.marks) {
        if (!Number.isFinite(m.x) || !Number.isFinite(m.y)) continue;
        const colour = colourOf(m.colour || 'accent', 0);
        svgEl('circle', { cx: x(m.x), cy: y(m.y), r: 4, fill: colour }, svg);
        if (m.label) {
          // Beside the point, on the side with room.
          const right = x(m.x) < L + plotW * 0.7;
          const above = y(m.y) > T + 20;
          boxedLabel(svg, x(m.x) + (right ? 8 : -8), y(m.y) + (above ? -10 : 18), String(m.label), { anchor: right ? 'start' : 'end' });
        }
      }
    });
    return scales;
  };

  /**
   * Controls: each a label, its value with its unit, the input, its range, and
   * a line of help. `onChange(values)` is called once at the start and on
   * every change, with every control's value by `id`.
   *
   *   synabit.controls(el, [
   *     { id: 'h', label: 'Độ cao', min: 5, max: 50, step: 5, value: 20, unit: 'm', help: 'Thả từ độ cao này', presets: [{ label: 'Tháp Eiffel', value: 300 }] },
   *     { id: 'planet', label: 'Nơi thả', options: [{ value: 'earth', label: 'Trái Đất' }], value: 'earth' },
   *     { id: 'air', label: 'Có lực cản', type: 'checkbox', value: false },
   *   ], (v) => draw(v));
   */
  const controls = (el, specs, onChange) => {
    const root = target(el);
    if (!root) throw new Error('synabit.controls: no element to draw into');
    root.replaceChildren();
    const box = htmlEl('div', 'display:grid;gap:12px', root);
    const read = {};
    const values = () => Object.fromEntries(Object.entries(read).map(([id, f]) => [id, f()]));
    const changed = () => typeof onChange === 'function' && onChange(values());
    for (const spec of specs || []) {
      const id = String(spec.id);
      const wrap = htmlEl('div', '', box);
      const head = htmlEl('div', 'display:flex;justify-content:space-between;align-items:baseline;gap:8px', wrap);
      const label = htmlEl('label', 'font-size:13px', head, spec.label || id);
      const shown = htmlEl('span', 'font-variant-numeric:tabular-nums;font-weight:600', head);
      let input;
      if (Array.isArray(spec.options)) {
        input = htmlEl('select', 'width:100%;margin-top:4px', wrap);
        for (const opt of spec.options) {
          const option = htmlEl('option', '', input, opt.label != null ? opt.label : String(opt.value));
          option.value = String(opt.value);
        }
        if (spec.value != null) input.value = String(spec.value);
        read[id] = () => {
          const opt = spec.options.find((x) => String(x.value) === input.value);
          return opt ? opt.value : input.value;
        };
        shown.textContent = '';
      } else if (spec.type === 'checkbox') {
        input = htmlEl('input', '', head);
        input.type = 'checkbox';
        input.checked = !!spec.value;
        head.insertBefore(input, label);
        read[id] = () => input.checked;
      } else {
        input = htmlEl('input', 'width:100%;margin-top:4px', wrap);
        input.type = 'range';
        input.min = String(spec.min != null ? spec.min : 0);
        input.max = String(spec.max != null ? spec.max : 100);
        input.step = String(spec.step != null ? spec.step : 'any');
        input.value = String(spec.value != null ? spec.value : spec.min || 0);
        const digits = spec.digits != null ? spec.digits : spec.step != null ? Math.max(0, -Math.floor(Math.log10(spec.step) + 1e-9)) : 2;
        const unit = spec.unit ? ` ${spec.unit}` : '';
        const show = () => (shown.textContent = `${formatNumber(+input.value, digits)}${unit}`);
        show();
        input.addEventListener('input', show);
        read[id] = () => +input.value;
        htmlEl('div', 'display:flex;justify-content:space-between;font-size:12px;color:var(--muted)', wrap)
          .append(htmlEl('span', '', null, `${formatNumber(+input.min, digits)}${unit}`), htmlEl('span', '', null, `${formatNumber(+input.max, digits)}${unit}`));
        if (Array.isArray(spec.presets) && spec.presets.length) {
          const row = htmlEl('div', 'display:flex;flex-wrap:wrap;gap:6px;margin-top:4px', wrap);
          for (const preset of spec.presets) {
            const b = htmlEl('button', 'font-size:12px;padding:2px 8px', row, preset.label);
            b.type = 'button';
            b.addEventListener('click', () => {
              input.value = String(preset.value);
              show();
              changed();
            });
          }
        }
      }
      input.id = `control-${id}`;
      label.htmlFor = input.id;
      input.addEventListener(input.tagName === 'SELECT' || input.type === 'checkbox' ? 'change' : 'input', changed);
      if (spec.help) htmlEl('div', 'font-size:12px;color:var(--muted);margin-top:2px', wrap, spec.help);
    }
    changed();
    return { values };
  };

  /**
   * Numbers to read at a glance: each a label, the value with its unit, and a
   * note — the formula or the check it came from. Not the value of a control:
   * the control already shows that.
   *
   *   synabit.stats(el, [{ label: 'Chạm đất sau', value: 2.02, unit: 's', digits: 2, note: '√(2h/g)' }]);
   */
  const stats = (el, items) => {
    const root = target(el);
    if (!root) throw new Error('synabit.stats: no element to draw into');
    root.replaceChildren();
    const grid = htmlEl('div', 'display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:10px 16px', root);
    for (const item of items || []) {
      const cell = htmlEl('div', '', grid);
      htmlEl('div', 'font-size:12px;color:var(--muted)', cell, item.label || '');
      const line = htmlEl('div', '', cell);
      const value = typeof item.value === 'number' ? formatNumber(item.value, item.digits) : String(item.value != null ? item.value : '—');
      htmlEl('span', `font-size:22px;font-weight:600;font-variant-numeric:tabular-nums;color:${item.colour ? colourOf(item.colour, 0) : 'var(--text)'}`, line, value);
      if (item.unit) htmlEl('span', 'font-size:14px;color:var(--text-2);margin-left:4px', line, item.unit);
      if (item.note) htmlEl('div', 'font-size:12px;color:var(--muted)', cell, item.note);
    }
  };

  /**
   * The frame of a visual, as named slots to draw into:
   * - `side`: `controls` beside `stats` over `stage`; one column when narrow.
   * - `stack`: `controls`, then `stats`, then `stage`.
   * - `compare`: `controls`, then two cases side by side, `a` and `b`, each
   *   with its own `aStats` / `bStats`; one column when narrow.
   */
  const layout = (el, kind) => {
    const root = target(el);
    if (!root) throw new Error('synabit.layout: no element to draw into');
    root.replaceChildren();
    const frame = htmlEl('div', 'max-width:640px;display:flex;flex-direction:column;gap:14px', root);
    const slot = (parent, style) => htmlEl('div', style || '', parent);
    if (kind === 'side') {
      const row = slot(frame, 'display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start');
      const controlsSlot = slot(row, 'flex:1 1 180px;min-width:0');
      const main = slot(row, 'flex:2 1 340px;min-width:0;display:flex;flex-direction:column;gap:10px');
      return { controls: controlsSlot, stats: slot(main), stage: slot(main) };
    }
    if (kind === 'compare') {
      const controlsSlot = slot(frame);
      const row = slot(frame, 'display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start');
      const half = () => slot(row, 'flex:1 1 260px;min-width:0;display:flex;flex-direction:column;gap:8px');
      const a = half();
      const b = half();
      return { controls: controlsSlot, aStats: slot(a), a: slot(a), bStats: slot(b), b: slot(b) };
    }
    return { controls: slot(frame), stats: slot(frame), stage: slot(frame) };
  };

  // ── Numbers worked out from the model, not derived by hand ──
  //
  // The tax visual of 2026-10-28 drew the equilibrium from `(t + 120) / 5`, a
  // formula derived in the head with a 3 dropped; `compute` ran it faithfully.
  // After Q4 its own `synabit.expect` caught the price, three times, and the
  // model could not find the right formula to put in its place. These find
  // the number from the model's own functions, so there is no formula to get
  // wrong (plan 2026-10-28, §2.2).

  const refuse = (what, why) => {
    throw new Error(`synabit.${what}: ${why}`);
  };
  const finite = (what, v, name) => (Number.isFinite(v) ? v : refuse(what, `${name} is ${v}, not a number`));

  /**
   * Where `f` crosses zero between `lo` and `hi`: an equilibrium is
   * `solve((p) => demand(p) - supply(p), 0, 100)`. If `f` has the same sign
   * at both ends, the range is searched for a crossing; if there is none, it
   * throws, saying so.
   */
  const solve = (f, lo, hi) => {
    if (typeof f !== 'function') refuse('solve', 'the first argument is the function whose zero is wanted');
    let a = finite('solve', +lo, 'lo');
    let b = finite('solve', +hi, 'hi');
    let fa = f(a);
    let fb = f(b);
    if (fa === 0) return a;
    if (fb === 0) return b;
    if (!(fa * fb < 0)) {
      // Look for a crossing inside: a root between the ends with the same sign at both.
      const n = 400;
      let found = false;
      for (let i = 1; i <= n && !found; i++) {
        const x = a + ((b - a) * i) / n;
        const fx = f(x);
        if (fx === 0) return x;
        if (Number.isFinite(fx) && Number.isFinite(fa) && fa * fx < 0) {
          b = x;
          fb = fx;
          found = true;
        } else if (Number.isFinite(fx) && !Number.isFinite(fa)) {
          a = x;
          fa = fx;
        }
      }
      if (!found) refuse('solve', `the function does not cross zero between ${lo} and ${hi} (it is ${fa} and ${fb} at the ends); give a range where it changes sign`);
    }
    for (let i = 0; i < 200 && b - a > 1e-13 * Math.max(1, Math.abs(a)); i++) {
      const m = (a + b) / 2;
      const fm = f(m);
      if (fm === 0) return m;
      if (fa * fm < 0) {
        b = m;
        fb = fm;
      } else {
        a = m;
        fa = fm;
      }
    }
    return (a + b) / 2;
  };

  /**
   * The highest point of `f` between `lo` and `hi`, as `{ x, y }`; with
   * `'min'`, the lowest. The whole range is searched, then the best place is
   * refined: a peak read off a part of the range is the wrong peak.
   */
  const peak = (f, lo, hi, which) => {
    if (typeof f !== 'function') refuse('peak', 'the first argument is the function to search');
    const a = finite('peak', +lo, 'lo');
    const b = finite('peak', +hi, 'hi');
    const sign = which === 'min' ? -1 : 1;
    const g = (x) => {
      const y = f(x);
      return Number.isFinite(y) ? sign * y : -Infinity;
    };
    const n = 2000;
    let best = a;
    let bestY = g(a);
    for (let i = 1; i <= n; i++) {
      const x = a + ((b - a) * i) / n;
      const y = g(x);
      if (y > bestY) {
        best = x;
        bestY = y;
      }
    }
    // Golden-section search in the cell around the best sample.
    const h = (b - a) / n;
    let l = Math.max(a, best - h);
    let r = Math.min(b, best + h);
    const phi = (Math.sqrt(5) - 1) / 2;
    for (let i = 0; i < 80; i++) {
      const m1 = r - phi * (r - l);
      const m2 = l + phi * (r - l);
      if (g(m1) < g(m2)) l = m1;
      else r = m2;
    }
    const x = g((l + r) / 2) >= bestY ? (l + r) / 2 : best;
    if (!Number.isFinite(g(x))) refuse('peak', `the function has no value between ${lo} and ${hi}`);
    return { x, y: f(x) };
  };

  /** The integral of `f` from `a` to `b`, by Simpson's rule on `n` steps. */
  const integrate = (f, a, b, n) => {
    if (typeof f !== 'function') refuse('integrate', 'the first argument is the function to integrate');
    const lo = finite('integrate', +a, 'a');
    const hi = finite('integrate', +b, 'b');
    const steps = Math.max(2, Math.round((n || 2000) / 2) * 2);
    const h = (hi - lo) / steps;
    let s = f(lo) + f(hi);
    for (let i = 1; i < steps; i++) s += f(lo + i * h) * (i % 2 ? 4 : 2);
    return finite('integrate', (s * h) / 3, 'the integral');
  };

  // One step of a state of named numbers. `rates(state, t)` gives each
  // one's rate of change, by the same names.
  const add = (s, d, k) => {
    const out = {};
    for (const key in s) out[key] = s[key] + (d[key] || 0) * k;
    return out;
  };
  const rk4 = (rates, s, t, dt) => {
    const k1 = rates(s, t);
    const k2 = rates(add(s, k1, dt / 2), t + dt / 2);
    const k3 = rates(add(s, k2, dt / 2), t + dt / 2);
    const k4 = rates(add(s, k3, dt), t + dt);
    const out = {};
    for (const key in s) out[key] = s[key] + (dt / 6) * ((k1[key] || 0) + 2 * (k2[key] || 0) + 2 * (k3[key] || 0) + (k4[key] || 0));
    return out;
  };
  // Velocity Verlet, for mechanics: `pairs` names each position's velocity,
  // and the velocity's rate (the acceleration) depends on positions only.
  const verlet = (rates, pairs, s, t, dt) => {
    const a0 = rates(s, t);
    const out = Object.assign({}, s);
    for (const [x, v] of Object.entries(pairs)) out[x] = s[x] + s[v] * dt + 0.5 * (a0[v] || 0) * dt * dt;
    const a1 = rates(out, t + dt);
    for (const [, v] of Object.entries(pairs)) out[v] = s[v] + 0.5 * ((a0[v] || 0) + (a1[v] || 0)) * dt;
    return out;
  };
  const stepper = (opts) => {
    if (typeof opts.rates !== 'function') refuse('ode', '`rates(state, t)` gives the rate of change of each part of the state');
    return opts.method === 'verlet'
      ? (s, t, dt) => verlet(opts.rates, opts.pairs || {}, s, t, dt)
      : (s, t, dt) => rk4(opts.rates, s, t, dt);
  };

  /**
   * A system of equations followed from `t0` to `t1` with a fixed step: an
   * epidemic, a population, a step response. Returns the states, each with its
   * `t`, about `points` of them (default 400) for a chart.
   *
   *   const path = synabit.ode((s) => ({ S: -b * s.S * s.I, I: b * s.S * s.I - g * s.I, R: g * s.I }),
   *     { S: 0.99, I: 0.01, R: 0 }, 0, 160, { dt: 0.05 });
   *   synabit.chart(el, { series: [{ name: 'I', points: path.map((p) => [p.t, p.I]) }] });
   */
  const ode = (rates, state, t0, t1, opts) => {
    const o = Object.assign({ dt: 0.01, points: 400 }, opts || {}, { rates });
    const step = stepper(o);
    const start = finite('ode', +t0, 't0');
    const end = finite('ode', +t1, 't1');
    const n = Math.max(1, Math.ceil((end - start) / o.dt));
    const dt = (end - start) / n;
    const every = Math.max(1, Math.floor(n / o.points));
    let s = Object.assign({}, state);
    const out = [Object.assign({ t: start }, s)];
    for (let i = 1; i <= n; i++) {
      s = step(s, start + (i - 1) * dt, dt);
      if (i % every === 0 || i === n) out.push(Object.assign({ t: start + i * dt }, s));
    }
    for (const key in s) finite('ode', s[key], `${key} at the end`);
    return out;
  };

  /**
   * A system that moves, drawn as it moves, with run / pause / reset and its
   * invariants shown with how far they have drifted.
   *
   *   synabit.sim(el, {
   *     state: { theta: 0.5, omega: 0 },
   *     rates: (s) => ({ theta: s.omega, omega: -(9.81 / L) * Math.sin(s.theta) }),
   *     method: 'verlet', pairs: { theta: 'omega' },     // or the default, RK4
   *     dt: 0.001, speed: 1,                              // seconds of model per second shown
   *     invariants: { 'cơ năng (J)': (s) => energy(s) },
   *     draw: (s, t) => { … },                            // called every frame
   *   });
   *
   * Returns `{ reset(state), state(), time() }`. Starts paused when `paused`.
   */
  const nextFrame = (f) => (typeof requestAnimationFrame === 'function' ? requestAnimationFrame(f) : setTimeout(() => f(Date.now()), 16));
  const sim = (el, opts) => {
    const root = target(el);
    if (!root) refuse('sim', 'no element to draw into');
    const o = Object.assign({ dt: 0.001, speed: 1, invariants: {} }, opts || {});
    const step = stepper(o);
    if (typeof o.draw !== 'function') refuse('sim', '`draw(state, t)` draws the state');
    root.replaceChildren();
    const vi = String(context.locale).toLowerCase().startsWith('vi');
    const bar = htmlEl('div', 'display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:6px', root);
    const play = htmlEl('button', '', bar);
    play.type = 'button';
    const again = htmlEl('button', '', bar, vi ? 'Đặt lại' : 'Reset');
    again.type = 'button';
    const clock = htmlEl('span', 'font-variant-numeric:tabular-nums;color:var(--muted);font-size:12px', bar);
    const kept = htmlEl('div', '', root);
    let s;
    let t;
    let start;
    let running = !o.paused;
    let last = null;
    const label = () => (play.textContent = running ? (vi ? 'Dừng' : 'Pause') : vi ? 'Chạy' : 'Run');
    const show = () => {
      clock.textContent = `t = ${formatNumber(t, 2)} s`;
      const items = Object.entries(o.invariants).map(([name, f]) => {
        const now = f(s);
        const was = start[name];
        const drift = was ? ((now - was) / Math.abs(was)) * 100 : now - was;
        return { label: name, value: now, digits: 4, note: `${vi ? 'lệch' : 'drift'} ${formatNumber(drift, 3)}${was ? '%' : ''}` };
      });
      if (items.length) stats(kept, items);
      o.draw(s, t);
    };
    const reset = (state) => {
      s = Object.assign({}, state || o.state);
      t = 0;
      start = Object.fromEntries(Object.entries(o.invariants).map(([name, f]) => [name, f(s)]));
      show();
    };
    const frame = (now) => {
      if (running) {
        // Real time since the last frame, capped so a hidden tab does not leap.
        const real = last == null ? 0 : Math.min(0.05, (now - last) / 1000);
        let left = real * o.speed;
        while (left > 1e-12) {
          const dt = Math.min(o.dt, left);
          s = step(s, t, dt);
          t += dt;
          left -= dt;
        }
        show();
      }
      last = now;
      if (root.isConnected) nextFrame(frame);
    };
    play.addEventListener('click', () => {
      running = !running;
      label();
    });
    again.addEventListener('click', () => reset());
    label();
    reset();
    nextFrame(frame);
    return { reset, state: () => Object.assign({}, s), time: () => t };
  };

  // ── Steps and graphs ──
  //
  // The Dijkstra of 2026-10-27 showed "∞" in its caption while its table at
  // step 0 already held B = 4, drew arrows on an undirected graph, and gave
  // its nodes no colour for their state; another looped for ever stepping
  // (plan 2026-10-28, §1). A stepper that only shows steps worked out before,
  // and a graph drawn from its state, leave none of that to write.

  const words = () =>
    String(context.locale).toLowerCase().startsWith('vi')
      ? { back: 'Lùi', next: 'Tiếp', play: 'Tự chạy', pause: 'Dừng', step: 'Bước' }
      : { back: 'Back', next: 'Next', play: 'Play', pause: 'Pause', step: 'Step' };

  /**
   * Steps worked out before anything is drawn, shown one at a time. Step 0 is
   * the state before the first step; each step's `line` says what it did,
   * with its arithmetic. `render(step, index, el)` draws one into `el`.
   *
   *   const steps = [{ line: 'Bắt đầu: d(A) = 0, mọi đỉnh khác ∞', … }, …];   // worked out first
   *   synabit.stepper(el, { steps, render: (s, i, stage) => synabit.graph(stage, { … }) });
   *
   * Back is disabled at step 0, next at the last; play steps on by itself
   * every `every` ms (default 900). Returns `{ go(i), at() }`.
   */
  const stepperOf = (el, opts) => {
    const root = target(el);
    if (!root) refuse('stepper', 'no element to draw into');
    const o = opts || {};
    const steps = Array.isArray(o.steps) ? o.steps : [];
    if (!steps.length) refuse('stepper', '`steps` is the list of states, worked out before; step 0 is the start');
    if (typeof o.render !== 'function') refuse('stepper', '`render(step, index, el)` draws one step');
    root.replaceChildren();
    const w = words();
    const bar = htmlEl('div', 'display:flex;align-items:center;gap:8px;flex-wrap:wrap', root);
    const back = htmlEl('button', '', bar, w.back);
    const next = htmlEl('button', '', bar, w.next);
    const play = htmlEl('button', '', bar, w.play);
    for (const b of [back, next, play]) b.type = 'button';
    const where = htmlEl('span', 'font-variant-numeric:tabular-nums;color:var(--muted);font-size:13px', bar);
    const track = htmlEl('div', 'height:4px;border-radius:2px;background:var(--border);margin:8px 0', root);
    const fill = htmlEl('div', 'height:100%;border-radius:2px;background:var(--accent);width:0', track);
    const line = htmlEl('div', 'font-size:14px;margin-bottom:8px;min-height:1.4em', root);
    const stage = htmlEl('div', '', root);
    let at = 0;
    let timer = null;
    const last = steps.length - 1;
    const go = (i) => {
      at = Math.max(0, Math.min(last, i));
      back.disabled = at === 0;
      next.disabled = at === last;
      where.textContent = `${w.step} ${at} / ${last}`;
      fill.style.width = `${last ? (at / last) * 100 : 100}%`;
      const step = steps[at];
      const said = typeof o.line === 'function' ? o.line(step, at) : step && step.line;
      line.textContent = said != null ? String(said) : '';
      stage.replaceChildren();
      o.render(step, at, stage);
      if (at === last) stop();
    };
    const stop = () => {
      if (timer) clearInterval(timer);
      timer = null;
      play.textContent = w.play;
    };
    back.addEventListener('click', () => go(at - 1));
    next.addEventListener('click', () => go(at + 1));
    play.addEventListener('click', () => {
      if (timer) return stop();
      if (at === last) go(0);
      play.textContent = w.pause;
      timer = setInterval(() => go(at + 1), o.every || 900);
    });
    go(o.start || 0);
    return { go, at: () => at };
  };

  // What each state looks like, and what the legend calls it.
  const NODE_STATES = {
    done: { fill: 'var(--success)', text: 'var(--surface)' },
    current: { fill: 'var(--accent)', text: 'var(--surface)' },
    seen: { fill: 'var(--warning)', text: 'var(--text)' },
    todo: { fill: 'var(--surface-alt)', text: 'var(--text)' },
  };
  const EDGE_STATES = {
    chosen: { stroke: 'var(--accent)', width: 3.5 },
    active: { stroke: 'var(--warning)', width: 3 },
    rejected: { stroke: 'var(--danger)', width: 1.5, dash: '5 4' },
    idle: { stroke: 'var(--muted)', width: 1.5 },
  };

  /**
   * A graph drawn from its state: nodes coloured by what they are now, edges
   * by whether they are chosen, weights on a box beside each edge, and a note
   * under each node — a distance, a label.
   *
   *   synabit.graph(el, {
   *     nodes: [{ id: 'A', x: 0, y: 1 }, …],               // positions in any units; a circle if none
   *     edges: [{ from: 'A', to: 'B', weight: 4 }, …],
   *     directed: false,
   *     state: { nodes: { A: 'done', B: 'current' }, edges: { 'A-B': 'chosen' }, notes: { A: 'd = 0' } },
   *     legend: { done: 'đã chốt', current: 'đang xét', todo: 'chưa xét', chosen: 'trên đường đi' },
   *   });
   *
   * Node states: done, current, seen, todo. Edge states: chosen, active,
   * rejected, idle. An edge's key is `from-to`; undirected, either order.
   */
  const graph = (el, opts) => {
    const root = target(el);
    if (!root) refuse('graph', 'no element to draw into');
    const o = opts || {};
    const nodes = Array.isArray(o.nodes) ? o.nodes : [];
    const edges = Array.isArray(o.edges) ? o.edges : [];
    const state = o.state || {};
    const nodeState = state.nodes || {};
    const edgeState = state.edges || {};
    const notes = state.notes || {};
    root.replaceChildren();
    const W = Math.round(Math.min(640, Math.max(280, root.clientWidth || 640)));
    const H = o.height || Math.round(W * 0.55);
    const R = o.radius || 17;
    // Positions: given, scaled to fit; or round a circle.
    const placed = nodes.every((n) => Number.isFinite(n.x) && Number.isFinite(n.y));
    const xs = placed ? nodes.map((n) => n.x) : nodes.map((_, i) => Math.cos((2 * Math.PI * i) / nodes.length - Math.PI / 2));
    const ys = placed ? nodes.map((n) => n.y) : nodes.map((_, i) => Math.sin((2 * Math.PI * i) / nodes.length - Math.PI / 2));
    const pad = R + 26;
    const span = (v) => Math.max(...v) - Math.min(...v) || 1;
    const sx = (v) => pad + ((v - Math.min(...xs)) / span(xs)) * (W - 2 * pad);
    const sy = (v) => pad + ((v - Math.min(...ys)) / span(ys)) * (H - 2 * pad);
    const at = Object.fromEntries(nodes.map((n, i) => [String(n.id), { x: sx(xs[i]), y: sy(ys[i]) }]));
    // In the page from the start, so the boxes are measured to their labels.
    const svg = svgEl('svg', { viewBox: `0 0 ${W} ${H}`, role: 'img', 'aria-label': o.title || 'graph', 'data-synabit': 'graph' }, root);
    svg.style.width = '100%';
    svg.style.display = 'block';
    const legendUsed = new Set();
    const keyOf = (e) => `${e.from}-${e.to}`;
    const edgeIs = (e) => edgeState[keyOf(e)] || (!o.directed && edgeState[`${e.to}-${e.from}`]) || 'idle';
    if (o.directed) {
      const defs = svgEl('defs', {}, svg);
      for (const [name, look] of Object.entries(EDGE_STATES)) {
        // Sized in the drawing's units, not the stroke's: a chosen edge is
        // drawn thick, and its arrow would be too.
        const marker = svgEl('marker', { id: `arrow-${name}`, viewBox: '0 0 10 10', refX: 9, refY: 5, markerWidth: 10, markerHeight: 10, markerUnits: 'userSpaceOnUse', orient: 'auto-start-reverse' }, defs);
        svgEl('path', { d: 'M0,0 L10,5 L0,10 z', fill: look.stroke }, marker);
      }
    }
    // Edges from rim to rim, then their weights on a box to one side.
    const weights = [];
    for (const e of edges) {
      const a = at[String(e.from)];
      const b = at[String(e.to)];
      if (!a || !b) continue;
      const is = edgeIs(e);
      if (is !== 'idle') legendUsed.add(is);
      const look = EDGE_STATES[is] || EDGE_STATES.idle;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const len = Math.hypot(dx, dy) || 1;
      const ux = dx / len;
      const uy = dy / len;
      svgEl('line', {
        x1: a.x + ux * R, y1: a.y + uy * R, x2: b.x - ux * (R + (o.directed ? 3 : 0)), y2: b.y - uy * (R + (o.directed ? 3 : 0)),
        stroke: look.stroke, 'stroke-width': look.width, 'stroke-dasharray': look.dash || null,
        'marker-end': o.directed ? `url(#arrow-${is})` : null,
      }, svg);
      const label = e.label != null ? e.label : e.weight;
      if (label != null) {
        // Beside the middle of the edge, on its left: never on the line.
        weights.push({ x: (a.x + b.x) / 2 - uy * 13, y: (a.y + b.y) / 2 + ux * 13 + 4, text: typeof label === 'number' ? formatNumber(label) : String(label) });
      }
    }
    for (const wt of weights) boxedLabel(svg, wt.x, wt.y, wt.text, { size: 12 });
    for (const n of nodes) {
      const p = at[String(n.id)];
      const is = nodeState[String(n.id)] || 'todo';
      if (is !== 'todo') legendUsed.add(is);
      const look = NODE_STATES[is] || NODE_STATES.todo;
      svgEl('circle', { cx: p.x, cy: p.y, r: R, fill: look.fill, stroke: 'var(--border)', 'stroke-width': 1 }, svg);
      svgEl('text', { x: p.x, y: p.y + 4.5, 'text-anchor': 'middle', 'font-size': 13, 'font-weight': 600, fill: look.text }, svg, n.label != null ? String(n.label) : String(n.id));
      const note = notes[String(n.id)];
      if (note != null) boxedLabel(svg, p.x, p.y + R + 15, String(note), { size: 12 });
    }
    // The legend: every state shown, with what the visual calls it.
    const names = Object.assign({}, o.legend || {});
    // In the order the visual named them, so the legend stays put from step
    // to step; a state shown that it did not name, after them.
    const used = Object.keys(names).concat([...legendUsed].filter((k) => !(k in names)));
    if (used.length) {
      const legend = htmlEl('div', 'display:flex;flex-wrap:wrap;gap:4px 14px;font-size:12px;color:var(--text-2);margin-bottom:4px', null);
      root.insertBefore(legend, svg);
      for (const k of used) {
        const node = NODE_STATES[k];
        const edge = EDGE_STATES[k];
        if (!node && !edge) continue;
        const item = htmlEl('span', 'display:inline-flex;align-items:center;gap:6px', legend);
        htmlEl('span', node
          ? `display:inline-block;width:11px;height:11px;border-radius:50%;background:${node.fill};border:1px solid var(--border)`
          : `display:inline-block;width:16px;height:0;border-top:3px ${edge.dash ? 'dashed' : 'solid'} ${edge.stroke}`, item);
        htmlEl('span', '', item, names[k] || k);
      }
    }
    return { svg, at };
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
      return formatNumber(value, digits);
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
    /** The parts a visual is built from; see each one's comment above. */
    chart,
    controls,
    stats,
    layout,
    /** Numbers from the model's own functions; see each one's comment above. */
    solve,
    peak,
    integrate,
    ode,
    sim,
    /** Steps worked out first, and a graph drawn from its state. */
    stepper: stepperOf,
    graph,
  });
})();
