import { describe, expect, it } from 'vitest';
import { TEMPLATES } from '../templates';
import { SHAPES_MAP } from '../shapes';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

/** Look a dotted key up in a locale file; undefined when it is missing. */
const lookup = (locale: any, key: string): unknown => key.split('.').reduce((at, part) => at?.[part], locale);

describe.each(TEMPLATES.map((tpl) => [tpl.id, tpl] as const))('template %s', (id, tpl) => {
  const used: string[] = [];
  const built = tpl.build((key) => {
    used.push(key);
    return key;
  });

  it('puts something on the board, every item with an id of its own', () => {
    expect(built.nodes.length).toBeGreaterThan(0);
    const ids = built.nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('joins only items it made, and draws only shapes the board has', () => {
    const ids = new Set(built.nodes.map((n) => n.id));
    for (const e of built.edges) expect(ids.has(e.source) && ids.has(e.target)).toBe(true);
    for (const n of built.nodes.filter((x) => x.type === 'shape')) expect(SHAPES_MAP[n.data.shapeType]).toBeDefined();
  });

  it('has every word in English and Vietnamese', () => {
    for (const key of [...used, `whiteboard.templates.${id}.name`, `whiteboard.templates.${id}.desc`]) {
      expect(typeof lookup(en, key), `${key} in en`).toBe('string');
      expect(typeof lookup(vi, key), `${key} in vi`).toBe('string');
    }
  });
});
