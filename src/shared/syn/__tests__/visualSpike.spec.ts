import { describe, it, expect, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { visualOrigin, judge, summarise, asMarkdown, type SpikeRow } from '../visualSpike';

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
    expect(summarise(rows, 'ua', 'o').leaked).toBe(1);
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
