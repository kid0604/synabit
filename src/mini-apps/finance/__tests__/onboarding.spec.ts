import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { i18n } from '../../../i18n';
import FinanceOnboarding from '../FinanceOnboarding.vue';

/**
 * An account typed into the "add another" box and never added with + used to
 * be dropped without a word when Start was pressed.
 */
describe('finance onboarding', () => {
  const start = async (typed: string) => {
    const wrapper = mount(FinanceOnboarding, { global: { plugins: [i18n] } });
    const box = wrapper.findAll('input[type="text"]').find(i => i.attributes('placeholder') === i18n.global.t('finance.add_another_acc'))!;
    await box.setValue(typed);
    const buttons = wrapper.findAll('button');
    await buttons[buttons.length - 1].trigger('click');
    const done = wrapper.emitted('complete')?.[0]?.[0] as { accounts: { name: string }[] } | undefined;
    wrapper.unmount();
    return done;
  };

  it('keeps an account that was typed but not added', async () => {
    const done = await start('Savings');
    expect(done?.accounts.map(a => a.name)).toContain('Savings');
  });

  it('adds nothing when the box is empty', async () => {
    const done = await start('  ');
    expect(done?.accounts).toHaveLength(2);
  });

  it('lets the whole form scroll, so Start is reachable on a small screen', () => {
    const wrapper = mount(FinanceOnboarding, { global: { plugins: [i18n] } });
    expect(wrapper.element.classList.contains('overflow-y-auto')).toBe(true);
    wrapper.unmount();
  });
});
