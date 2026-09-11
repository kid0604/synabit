import { describe, it, expect, vi, beforeEach } from 'vitest';

let listed: Array<{ name: string }> = [];
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === 'syn_list_models') return listed;
    if (cmd === 'syn_get_settings') return { default_model: null };
    return undefined;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));

const { useSynModels } = await import('../useSynModels');

const model = (name: string) => ({
  name, model: name, size: 0, digest: '', modified_at: '', details: null,
});

beforeEach(() => { listed = []; });

/**
 * The chat header went on saying `gpt-5.6-luna` after Settings had been
 * switched to Gemini, and every message went to Gemini asking for it — a 404
 * each time, under a header that looked perfectly normal.
 */
describe('the model in the chat header, across a change of provider', () => {
  it('chooses again when the new provider does not have it', async () => {
    const m = useSynModels();
    listed = [model('gpt-5.6-luna'), model('gpt-4.1-mini')];
    await m.fetchModels('/vault');
    expect(m.selectedModel.value).toBe('gpt-5.6-luna');

    listed = [model('gemini-3.8-flash'), model('gemini-3.5-flash-lite')];
    await m.fetchModels('/vault');
    expect(m.selectedModel.value).toBe('gemini-3.8-flash');
  });

  it('keeps a choice the provider still offers', async () => {
    const m = useSynModels();
    listed = [model('a'), model('b')];
    await m.fetchModels('/vault');
    m.selectedModel.value = 'b';

    await m.fetchModels('/vault');
    expect(m.selectedModel.value).toBe('b');
  });

  /**
   * An empty list is a failed fetch or a missing key. Throwing away somebody's
   * choice because the network hiccupped would be worse than keeping it.
   */
  it('keeps a choice when there is nothing to judge it by', async () => {
    const m = useSynModels();
    listed = [model('gpt-5.6-luna')];
    await m.fetchModels('/vault');

    listed = [];
    await m.fetchModels('/vault');
    expect(m.selectedModel.value).toBe('gpt-5.6-luna');
  });
});

/**
 * Opening a conversation used to copy its model into the header whatever the
 * provider. One old OpenAI conversation opened after switching to Gemini, and
 * the next message asked Gemini for `gpt-5.6-luna` — a 404, which is how the
 * first Gemini conversation in the vault began.
 */
describe('opening a conversation from another provider', () => {
  it('takes its model only when the provider in use has it', async () => {
    const app = (await import('../../MessagesApp.vue?raw')).default;
    const body = app.slice(app.indexOf('const loadConversation'), app.indexOf('const loadConversation') + 1600);
    expect(body).toContain('models.value.some(m => m.name === pinned)');
    expect(body, 'no unconditional copy of the pin').not.toMatch(/if \(full\.meta\.model\) \{\s*selectedModel\.value = full\.meta\.model;/);
  });
});
