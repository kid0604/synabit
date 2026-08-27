import { describe, it, expect } from 'vitest';
import { foldDiacritics, looseIncludes } from '../diacritics';

describe('foldDiacritics', () => {
  it('drops Vietnamese tone marks', () => {
    expect(foldDiacritics('công ty cổ phần')).toBe('cong ty co phan');
    expect(foldDiacritics('hoá đơn tháng này')).toBe('hoa don thang nay');
  });

  it('folds đ, because the search index now does too', () => {
    // SQLite will not fold it — it is a letter, not a mark — so the index
    // carries a shadow column of the `đ` words instead. This pass has to agree
    // with that, or the list fills and then rearranges itself.
    expect(foldDiacritics('đông')).toBe('dong');
    expect(foldDiacritics('Đà Nẵng')).toBe('Da Nang');
  });

  it('leaves text without marks exactly as it was', () => {
    expect(foldDiacritics('splunk query')).toBe('splunk query');
  });
});

describe('looseIncludes', () => {
  it('matches a term typed without its marks', () => {
    expect(looseIncludes('Công ty cổ phần ABC', 'cong ty')).toBe(true);
    expect(looseIncludes('hoá đơn', 'hoa')).toBe(true);
  });

  it('matches a term typed with its marks', () => {
    expect(looseIncludes('Công ty cổ phần ABC', 'công')).toBe(true);
  });

  it('ignores case on both sides', () => {
    expect(looseIncludes('Splunk Query', 'splunk')).toBe(true);
    expect(looseIncludes('splunk query', 'SPLUNK')).toBe(true);
  });

  it('still says no when the term is not there', () => {
    expect(looseIncludes('công ty', 'ngân hàng')).toBe(false);
  });

  it('matches a stroked đ typed as a plain d', () => {
    // The whole reason for the shadow column: this is how people type.
    expect(looseIncludes('đông dương', 'dong duong')).toBe(true);
    expect(looseIncludes('đơn hàng', 'don hang')).toBe(true);
  });
});
