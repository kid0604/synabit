import { describe, it, expect } from 'vitest';

import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import app from '../../../App.vue?raw';
import quickEntry from '../../../QuickEntry.vue?raw';
import selectionButton from '../SelectionAskSyn.vue?raw';
import editor from '../../../mini-apps/note/TiptapEditor.vue?raw';

/**
 * Every way into the ask bar that is not the key.
 *
 * The key was built with the locks and the Syn switch in front of it, and a
 * test that says so (`whenTheScreenIsRead.spec.ts`). Each new way in is a new
 * chance to walk around them — a button in the chrome that opened the bar over
 * the PIN screen would be the exact hole the first version of the key had. So
 * each one is checked here for going through the same gate.
 */
describe('the ways into the ask bar', () => {
  const between = (source: string, start: string, end = '\n};') =>
    source.split(start)[1]?.split(end)[0] ?? '';

  it('opens from the chrome only where the key would', () => {
    // Desktop sidebar and mobile bar both.
    const buttons = [...app.matchAll(/<button[^>]*@click="toggleAskBar"[^>]*>/g)].map((m) => m[0]);
    expect(buttons.length, 'a Syn button on desktop and one on mobile').toBe(2);
    for (const button of buttons) expect(button).toContain('askBarAllowed');
  });

  it('goes through openAskBar from the chrome, so the screen is read the same way', () => {
    expect(between(app, 'const toggleAskBar = () => {')).toContain('openAskBar()');
  });

  it('refuses a selection when the key would be refused', () => {
    expect(between(app, 'const onAskAbout = (e: Event) => {')).toContain('askBarAllowed.value');
  });

  it('routes a quick-entry question by the same gate', () => {
    expect(between(app, 'const collectQuickQuestion = async () => {')).toContain(
      'allowed: askBarAllowed.value',
    );
  });

  /** The toolbar and the selection button ask `App.vue` rather than deciding. */
  it('lets the buttons in mini-apps ask the shell whether they may appear', () => {
    expect(app).toContain('provide(SYN_ASK, { allowed: askBarAllowed');
    expect(selectionButton).toContain('inject(SYN_ASK');
    expect(editor).toContain('inject(SYN_ASK');
  });

  /**
   * The words travel by command, not on the event: every webview hears events,
   * including the browsing pane. See `QuickQuestion` in `capture.rs`.
   */
  it('never puts a quick-entry question on an event', () => {
    expect(app).toContain("invoke<string | null>('take_quick_question')");
    expect(quickEntry).toContain("invoke('ask_syn_from_quick_entry'");
    expect(quickEntry).not.toMatch(/emit(To)?\(/);
  });

  it('keeps capture the default in the quick-entry box', () => {
    expect(quickEntry).toContain("const mode = ref<QuickEntryMode>('capture')");
  });
});

describe('what the new ways in say', () => {
  const synKeys = [
    'open_ask_bar',
    'open_ask_bar_hint',
    'ask_about_selection',
    'ask_about_selection_hint',
    'ask_about_block',
  ];
  const quickKeys = [
    ...new Set([...quickEntry.matchAll(/t\('quickcap\.([a-z_]+)'/g)].map((m) => m[1])),
  ];

  it('reads the quick-entry keys it expects to', () => {
    expect(quickKeys.length).toBeGreaterThan(5);
  });

  it('has every key in both languages', () => {
    for (const key of synKeys) {
      expect(en.syn, `en is missing syn.${key}`).toHaveProperty(key);
      expect(vi.syn, `vi is missing syn.${key}`).toHaveProperty(key);
    }
    for (const key of quickKeys) {
      expect(en.quickcap, `en is missing quickcap.${key}`).toHaveProperty(key);
      expect(vi.quickcap, `vi is missing quickcap.${key}`).toHaveProperty(key);
    }
  });

  /** A tooltip with `{shortcut}` in it and nothing behind it shows the braces. */
  it('wants the shortcut in both languages', () => {
    for (const key of ['open_ask_bar_hint', 'ask_about_selection_hint'] as const) {
      expect(en.syn[key]).toContain('{shortcut}');
      expect(vi.syn[key]).toContain('{shortcut}');
    }
  });
});
