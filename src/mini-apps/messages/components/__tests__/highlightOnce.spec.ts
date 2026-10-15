import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { marked } from 'marked';
import { i18n } from '../../../../i18n';

/**
 * A code block is highlighted once, however many bubbles have been drawn.
 *
 * The highlighter used to be added to the shared `marked` inside
 * `<script setup>`, once per bubble. Opening a conversation after a few others
 * ran every code block through hundreds of highlighters, each escaping the
 * last one's HTML — `sudo xattr …` drawn as `&amp;amp;amp;…`, and on
 * 2026-10-15 the WebView held at full CPU and 5 GB. See `messageMarkdown`.
 */

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async () => []),
  convertFileSrc: (p: string) => p,
}));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));
vi.mock('../../../../composables/useNodeService', () => ({
  useNodeService: () => ({ writeNode: vi.fn() }),
}));

import MessageBubble from '../MessageBubble.vue';

const ANSWER = 'Tìm thấy rồi:\n\n```bash\nsudo xattr -r -d com.apple.quarantine /Applications/Synabit.app\n```';

const bubble = async (id: string) => {
  const wrapper = mount(MessageBubble, {
    props: { message: { id, role: 'assistant', content: ANSWER, timestamp: '' } },
    global: { plugins: [i18n] },
  });
  await flushPromises();
  return wrapper;
};

describe('drawing a conversation', () => {
  it('highlights a code block once, in the fortieth bubble as in the first', async () => {
    const first = await bubble('m0');
    const once = first.find('pre code').html();
    first.unmount();

    for (let i = 1; i < 40; i++) (await bubble(`m${i}`)).unmount();

    const fortieth = await bubble('m40');
    const code = fortieth.find('pre code');
    expect(code.text()).toBe('sudo xattr -r -d com.apple.quarantine /Applications/Synabit.app');
    expect(code.html()).not.toContain('&amp;');
    expect(code.html()).toBe(once);
    fortieth.unmount();
  });

  it('leaves the shared `marked` as other screens expect it', async () => {
    (await bubble('m')).unmount();
    const html = marked.parse('```bash\necho "a & b"\n```') as string;
    expect(html, 'not highlighted by the chat').not.toContain('hljs');
  });
});
