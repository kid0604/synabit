// The kit every Syn visual is given: the bridge to the app around it.
//
// A visual runs in a sandboxed frame on `synvisual://` with no network, so
// the only way anything leaves it is `postMessage` to the frame's parent.
// What it says there is JSON-RPC 2.0 with the message names of the MCP Apps
// standard (SEP-1865) — `ui/initialize`, `ui/notifications/size-changed`,
// `ui/notifications/host-context-changed`, `ui/message` — so the host that
// shows these can one day show an MCP App's view as well. Two are Synabit's
// own: `synabit/error`, what broke inside the frame; `synabit/selftest`,
// which only the step-0 page sends.
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
  const fail = (kind, message, where) =>
    notify('synabit/error', { kind, message: String(message || '').slice(0, 500), where: String(where || '') });
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
  let reported = 0;
  const report = () => {
    const height = Math.ceil(Math.max(document.body ? document.body.scrollHeight : 0, html.scrollHeight));
    if (height && height !== reported) {
      reported = height;
      notify('ui/notifications/size-changed', { height });
    }
  };
  document.addEventListener('DOMContentLoaded', () => {
    new ResizeObserver(report).observe(document.body);
    report();
  });

  const ready = request('ui/initialize', {
    protocolVersion: '2026-01-26',
    appInfo: { name: 'synabit-visual', version: '1' },
    appCapabilities: {},
  })
    .then((result) => {
      if (result && result.hostContext) apply(result.hostContext);
      notify('ui/notifications/initialized');
      report();
      return Object.assign({}, context);
    })
    .catch(() => Object.assign({}, context));

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
      return new Intl.NumberFormat(context.locale, { maximumFractionDigits: digits == null ? 2 : digits }).format(value);
    },
    /** A theme token's current value, for a canvas that cannot read CSS variables. */
    color(name) {
      return getComputedStyle(html).getPropertyValue(`--${name}`).trim();
    },
  });
})();
