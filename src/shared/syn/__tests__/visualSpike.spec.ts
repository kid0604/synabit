import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { visualOrigin, judge, judgeKit, judgeChat, judgeKeep, summarise, asMarkdown, KIT_HOST_CONTEXT, type SpikeRow, type KitSeen, type ChatSeen, type KeepSeen } from '../visualSpike';

const blocked: SpikeRow = { group: 'exfil', name: 'fetch https', outcome: 'blocked', detail: 'CSP' };

describe('where a visual is served from', () => {
  it('is its own scheme except on Windows', () => {
    expect(visualOrigin('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe('synvisual://localhost');
    expect(visualOrigin('Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15')).toBe('synvisual://localhost');
    expect(visualOrigin('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/130.0')).toBe('http://synvisual.localhost');
  });
});

describe("the host's verdict", () => {
  it('counts a page that never answered as a failure, not as nothing leaked', () => {
    const rows = judge({ results: null, pings: 0, afterForm: false, topNav: null });
    expect(rows).toHaveLength(1);
    expect(summarise(rows, 'ua', 'o').broken).toBe(1);
  });

  it('calls any arrival at the ping command a leak', () => {
    const rows = judge({ results: [blocked], pings: 1, afterForm: true, topNav: { threw: true, name: 'SecurityError' } });
    expect(rows.find((r) => r.name === 'an IPC message reached Rust')?.outcome).toBe('LEAKED');
  });

  it('calls a page that went quiet after its form a leak', () => {
    const rows = judge({ results: [blocked], pings: 0, afterForm: false, topNav: null });
    expect(rows.find((r) => r.name === 'form submission')?.outcome).toBe('LEAKED');
  });

  it('passes a clean run', () => {
    const report = summarise(
      judge({ results: [blocked], pings: 0, afterForm: true, topNav: { threw: true, name: 'SecurityError' } }),
      'ua',
      'synvisual://localhost',
    );
    expect(report.leaked).toBe(0);
    expect(report.rows).toHaveLength(4);
  });

  it('writes a table that survives a pipe in the detail', () => {
    const report = summarise([{ ...blocked, detail: 'a | b' }], 'ua', 'o');
    expect(asMarkdown(report)).toContain('a \\| b');
  });
});

describe('the kit, judged', () => {
  const working: KitSeen = {
    initializeAsked: true,
    initialized: true,
    heights: [0, 214],
    asked: ['Con lắc dài gấp đôi thì chu kỳ đổi thế nào?'],
    errors: [],
    selftest: {
      d3: true, katex: true, katexFont: true, tokens: true,
      themeFromHost: KIT_HOST_CONTEXT.theme, themeOnPage: KIT_HOST_CONTEXT.theme, textColor: '#f4f4f5',
      number: '1.234,5', askAnswered: true,
    },
  };

  it('passes a kit that did everything', () => {
    const rows = judgeKit(working);
    expect(rows.filter((r) => r.outcome !== 'ok')).toEqual([]);
  });

  it('fails a kit that kept the page theme over the host', () => {
    const rows = judgeKit({ ...working, selftest: { ...working.selftest, themeOnPage: 'light' } });
    expect(rows.find((r) => r.name === "the host's theme applied")?.outcome).toBe('error');
  });

  it('fails a kit that reported something broke', () => {
    const rows = judgeKit({ ...working, errors: [{ kind: 'policy', message: 'font-src https://cdn.example' }] });
    expect(summarise(rows, 'ua', 'o').broken).toBe(1);
  });

  it('fails a page that never sent its self-test', () => {
    const rows = judgeKit({ ...working, selftest: null });
    expect(rows.filter((r) => r.outcome === 'error').length).toBeGreaterThan(5);
  });
});

describe("the chat's own frame, judged", () => {
  const working: ChatSeen = {
    skipped: null, framed: true, sandbox: 'allow-scripts', titleShown: true, height: 322,
    selftest: { d3: true, katex: true, katexFont: true, themeOnPage: 'light' }, asked: ['q'], appTheme: 'light', exportStandsAlone: true,
  };

  it('passes a frame that did everything', () => {
    expect(judgeChat(working).filter((r) => r.outcome !== 'ok')).toEqual([]);
  });

  it('fails a frame sandboxed more loosely', () => {
    const rows = judgeChat({ ...working, sandbox: 'allow-scripts allow-same-origin' });
    expect(rows.find((r) => r.name === 'SynVisual framed it')?.outcome).toBe('error');
  });

  it('reports a run with no vault as not run, not as broken', () => {
    const rows = judgeChat({ ...working, skipped: 'not run: no vault is open' });
    expect(rows).toHaveLength(1);
    expect(summarise(rows, 'ua', 'o')).toMatchObject({ broken: 0, unclear: 1 });
  });
});

describe('keeping a visual and opening its note', () => {
  const ID = '0123456789abcdef0123456789abcdef';
  const working: KeepSeen = {
    skipped: null,
    keptLabel: true,
    opened: { id: 'Notes/abc.md', title: 'Bước 0', node_type: 'note' },
    noteText: `---\ntitle: Bước 0\n---\n<div data-type="syn-visual" data-visual="${ID}"></div>\n`,
    id: ID,
    framed: true,
    sandbox: 'allow-scripts',
    selftest: { d3: true, katex: true },
  };

  it('passes a note that holds the block and runs it', () => {
    expect(judgeKeep(working).filter((r) => r.outcome !== 'ok')).toEqual([]);
  });

  it('fails a note that carries the HTML itself', () => {
    const rows = judgeKeep({ ...working, noteText: `${working.noteText}<script>x()</script>` });
    expect(rows.find((r) => r.name.startsWith('the note on disk'))?.outcome).toBe('error');
  });

  it('fails a note that opened without running the visual', () => {
    const rows = judgeKeep({ ...working, selftest: null });
    expect(rows.find((r) => r.name === 'the kit ran inside the note')?.outcome).toBe('error');
  });

  it('reports a run that cannot open a note as not run', () => {
    const rows = judgeKeep({ ...working, skipped: 'not run: only the unattended run can open a note' });
    expect(summarise(rows, 'ua', 'o')).toMatchObject({ broken: 0, unclear: 1 });
  });
});
