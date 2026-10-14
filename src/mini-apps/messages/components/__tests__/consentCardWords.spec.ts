import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { i18n } from '../../../../i18n';
import ConsentCard from '../ConsentCard.vue';
import type { ConsentAsk } from '../../types';

/**
 * The card says what Syn is doing in words, never by the tool's own name —
 * `connector__jira__create_issue` is the model's vocabulary, not the person's.
 */
describe('The permission card', () => {
  it('names the work, not the tool', () => {
    i18n.global.locale.value = 'en';
    const ask: ConsentAsk = {
      capability: { NetWrite: { domain: 'Jira', tool: 'create_issue' } },
      tool: 'connector__jira__create_issue',
      about: '',
      can_be_remembered: true,
      asked_at: '',
    } as ConsentAsk;
    const text = mount(ConsentCard, { props: { ask }, global: { plugins: [i18n] } }).text();
    expect(text).not.toContain('connector__');
    expect(text).not.toContain('create_issue');
    expect(text).toContain('create issue');
    expect(text).toContain('Jira');
  });

  /** A command on someone's machine is shown exactly as it will be sent. */
  it('shows the command line an ssh connector would send', () => {
    i18n.global.locale.value = 'en';
    const ask = {
      capability: { NetWrite: { domain: 'Raspberry Pi', tool: 'restart_service' } },
      tool: 'connector__pi__restart_service',
      about: '',
      can_be_remembered: true,
      asked_at: '',
      detail: "sudo systemctl restart 'my app'",
    } as ConsentAsk;
    const w = mount(ConsentCard, { props: { ask }, global: { plugins: [i18n] } });
    expect(w.find('[data-test="consent-detail"]').text()).toBe("sudo systemctl restart 'my app'");
  });

  /** One command line: not now, this time, this line for the session — never "never". */
  it('asks about one command with not now / this time / this session', async () => {
    i18n.global.locale.value = 'en';
    const ask = {
      capability: { NetWrite: { domain: 'Raspberry Pi', tool: 'run' } },
      tool: 'connector__pi__run',
      about: '',
      can_be_remembered: false,
      asked_at: '',
      detail: 'pkill -f openclaw-gateway',
      per_call: true,
    } as ConsentAsk;
    const w = mount(ConsentCard, { props: { ask }, global: { plugins: [i18n] } });
    expect(w.text()).not.toContain('Never');
    expect(w.find('[data-test="consent-not-now"]').text()).toBe('Not now');
    expect(w.find('[data-test="consent-this-time"]').text()).toBe('Allow this time');
    expect(w.find('[data-test="consent-session"]').text()).toBe('Always allow in this session');
    await w.find('[data-test="consent-not-now"]').trigger('click');
    await w.find('[data-test="consent-session"]').trigger('click');
    await w.find('[data-test="consent-this-time"]').trigger('click');
    expect(w.emitted('answer')).toEqual([['not_now'], ['session'], ['once']]);
  });
});
