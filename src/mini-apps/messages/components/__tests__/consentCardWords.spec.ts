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
});
