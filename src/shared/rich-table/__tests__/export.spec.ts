import { describe, it, expect } from 'vitest';
import { toCsv, toSheetRows, viewGrid } from '../export';
import type { RichTable } from '../model';

const t: RichTable = {
  columns: [
    { name: 'Khoản', type: 'text' }, { name: 'Số tiền', type: 'number' },
    { name: 'Xong', type: 'checkbox' }, { name: 'Thuế', type: 'formula', expr: '[Số tiền] * 0.1' },
  ],
  rows: [['Cà phê, sữa', '45000', '[x]', '4500'], ['Nói "chào"', 'abc', '', '#TYPE']],
  views: [{}],
};

describe('exporting a view', () => {
  it('writes the columns and rows it is given, in that order', () => {
    expect(viewGrid(t, [1, 0], [1, 0])).toEqual([['Số tiền', 'Khoản'], ['abc', 'Nói "chào"'], ['45000', 'Cà phê, sữa']]);
  });

  it('writes CSV Excel can open: quoted, CRLF, with a byte order mark', () => {
    expect(toCsv(viewGrid(t, [0, 1], [0, 1]))).toBe('﻿Khoản,Số tiền\r\n"Cà phê, sữa",45000\r\n"Nói ""chào""",abc\r\n');
  });

  it('keeps a cell that starts like a formula from running when the file is opened', () => {
    expect(toCsv([['=HYPERLINK("x")', '-12.5', '+84 90', '@a', 'ok']])).toBe('﻿"\'=HYPERLINK(""x"")",-12.5,\'+84 90,\'@a,ok\r\n');
  });

  it('types the cells of a sheet', () => {
    expect(toSheetRows(t, [0, 1], [0, 1, 2, 3])).toEqual([
      ['Khoản', 'Số tiền', 'Xong', 'Thuế'],
      ['Cà phê, sữa', 45000, true, 4500],
      ['Nói "chào"', 'abc', null, '#TYPE'],
    ]);
  });
});
