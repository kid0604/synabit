import { describe, it, expect, vi, beforeEach } from 'vitest';

import app from '../../MessagesApp.vue?raw';
import askBar from '../../../../shared/syn/AskBar.vue?raw';

/**
 * A question Syn stopped on belongs to the conversation whose run stopped.
 *
 * It used to be shown in whichever conversation was open, and answering it
 * carried the work on there — and the ask bar, which has its own conversation,
 * never saw it at all: the listener lived in Messages alone.
 */

// The one listener each store registers, captured so the test can play the
// backend and emit.
const handlers = new Map<string, (event: { payload: unknown }) => void>();
const listen = vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
  handlers.set(name, handler);
  return () => handlers.delete(name);
});
vi.mock('@tauri-apps/api/event', () => ({ listen }));
const invoke = vi.fn(async (..._args: unknown[]): Promise<unknown> => true);
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { useSynConsent, refreshWaiting } = await import('../useSynConsent');
const { useSynChoice } = await import('../useSynChoice');

describe('where a consent question is shown', () => {
  beforeEach(() => {
    handlers.get('syn-consent-needed')?.({ payload: null });
    handlers.get('syn-choice-needed')?.({ payload: null });
  });

  it('is one store, listened to once, however many surfaces ask', () => {
    const a = useSynConsent(() => '/vault');
    const b = useSynConsent(() => '/vault');
    expect(listen.mock.calls.filter(([name]) => name === 'syn-consent-needed')).toHaveLength(1);
    handlers.get('syn-consent-needed')!({
      payload: { run_id: 'run-0', conversation_id: 'conv-shared', ask: { tool: 'browse' } },
    });
    expect(a.pendingIn('conv-shared')).toBe(b.pendingIn('conv-shared'));
  });

  it('is shown in its own conversation and nowhere else', () => {
    const { pendingIn } = useSynConsent(() => '/vault');
    handlers.get('syn-consent-needed')!({
      payload: { run_id: 'run-1', conversation_id: 'conv-a', ask: { tool: 'browse' } },
    });
    expect(pendingIn('conv-a')?.run_id).toBe('run-1');
    expect(pendingIn('conv-b')).toBeNull();
    expect(pendingIn(null)).toBeNull();
  });

  it('is shown in no conversation when it came from none', () => {
    const { pendingIn } = useSynConsent(() => '/vault');
    handlers.get('syn-consent-needed')!({
      payload: { run_id: 'run-2', conversation_id: null, ask: { tool: 'browse' } },
    });
    expect(pendingIn(null)).toBeNull();
    expect(pendingIn('conv-c')).toBeNull();
  });

  /** U5: two conversations can each be waiting; the second does not replace the first. */
  it('keeps one question per conversation', () => {
    const { pendingIn } = useSynConsent(() => '/vault');
    handlers.get('syn-consent-needed')!({
      payload: { run_id: 'run-routine', conversation_id: 'conv-routine', ask: { tool: 'browse' } },
    });
    handlers.get('syn-consent-needed')!({
      payload: { run_id: 'run-typed', conversation_id: 'conv-typed', ask: { tool: 'browse' } },
    });
    expect(pendingIn('conv-routine')?.run_id).toBe('run-routine');
    expect(pendingIn('conv-typed')?.run_id).toBe('run-typed');
  });

  it('holds for the choice card by the same rule', () => {
    const { pendingIn } = useSynChoice(() => '/vault');
    handlers.get('syn-choice-needed')!({
      payload: { run_id: 'run-3', conversation_id: 'conv-a', choice: { candidates: [] } },
    });
    expect(pendingIn('conv-a')?.run_id).toBe('run-3');
    expect(pendingIn('conv-b')).toBeNull();
  });
});

describe('a question asked while nobody was looking', () => {
  /** U5: read back from the runs on disk, so there is a card to answer it. */
  it('is read back from disk and shown in its conversation', async () => {
    invoke.mockImplementationOnce(async () => [
      { run_id: 'run-7am', conversation_id: 'conv-morning', ask: { tool: 'browse' } },
      { run_id: 'run-which', conversation_id: 'conv-other', choice: { candidates: [] } },
    ]);
    await refreshWaiting('/vault');
    const { pendingIn } = useSynConsent(() => '/vault');
    expect(invoke).toHaveBeenCalledWith('syn_waiting', { vaultPath: '/vault' });
    expect(pendingIn('conv-morning')?.run_id).toBe('run-7am');
    expect(pendingIn('conv-other'), 'a which-one is not a consent').toBeNull();
  });
});

describe('carrying on after the answer', () => {
  /** Into the conversation that stopped, not the one on screen. */
  it('resumes Messages into the question’s own conversation', () => {
    expect(app).toContain('const id = consentHere.value?.conversation_id;');
    const onConsent = app.split('const onConsent')[1]?.split('\n};')[0] ?? '';
    expect(onConsent, 'the resume must not read the open conversation').not.toContain(
      'const id = activeConversationId.value',
    );
    expect(app).toContain(':consent-ask="consentHere?.ask ?? null"');
    expect(app).toContain(':choice-ask="choiceHere?.choice ?? null"');
  });

  /** The ask bar draws both cards for its own conversation, and resumes. */
  it('lets the ask bar answer, too', () => {
    expect(askBar).toContain('<ConsentCard');
    expect(askBar).toContain('<ChoiceCard');
    expect(askBar).toContain('consentPendingIn(conversationId.value)');
    // Carried on through the same composable Messages uses, which sends it as
    // `resume_run`.
    expect(askBar).toContain('resumeRun,');
    expect(askBar).toContain('useSynChat()');
  });
});
