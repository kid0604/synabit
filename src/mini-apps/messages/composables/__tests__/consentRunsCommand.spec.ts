import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * A yes to a command on this Mac goes to the system's dialog, which Rust puts
 * up with the line as it will run (`syn_confirm_command`). The page never sends
 * that yes itself: Rust would refuse it, and a script here could have sent it
 * (A22). A no is the page's to send.
 */

const handlers = new Map<string, (event: { payload: unknown }) => void>();
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  }),
}));
const invoke = vi.fn(async (..._args: unknown[]): Promise<unknown> => true);
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { useSynConsent } = await import('../useSynConsent');

const asking = (run: string) =>
  handlers.get('syn-consent-needed')!({
    payload: { run_id: run, conversation_id: 'conv-run', ask: { tool: 'machine_host', capability: 'Execute', per_call: true } },
  });

describe('answering a card that asks to run a command', () => {
  beforeEach(() => invoke.mockClear());

  it('sends a yes to the system dialog, not to the card', async () => {
    const { answer } = useSynConsent(() => '/vault');
    for (const choice of ['once', 'session', 'always'] as const) {
      asking(`run-${choice}`);
      expect(await answer(choice, 'conv-run')).toBe(true);
    }
    const answers = invoke.mock.calls.filter(([name]) => String(name).startsWith('syn_answer') || name === 'syn_confirm_command');
    expect(answers.map(([name]) => name)).toEqual(['syn_confirm_command', 'syn_confirm_command', 'syn_confirm_command']);
    expect(answers[0][1]).toMatchObject({ vaultPath: '/vault', runId: 'run-once' });
  });

  it('sends a no as any other answer', async () => {
    const { answer } = useSynConsent(() => '/vault');
    asking('run-no');
    await answer('not_now', 'conv-run');
    const answers = invoke.mock.calls.filter(([name]) => String(name).startsWith('syn_answer') || name === 'syn_confirm_command');
    expect(answers.map(([name]) => name)).toEqual(['syn_answer_consent']);
  });
});
