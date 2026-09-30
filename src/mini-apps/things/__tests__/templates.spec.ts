import { describe, it, expect } from 'vitest';
import { KIND_TEMPLATES, asKey, kindFromTemplate, templateCreates, templateKindNames, existingTemplateKind } from '../templates';
import { DECLARABLE_KINDS } from '../../../shared/fieldValue';

/** A translator over a small dictionary, echoing the key when it has none. */
const translator = (words: Record<string, string>) => (key: string) => words[key] ?? key;

const book = KIND_TEMPLATES.find(t => t.id === 'book')!;

describe('kind templates', () => {
  it('offers the six collections, each with fields a person could declare', () => {
    expect(KIND_TEMPLATES.map(t => t.id)).toEqual(['book', 'recipe', 'student', 'stock', 'client', 'reference']);
    for (const template of KIND_TEMPLATES) {
      expect(template.fields.length).toBeGreaterThan(0);
      for (const field of template.fields) expect(DECLARABLE_KINDS).toContain(field.kind);
    }
  });

  it('becomes a kind named and shaped in the words of the app language', () => {
    const en = translator({
      'things.templates.book.kind': 'book',
      'things.templates.book.fields.author': 'author',
      'things.templates.book.fields.status': 'status',
      'things.templates.book.fields.rating': 'rating',
      'things.templates.book.fields.finished': 'finished date',
    });
    expect(kindFromTemplate(book, en)).toEqual({
      nodeType: 'book',
      fields: [
        { key: 'author', kind: 'text' },
        { key: 'status', kind: 'text' },
        { key: 'rating', kind: 'number' },
        { key: 'finished_date', kind: 'date' },
      ],
    });
  });

  it('writes Vietnamese as one word per key, so a query can name it', () => {
    const vi = translator({
      'things.templates.book.kind': 'Sách',
      'things.templates.book.fields.author': 'tác giả',
      'things.templates.book.fields.status': 'trạng thái',
      'things.templates.book.fields.rating': 'đánh giá',
      'things.templates.book.fields.finished': 'ngày  đọc xong ',
    });
    const { nodeType, fields } = kindFromTemplate(book, vi);
    expect(nodeType).toBe('sách');
    expect(fields.map(f => f.key)).toEqual(['tác_giả', 'trạng_thái', 'đánh_giá', 'ngày_đọc_xong']);
    for (const f of fields) expect(f.key).not.toMatch(/\s/);
  });

  it('leaves out a field whose translation is blank, repeated, or a key the app governs', () => {
    const odd = translator({
      'things.templates.book.kind': 'book',
      'things.templates.book.fields.author': 'name',
      'things.templates.book.fields.status': 'Name',
      'things.templates.book.fields.rating': '  ',
      'things.templates.book.fields.finished': 'title',
    });
    expect(kindFromTemplate(book, odd).fields).toEqual([{ key: 'name', kind: 'text' }]);
  });

  it('opens a kind that already exists rather than redeclaring it', () => {
    expect(templateCreates('book', ['note', 'task'])).toBe(true);
    expect(templateCreates('book', ['note', 'book'])).toBe(false);
    expect(templateCreates('', [])).toBe(false);
  });

  it('recognises a kind it made whatever the case', () => {
    expect(templateCreates('book', ['Book'])).toBe(false);
    expect(existingTemplateKind(['sách'], ['note', 'Sách'])).toBe('Sách');
  });

  it('recognises a kind it made in the other language, so a switch does not make a second one', () => {
    const en = translator({ 'things.templates.book.kind': 'Book' });
    const vi = translator({ 'things.templates.book.kind': 'Sách' });
    const names = templateKindNames(book, [en, vi]);
    expect(names).toEqual(['book', 'sách']);
    // Made in English, picked again in Vietnamese: the English one opens.
    expect(existingTemplateKind(['sách', ...names], ['note', 'book'])).toBe('book');
    // Decomposed Unicode, as some filesystems hand names back, is the same word.
    expect(existingTemplateKind(names, ['sa\u0301ch'.normalize('NFD')])).toBe('sa\u0301ch'.normalize('NFD'));
    expect(existingTemplateKind(names, ['note'])).toBeNull();
  });

  it('makes keys the way the designer makes names, plus underscores for spaces', () => {
    expect(asKey('  Next Step Date ')).toBe('next_step_date');
  });
});
