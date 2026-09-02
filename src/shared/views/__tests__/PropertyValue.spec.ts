import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import PropertyValue from '../PropertyValue.vue';
import type { FieldKind } from '../../fieldValue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        value_yes: 'Yes', value_no: 'No',
        field_value: 'Value', remove_field: 'Remove',
      },
    },
  },
});

const draw = (kind: FieldKind, modelValue: string) =>
  mount(PropertyValue, { props: { kind, modelValue }, global: { plugins: [i18n] } });

/** Which control a kind produces, named the way somebody looking would name it. */
const control = (kind: FieldKind, value: string) => {
  const w = draw(kind, value);
  if (w.find('input[type="date"]').exists()) return 'date';
  if (w.find('input[type="number"]').exists()) return 'number';
  if (w.find('input[type="text"]').exists()) return 'text';
  if (w.findAll('button').length) return 'chips-or-switch';
  return 'none';
};

describe('drawing a value by its kind', () => {
  it('gives each kind its own control once there is a value', () => {
    expect(control('date', '2026-07-09')).toBe('date');
    expect(control('number', '5')).toBe('number');
    expect(control('boolean', 'true')).toBe('chips-or-switch');
    expect(control('list', '["mdp"]')).toBe('chips-or-switch');
    expect(control('text', 'chưa xong')).toBe('text');
  });

  /**
   * The kind has to survive the value being empty, because that is exactly
   * when a declared kind is the only thing there is to go on.
   *
   * A list was the one that did not: `items` refuses anything that does not
   * start with `[`, so an unfilled list fell through to a text box and asked
   * for JSON punctuation by hand — the very thing chips exist to end.
   */
  it('keeps its control when the value is empty', () => {
    expect(control('date', '')).toBe('date');
    expect(control('number', '')).toBe('number');
    expect(control('boolean', '')).toBe('chips-or-switch');
    expect(control('list', ''), 'an unfilled list is an empty list').toBe('chips-or-switch');
    expect(control('text', '')).toBe('text');
  });

  it('offers to add the first item to an empty list', async () => {
    const wrapper = draw('list', '');
    const add = wrapper.findAll('button')[0];
    await add.trigger('click');

    const box = wrapper.find('input');
    await box.setValue('mdp');
    await box.trigger('keydown.enter');

    const emitted = wrapper.emitted('update:modelValue') ?? [];
    expect(emitted[emitted.length - 1]).toEqual(['["mdp"]']);
  });

  /**
   * A merge between two devices can leave anything in a field. Malformed JSON
   * must still fall back to text — that is how somebody repairs it — and the
   * empty case above must not take that fallback away.
   */
  it('falls back to text for a list that is not a list', () => {
    expect(control('list', '["a",')).toBe('text');
    expect(control('list', 'mdp, network')).toBe('text');
  });

  /** An empty boolean is off, not a third state. */
  it('reads an empty boolean as off', () => {
    expect(draw('boolean', '').text()).toContain('No');
  });
});
