import { describe, it, expect, afterEach } from 'vitest';
import { applyUiScale, normaliseUiScale, UI_SCALES } from '../uiScale';

afterEach(() => {
  document.documentElement.style.fontSize = '';
});

describe('interface size', () => {
  it('offers nothing below the size the screens were drawn at', () => {
    expect(Math.min(...UI_SCALES.map(s => s.value))).toBe(1);
  });

  it('reads a size that is no longer offered as the default', () => {
    expect(normaliseUiScale(0.9)).toBe(1);
    expect(normaliseUiScale('big')).toBe(1);
    expect(normaliseUiScale(1.3)).toBe(1.3);
  });

  /**
   * Outside Tauri (and on Android) there is no webview zoom. The fallback grows
   * the root font size, never CSS `zoom`, which inflated every `vh` and pushed
   * the phone's tab bar off the screen.
   */
  it('falls back to the root font size, not zoom', async () => {
    await applyUiScale(1.3);
    expect(document.documentElement.style.fontSize).toBe('20.8px');
    expect(document.documentElement.style.zoom).toBe('');
    await applyUiScale(1);
    expect(document.documentElement.style.fontSize).toBe('');
  });
});
