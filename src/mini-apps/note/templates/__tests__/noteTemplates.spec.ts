import { describe, it, expect } from 'vitest';
import {
  fillPlaceholders,
  formatTemplateDate,
  instantiateTemplate,
  isTemplatePath,
  userTemplatesFrom,
  filterTemplates,
  builtInTemplates,
  BUILT_IN_TEMPLATE_IDS,
  type NoteTemplate,
} from '../noteTemplates';

// Noon, so no time zone the tests run in moves it to another day.
const NOW = new Date(2026, 8, 30, 12, 0, 0);

describe('fillPlaceholders', () => {
  const ctx = { now: NOW, locale: 'en', title: 'Standup' };

  it('fills the date in the app language', () => {
    expect(fillPlaceholders('{{date}}', ctx)).toBe('September 30, 2026');
    expect(fillPlaceholders('{{date}}', { ...ctx, locale: 'vi' })).toBe(formatTemplateDate(NOW, 'vi'));
    expect(formatTemplateDate(NOW, 'vi')).toMatch(/30/);
    expect(formatTemplateDate(NOW, 'vi')).toMatch(/2026/);
  });

  it('fills the title, and tolerates spaces and capitals inside the braces', () => {
    expect(fillPlaceholders('# {{title}} / {{ Title }} / {{TITLE}}', ctx)).toBe('# Standup / Standup / Standup');
  });

  it('fills the time', () => {
    expect(fillPlaceholders('{{time}}', ctx)).toMatch(/12:00/);
  });

  it('leaves placeholders it does not know exactly as written', () => {
    expect(fillPlaceholders('{{owner}} and {{ date }}', ctx)).toBe('{{owner}} and September 30, 2026');
    expect(fillPlaceholders('{single} {{}}', ctx)).toBe('{single} {{}}');
  });

  it('falls back to English for a locale the runtime cannot read', () => {
    expect(fillPlaceholders('{{date}}', { ...ctx, locale: 'not a locale!' })).toBe('September 30, 2026');
  });
});

describe('instantiateTemplate', () => {
  it('fills the title first and uses the finished title in the body', () => {
    const out = instantiateTemplate(
      { name: 'Meeting notes', titlePattern: 'Meeting — {{date}}' },
      '# {{title}}\nOn {{date}}',
      NOW,
      'en',
    );
    expect(out.title).toBe('Meeting — September 30, 2026');
    expect(out.body).toBe('# Meeting — September 30, 2026\nOn September 30, 2026');
  });

  it('reads {{title}} in the title pattern as the template name', () => {
    expect(instantiateTemplate({ name: 'Brief', titlePattern: '{{title}} draft' }, '', NOW, 'en').title).toBe('Brief draft');
  });
});

describe('isTemplatePath', () => {
  it('finds notes in Templates/, at any depth, on any platform', () => {
    expect(isTemplatePath('Templates/abc.md')).toBe(true);
    expect(isTemplatePath('Templates/Work/abc.md')).toBe(true);
    expect(isTemplatePath('templates/abc.md')).toBe(true);
    expect(isTemplatePath('Templates\\abc.md')).toBe(true);
    expect(isTemplatePath('./Templates/abc.md')).toBe(true);
  });

  it('does not match notes that merely mention the word', () => {
    expect(isTemplatePath('Notes/Templates.md')).toBe(false);
    expect(isTemplatePath('Notes/Templates/abc.md')).toBe(false);
    expect(isTemplatePath('TemplatesOld/abc.md')).toBe(false);
  });
});

describe('userTemplatesFrom', () => {
  it('lists only template notes, by name, with their tags', () => {
    const list = userTemplatesFrom(
      [
        { id: 'Notes/a.md', title: 'Not a template' },
        { id: 'Templates/b.md', title: 'Standup {{date}}', tags: ['work'] },
        { id: 'Templates/c.md', title: 'Anamnesis' },
        { id: 'Templates/d.md', title: '  ' },
      ],
      'Untitled',
    );
    expect(list.map((t) => t.name)).toEqual(['Anamnesis', 'Standup {{date}}', 'Untitled']);
    expect(list[1]).toMatchObject({ key: 'Templates/b.md', source: 'user', titlePattern: 'Standup {{date}}', body: null, tags: ['work'] });
  });
});

describe('filterTemplates', () => {
  const list: NoteTemplate[] = [
    { key: 'a', source: 'builtin', name: 'Nhật ký hằng ngày', description: 'Việc ưu tiên', titlePattern: '', body: '', tags: [] },
    { key: 'b', source: 'builtin', name: 'Meeting notes', description: 'Attendees, agenda', titlePattern: '', body: '', tags: [] },
  ];

  it('returns everything for an empty query', () => {
    expect(filterTemplates(list, '  ')).toHaveLength(2);
  });

  it('matches without diacritics, and on the description', () => {
    expect(filterTemplates(list, 'nhat ky').map((t) => t.key)).toEqual(['a']);
    expect(filterTemplates(list, 'uu tien').map((t) => t.key)).toEqual(['a']);
    expect(filterTemplates(list, 'AGENDA').map((t) => t.key)).toEqual(['b']);
  });

  it('needs every word', () => {
    expect(filterTemplates(list, 'meeting diary')).toEqual([]);
  });
});

describe('builtInTemplates', () => {
  it('hands the locale strings their placeholders back in double braces', () => {
    const t = (key: string, named?: Record<string, unknown>) =>
      key.endsWith('.body') ? `On ${named?.date} for ${named?.title}` : key.endsWith('.title') ? `T ${named?.date}` : key;
    const list = builtInTemplates(t);
    expect(list.map((x) => x.key)).toEqual(BUILT_IN_TEMPLATE_IDS.map((id) => `builtin:${id}`));
    expect(list[0].body).toBe('On {{date}} for {{title}}');
    expect(list[0].titlePattern).toBe('T {{date}}');
    expect(list[0].name).toBe('note.templates.builtin.meeting.name');
  });
});
