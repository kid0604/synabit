/**
 * Bước 0 of Syn's visuals (`docs/syn-visuals-plan-2026-10-26.md`): frame the
 * attack page `synvisual://…/spike/attack` the way a visual will be framed,
 * collect what it reports, and add what only the host can see.
 *
 * The page judges what it can from inside — a fetch refused, a global absent.
 * Three things it cannot: whether an IPC message it sent reached Rust (a wrong
 * invoke key is dropped without an answer), whether its form submission took
 * the frame away, and whether its top-level navigation took the app away. The
 * host answers those: Rust counts arrivals at `visual_spike_ping`, the page
 * says "still here" after the form, and this runner is still running after the
 * navigation.
 *
 * Debug builds only — the commands exist nowhere else.
 */
import { invoke } from '@tauri-apps/api/core';

export type Outcome = 'blocked' | 'LEAKED' | 'reachable' | 'unclear' | 'ok' | 'info' | 'error';

export interface SpikeRow {
  group: string;
  name: string;
  outcome: Outcome;
  detail: string;
}

export interface SpikeReport {
  when: string;
  userAgent: string;
  origin: string;
  rows: SpikeRow[];
  /** Ways out that worked. */
  leaked: number;
  /** Things that should have worked and did not: the page, or the kit. */
  broken: number;
  unclear: number;
}

/** Where a custom scheme lives: its own scheme everywhere but Windows. */
export function visualOrigin(userAgent: string): string {
  return /Windows/i.test(userAgent) ? 'http://synvisual.localhost' : 'synvisual://localhost';
}

export interface HostSeen {
  /** What the page posted, or `null` if it never answered. */
  results: SpikeRow[] | null;
  /** Arrivals at `visual_spike_ping`. */
  pings: number;
  /** The page answered after its form submission. */
  afterForm: boolean;
  /** The page's report on its top-level navigation, if one came. */
  topNav: { threw: boolean; name?: string } | null;
}

/** The page's rows, followed by the host's verdict on what the page cannot see. */
export function judge(seen: HostSeen): SpikeRow[] {
  if (!seen.results) {
    return [{
      group: 'control',
      name: 'the page answered',
      outcome: 'error',
      detail: 'no reply — the frame did not load, or its script did not run',
    }];
  }
  const rows = [...seen.results];
  rows.push({
    group: 'ipc',
    name: 'an IPC message reached Rust',
    outcome: seen.pings > 0 ? 'LEAKED' : 'blocked',
    detail: `${seen.pings} arrival(s) at visual_spike_ping`,
  });
  rows.push({
    group: 'parent',
    name: 'form submission',
    outcome: seen.afterForm ? 'blocked' : 'LEAKED',
    detail: seen.afterForm ? 'the page was still there after submitting' : 'the page went away after submitting',
  });
  rows.push({
    group: 'parent',
    name: 'top-level navigation',
    // This code is still running, so the app was not taken away. The page
    // either threw or was silently refused; both are a refusal.
    outcome: 'blocked',
    detail: seen.topNav ? (seen.topNav.threw ? `threw ${seen.topNav.name ?? ''}`.trim() : 'did not throw, app stayed') : 'no report; app stayed',
  });
  return rows;
}

/** What the host saw of the kit page, `/spike/kit`. */
export interface KitSeen {
  /** It asked `ui/initialize`. */
  initializeAsked: boolean;
  /** It said `ui/notifications/initialized` once answered. */
  initialized: boolean;
  /** The heights it reported. */
  heights: number[];
  /** What it asked to have put in the chat box. */
  asked: string[];
  /** What it said broke. */
  errors: { kind: string; message: string }[];
  /** Its own report, or `null` if none came. */
  selftest: Record<string, unknown> | null;
}

/** The host answers `ui/initialize` with this; the kit page checks it took. */
export const KIT_HOST_CONTEXT = { theme: 'dark', locale: 'vi-VN' } as const;

/** The kit, judged: every row should be `ok`. */
export function judgeKit(seen: KitSeen): SpikeRow[] {
  const row = (name: string, ok: boolean, detail = ''): SpikeRow => ({ group: 'kit', name, outcome: ok ? 'ok' : 'error', detail });
  const t = seen.selftest ?? {};
  return [
    row('asked ui/initialize', seen.initializeAsked),
    row('said initialized once answered', seen.initialized),
    row('reported its height', seen.heights.some((h) => h > 0), seen.heights.join(', ')),
    row('sent its self-test', seen.selftest != null, seen.selftest ? '' : 'none came'),
    row('d3 drew', t.d3 === true),
    row('KaTeX typeset', t.katex === true),
    row("KaTeX's font loaded", t.katexFont === true),
    row('theme tokens resolve', t.tokens === true),
    row(
      "the host's theme applied",
      t.themeFromHost === KIT_HOST_CONTEXT.theme && t.themeOnPage === KIT_HOST_CONTEXT.theme && t.textColor === '#f4f4f5',
      `${String(t.themeFromHost)} / ${String(t.themeOnPage)} / ${String(t.textColor)}`,
    ),
    row("numbers in the host's locale", t.number === '1.234,5', String(t.number)),
    row('a question reached the host', t.askAnswered === true && seen.asked.length === 1, seen.asked.join(' | ')),
    row('nothing broke inside', seen.errors.length === 0 && t.failed == null, seen.errors.map((e) => `${e.kind}: ${e.message}`).join(' | ') || String(t.failed ?? '')),
  ];
}

export function summarise(rows: SpikeRow[], userAgent: string, origin: string): SpikeReport {
  return {
    when: new Date().toISOString(),
    userAgent,
    origin,
    rows,
    leaked: rows.filter((r) => r.outcome === 'LEAKED').length,
    broken: rows.filter((r) => r.outcome === 'error').length,
    unclear: rows.filter((r) => r.outcome === 'unclear').length,
  };
}

/** One table row per result, ready to paste under appendix A. */
export function asMarkdown(report: SpikeReport): string {
  const lines = [
    `${report.when} · ${report.userAgent}`,
    '',
    '| Nhóm | Phép thử | Kết quả | Chi tiết |',
    '| --- | --- | --- | --- |',
    ...report.rows.map((r) => `| ${r.group} | ${r.name} | ${r.outcome} | ${r.detail.replace(/\|/g, '\\|')} |`),
  ];
  return lines.join('\n');
}

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Frame the attack page, wait for everything it will say, and judge.
 *
 * `into` is where the frame is shown; without one it is laid out off screen,
 * since a frame that is never laid out may never run.
 */
export async function runVisualSpike(into?: HTMLElement, timeoutMs = 30_000): Promise<SpikeReport> {
  const userAgent = navigator.userAgent;
  const origin = visualOrigin(userAgent);
  const attack = await runAttack(origin, into, timeoutMs);
  const kit = await runKit(origin, into, timeoutMs);
  return summarise([...attack, ...kit], userAgent, origin);
}

/** A frame as a visual is framed: sandboxed to scripts, off screen unless shown. */
function frameFor(src: string, into?: HTMLElement): HTMLIFrameElement {
  const frame = document.createElement('iframe');
  frame.setAttribute('sandbox', 'allow-scripts');
  frame.src = src;
  frame.style.cssText = into
    ? 'width:100%;height:160px;border:1px solid currentColor;border-radius:8px'
    : 'position:absolute;left:-10000px;top:0;width:480px;height:320px';
  return frame;
}

/** The attack page: every way out, judged. */
async function runAttack(origin: string, into: HTMLElement | undefined, timeoutMs: number): Promise<SpikeRow[]> {
  await invoke('visual_spike_reset');

  const frame = frameFor(`${origin}/spike/attack`, into);

  const seen: HostSeen = { results: null, pings: 0, afterForm: false, topNav: null };
  let done: () => void = () => {};
  const finished = new Promise<void>((resolve) => { done = resolve; });

  const onMessage = (event: MessageEvent) => {
    // A sandboxed frame's origin is "null"; only its window identifies it.
    if (event.source !== frame.contentWindow) return;
    const data = event.data as { type?: string; results?: SpikeRow[]; threw?: boolean; name?: string };
    if (data?.type === 'synvisual-spike/result') seen.results = data.results ?? [];
    if (data?.type === 'synvisual-spike/after-form') seen.afterForm = true;
    if (data?.type === 'synvisual-spike/top-nav') {
      seen.topNav = { threw: !!data.threw, name: data.name };
      done();
    }
  };
  window.addEventListener('message', onMessage);
  (into ?? document.body).appendChild(frame);

  try {
    await Promise.race([finished, wait(timeoutMs)]);
    // The last IPC attempts were fire-and-forget; give them time to land.
    await wait(1500);
    seen.pings = await invoke<number>('visual_spike_pings');
  } finally {
    window.removeEventListener('message', onMessage);
    frame.remove();
  }
  return judge(seen);
}

/**
 * The kit page, with this code as its host: `ui/initialize` answered with a
 * dark theme and a Vietnamese locale (the page asks for light), a question
 * asked back answered, everything it says recorded.
 */
async function runKit(origin: string, into: HTMLElement | undefined, timeoutMs: number): Promise<SpikeRow[]> {
  const frame = frameFor(`${origin}/spike/kit?theme=light`, into);
  const seen: KitSeen = { initializeAsked: false, initialized: false, heights: [], asked: [], errors: [], selftest: null };
  let done: () => void = () => {};
  const finished = new Promise<void>((resolve) => { done = resolve; });
  const reply = (id: unknown, result: unknown) => frame.contentWindow?.postMessage({ jsonrpc: '2.0', id, result }, '*');

  const onMessage = (event: MessageEvent) => {
    if (event.source !== frame.contentWindow) return;
    const message = event.data as { jsonrpc?: string; id?: unknown; method?: string; params?: Record<string, unknown> };
    if (message?.jsonrpc !== '2.0') return;
    switch (message.method) {
      case 'ui/initialize':
        seen.initializeAsked = true;
        reply(message.id, { protocolVersion: '2026-01-26', hostInfo: { name: 'synabit', version: '1' }, hostCapabilities: {}, hostContext: KIT_HOST_CONTEXT });
        break;
      case 'ui/notifications/initialized':
        seen.initialized = true;
        break;
      case 'ui/notifications/size-changed':
        seen.heights.push(Number(message.params?.height) || 0);
        break;
      case 'ui/message': {
        const content = message.params?.content as { text?: string } | undefined;
        seen.asked.push(String(content?.text ?? ''));
        reply(message.id, {});
        break;
      }
      case 'synabit/error':
        seen.errors.push({ kind: String(message.params?.kind ?? ''), message: String(message.params?.message ?? '') });
        break;
      case 'synabit/selftest':
        seen.selftest = message.params ?? {};
        done();
        break;
    }
  };
  window.addEventListener('message', onMessage);
  (into ?? document.body).appendChild(frame);
  try {
    await Promise.race([finished, wait(timeoutMs)]);
    await wait(500);
  } finally {
    window.removeEventListener('message', onMessage);
    frame.remove();
  }
  return judgeKit(seen);
}

/**
 * Run the spike once at start-up when the build asks for it, and hand the
 * report to Rust, which writes it to a file. This is how the spike runs on a
 * machine nobody is sitting at: `VITE_SYNVISUAL_SPIKE=1` at build time.
 */
export async function autorunVisualSpike(): Promise<void> {
  if (import.meta.env.VITE_SYNVISUAL_SPIKE !== '1') return;
  try {
    await wait(3000);
    const report = await runVisualSpike();
    await invoke('visual_spike_report', { report });
  } catch (error) {
    console.error('[visual-spike] could not run', error);
  }
}
