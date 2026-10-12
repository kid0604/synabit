import { describe, it, expect, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { i18n } from '../../../../i18n';

const invoke = vi.hoisted(() => vi.fn(async () => []));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import ConnectorProposalCard from '../ConnectorProposalCard.vue';

describe('A connector Syn found or drafted, in the conversation', () => {
  it('is the connect card, and a no is an answer', async () => {
    const w = mount(ConnectorProposalCard, { props: { vaultPath: '/v', connectorId: 'gone' }, global: { plugins: [i18n] } });
    await flushPromises();
    expect(w.find('[data-test="connect-card"]').exists()).toBe(true);
    expect(w.find('[data-test="connect-missing"]').exists()).toBe(true);
  });
});
