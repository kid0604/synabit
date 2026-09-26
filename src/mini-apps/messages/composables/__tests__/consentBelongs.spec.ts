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
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => true) }));

const { useSynConsent } = await import('../useSynConsent');
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
    expect(a.pending).toBe(b.pending);
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
    expect(pendingIn('conv-a')).toBeNull();
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

describe('carrying on after the answer', () => {
  /** Into the conversation that stopped, not the one on screen. */
  it('resumes Messages into the question’s own conversation', () => {
    expect(app).toContain('const id = consentPending.value?.conversation_id;');
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
    expect(askBar).toContain('resume_run: resumeRun');
  });
});
