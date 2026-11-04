import { describe, it, expect, afterEach } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import en from '../../../../i18n/locales/en.json';
import vi from '../../../../i18n/locales/vi.json';

import ConsentCard from '../ConsentCard.vue';
import type { ConsentAsk } from '../../types';

/**
 * The permission card while its answer is being sent (review §5, U7).
 *
 * The buttons stayed live, so a second press answered a question the first
 * had already closed; and an answer that failed failed silently.
 */

const ask: ConsentAsk = {
  capability: 'VaultRead',
  tool: 'read_note',
  about: 'read your vault',
  can_be_remembered: true,
  asked_at: '',
};

const mounted: VueWrapper<any>[] = [];
afterEach(() => {
  mounted.splice(0).forEach((w) => w.unmount());
});

const card = (props: { busy?: boolean; error?: string | null }) => {
  const wrapper = mount(ConsentCard, { props: { ask, ...props }, global: { plugins: [i18n] } });
  mounted.push(wrapper);
  return wrapper;
};

describe('the permission card', () => {
  it('holds its buttons while the answer is on its way', () => {
    const buttons = card({ busy: true }).findAll('button');
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) expect(button.attributes('disabled')).toBeDefined();
  });

  it('says so when the answer did not go through', () => {
    const wrapper = card({ error: 'disk full' });
    const said = wrapper.find('[data-card-error]');
    expect(said.exists()).toBe(true);
    expect(said.attributes('role')).toBe('alert');
    expect(said.text()).toContain('disk full');
    for (const button of wrapper.findAll('button')) {
      expect(button.attributes('disabled'), 'and can be answered again').toBeUndefined();
    }
  });

  // A folder is answered by choosing it, in a dialog Rust puts up: no
  // "always", nothing that would name a path, and "not now" to decline.
  it('asks for a folder by having one chosen', async () => {
    const wrapper = mount(ConsentCard, {
      props: { ask: { ...ask, capability: 'Door', tool: 'machine_door', can_be_remembered: false, per_call: true } },
      global: { plugins: [i18n] },
    });
    mounted.push(wrapper);
    expect(wrapper.text()).toContain(en.syn.consent_door.split('.')[0]);
    expect(wrapper.find('[data-test="consent-session"]').exists()).toBe(false);
    expect(wrapper.find('input').exists()).toBe(false);
    await wrapper.find('[data-test="consent-pick-folder"]').trigger('click');
    await wrapper.find('[data-test="consent-not-now"]').trigger('click');
    expect(wrapper.emitted('answer')).toEqual([['pick_folder'], ['not_now']]);
    for (const lang of [en, vi]) {
      expect(lang.syn.consent_door).toBeTruthy();
      expect(lang.syn.consent_pick_folder).toBeTruthy();
      for (const why of ['not_a_folder', 'too_wide', 'vault', 'app_data', 'library', 'hidden']) {
        expect(lang.syn, why).toHaveProperty(`door_refused_${why}`);
      }
    }
  });

  it('says it in both languages, with the reason', () => {
    expect(en.syn.consent_failed).toContain('{reason}');
    expect(vi.syn.consent_failed).toContain('{reason}');
  });
});
