import { describe, it, expect } from 'vitest';
import { deriveTitle, extractTags, maskCode, removeTagFromContent, stripColorComment } from '../parsing';
import grammar from '../../../../contracts/tag-grammar.json';

/**
 * What counts as a tag is a product decision, and it is the decision that
 * decides what happens to a user's own sentences. Two things depend on
 * getting it written down rather than inferred from a regex:
 *
 *   - the migration that rewrites every existing cap reads tags with this
 *     grammar, so a change here silently changes thousands of files;
 *   - `properties.tags` is populated from it, so a false positive becomes
 *     a permanent entry in the vault rather than a wrong-looking chip.
 *
 * The cases themselves live in `contracts/tag-grammar.json`, not here,
 * because the grammar has a second implementation: the migration reads
 * tags off bytes on disk, in Rust. `src-tauri/src/utils/tag_grammar.rs`
 * runs this same file. Neither side owns the grammar, so neither side can
 * drift from it without going red.
 *
 * Adding a case to the fixture is how you change the grammar. Changing one
 * should take an argument.
 */
describe('extractTags — the shared grammar', () => {
  it('has a fixture that actually loaded', () => {
    expect(grammar.cases.length).toBeGreaterThanOrEqual(30);
  });

  for (const testCase of grammar.cases) {
    it(testCase.name, () => {
      expect(extractTags(testCase.input)).toEqual(testCase.tags);
    });
  }
});

/**
 * Below here is TypeScript's own business. The migration has no use for
 * masking helpers or for tag deletion — it reads a grammar and writes
 * frontmatter — so these have no counterpart in Rust and no place in the
 * shared fixture.
 */

describe('maskCode', () => {
  it('replaces code with the same number of characters', () => {
    const src = 'a `code` b';
    expect(maskCode(src)).toHaveLength(src.length);
  });

  it('leaves prose untouched', () => {
    expect(maskCode('không có code')).toBe('không có code');
  });
});

describe('stripColorComment', () => {
  it('removes the marker and the newline it sat on', () => {
    expect(stripColorComment('<!--color:red-->\nnội dung')).toBe('nội dung');
  });

  it('leaves a cap without one unchanged', () => {
    expect(stripColorComment('nội dung')).toBe('nội dung');
  });
});

describe('removeTagFromContent', () => {
  /**
   * Deleting a tag is the one moment QuickCap is allowed to edit the body,
   * so it has to touch exactly the tag and nothing else. The regression
   * this guards is the old behaviour: removal collapsed blank lines the
   * user had typed, quietly reflowing the note.
   */
  it('removes the tag and the space that introduced it', () => {
    expect(removeTagFromContent('họp lúc 3h #công-việc', 'công-việc')).toBe('họp lúc 3h');
  });

  it('removes a wrapped tag', () => {
    expect(removeTagFromContent('ghi chú #kế hoạch quý bốn#', 'kế hoạch quý bốn')).toBe('ghi chú');
  });

  it('leaves other tags in place', () => {
    expect(removeTagFromContent('#giữ #bỏ #giữ-nữa', 'bỏ')).toBe('#giữ #giữ-nữa');
  });

  it('leaves a tag whose name merely starts the same', () => {
    expect(removeTagFromContent('#dự-án #dự-án-lớn', 'dự-án')).toBe('#dự-án-lớn');
  });

  it('keeps the paragraph break the user typed', () => {
    expect(removeTagFromContent('đoạn một\n\nđoạn hai #thẻ', 'thẻ')).toBe('đoạn một\n\nđoạn hai');
  });

  it('treats a tag containing regex characters literally', () => {
    expect(removeTagFromContent('ghi #a.b(c) xong', 'a.b(c)')).toBe('ghi xong');
  });
});

describe('deriveTitle', () => {
  /**
   * FTS weights title ten times heavier than body, so this rule decides
   * what search is strongest at finding. It is recomputed on every write,
   * which is the fix for the title that used to freeze at creation.
   */
  it('takes the first line that has anything in it', () => {
    expect(deriveTitle('họp với Minh\n\nchi tiết ở dưới')).toBe('họp với Minh');
  });

  it('skips leading blank lines', () => {
    expect(deriveTitle('\n\n  \nnội dung thật')).toBe('nội dung thật');
  });

  it('ignores the legacy colour marker', () => {
    expect(deriveTitle('<!--color:red-->\nhọp với Minh')).toBe('họp với Minh');
  });

  it('trims a long line rather than cutting the cap off at fifty', () => {
    const long = 'a'.repeat(200);
    const title = deriveTitle(long);
    expect(title).toHaveLength(81);
    expect(title.endsWith('…')).toBe(true);
  });

  it('leaves a line that fits exactly as it is', () => {
    expect(deriveTitle('ngắn gọn')).toBe('ngắn gọn');
  });

  it('names an empty cap rather than returning nothing', () => {
    expect(deriveTitle('')).toBe('Untitled');
    expect(deriveTitle('\n \n')).toBe('Untitled');
  });

  it('counts characters, not bytes, so Vietnamese is not cut mid-word', () => {
    const title = deriveTitle('đ'.repeat(100));
    expect([...title]).toHaveLength(81);
  });
});
