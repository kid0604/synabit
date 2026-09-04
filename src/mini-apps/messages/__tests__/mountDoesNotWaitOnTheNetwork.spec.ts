import { describe, it, expect } from 'vitest';
import source from '../MessagesApp.vue?raw';

/**
 * Opening Messages must not wait for the provider to answer.
 *
 * It used to. `onMounted` awaited `checkStatus` and then `fetchModels` before
 * touching the vault, and `list_models` was built on the generation HTTP
 * client — a five-minute timeout with no connect timeout. An endpoint that
 * accepted the connection and then went quiet left the whole app behind a
 * spinner reading "No chat selected", with nothing clickable.
 *
 * It looked like a busy loop and was not. A Web Inspector timeline of fifteen
 * seconds in that state was completely empty: no layout, no JavaScript, no
 * CPU. The main thread was idle, waiting on an IPC call that had not come
 * back. That empty timeline is what turned the diagnosis around after three
 * wrong guesses about rendering cost.
 *
 * Two things were fixed; this guards the one a refactor could undo. The other
 * — `catalogue_client` in `syn/provider/mod.rs` — is a timeout, and a timeout
 * cannot be reordered by accident.
 */
describe('opening Messages', () => {
  const mount = source.split('onMounted(async () => {')[1]?.split('\n});')[0] ?? '';

  it('has a mount block to read', () => {
    expect(mount, 'MessagesApp should still have an onMounted').toBeTruthy();
    expect(mount).toContain('loading.value = false');
  });

  it('reads the vault before it tries the network', () => {
    const beforeLoadingCleared = mount.slice(0, mount.indexOf('loading.value = false'));

    expect(
      beforeLoadingCleared,
      'the conversation must load before the spinner is cleared',
    ).toContain('initConversation');

    for (const networkCall of ['checkStatus', 'fetchModels']) {
      expect(
        beforeLoadingCleared.includes(networkCall),
        `\`${networkCall}\` reaches the provider, so awaiting it before clearing ` +
          `\`loading\` puts the whole screen behind the network again`,
      ).toBe(false);
    }
  });

  it('still reaches the provider, just not in the way of the screen', () => {
    const afterLoadingCleared = mount.slice(mount.indexOf('loading.value = false'));
    expect(afterLoadingCleared).toContain('checkStatus');
    expect(afterLoadingCleared).toContain('fetchModels');
  });
});
