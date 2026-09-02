import { describe, it, expect } from 'vitest';
import { propertiesTable, withProperties } from '../exportMarkdown';

describe('what an export carries out of a node', () => {
  /**
   * The reason this exists. The Notes exporter writes the title and the body,
   * which for a book is a title and a blank page.
   */
  it('carries the fields that are the substance of a record', () => {
    const table = propertiesTable([
      { key: 'author', value: 'Harari' },
      { key: 'read_at', value: '2025-05-26' },
    ]);

    expect(table).toContain('| Author | Harari |');
    expect(table).toContain('| Read at | 2025-05-26 |');
  });

  /**
   * A note exported through Things must be the same file it is through Notes.
   * Tags are the export dialog's own option, so counting them here would both
   * duplicate them and give every note a one-row table it never had.
   */
  it('leaves a note exactly as the Notes exporter leaves it', () => {
    expect(propertiesTable([{ key: 'tags', value: '["mdp","network"]' }])).toBe('');
    expect(withProperties('Mất gói ở chặng hai.', [{ key: 'tags', value: '["mdp"]' }]))
      .toBe('Mất gói ở chặng hai.');
  });

  it('skips a field that is empty rather than printing a blank row', () => {
    expect(propertiesTable([{ key: 'author', value: '' }])).toBe('');
    expect(propertiesTable([{ key: '', value: 'orphan' }])).toBe('');
  });

  /** A pipe in a value would end the cell early and shift every column after it. */
  it('escapes a value that would break the table', () => {
    expect(propertiesTable([{ key: 'note', value: 'a | b' }]))
      .toContain('| Note | a \\| b |');
  });

  it('puts the table above the body without either forcing the other', () => {
    const rows = [{ key: 'species', value: 'Mèo' }];

    expect(withProperties('Nhặt được ở ngõ.', rows)).toBe(
      '| | |\n| --- | --- |\n| Species | Mèo |\n\nNhặt được ở ngõ.',
    );
    // A record with no body is the ordinary case for an animal.
    expect(withProperties('', rows)).toBe('| | |\n| --- | --- |\n| Species | Mèo |');
    expect(withProperties('', [])).toBe('');
  });
});
