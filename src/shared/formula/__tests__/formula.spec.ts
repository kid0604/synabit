import { describe, it, expect } from 'vitest';
import { evaluateAt, computeTable } from '../../rich-table/formulas';
import { FDate, FError, Problem, autoValue, toText, type Value } from '..';
import { parse, tokenize, renameInFormula, SyntaxProblem } from '../parser';
import { FUNCTIONS } from '../functions';
import { FUNCTION_DOCS } from '../docs';
import type { RichTable } from '../../rich-table/model';

/** A Thursday, at 09:30 local time. */
const NOW = new Date(2026, 9, 8, 9, 30);

const expenses = (): RichTable => ({
  name: 'chi',
  columns: [
    { name: 'Khoản', type: 'text' },
    { name: 'Loại', type: 'select' },
    { name: 'Số tiền', type: 'number' },
    { name: 'Ngày', type: 'date' },
    { name: 'Xong', type: 'checkbox' },
    { name: 'Nhãn', type: 'multi' },
    { name: 'Ghi chú', type: 'text' },
  ],
  rows: [
    ['Cà phê', 'Ăn uống', '45000', '2026-10-01', '[x]', 'a, b', ''],
    ['Grab', 'Đi lại', '62000', '2026-10-02', '[ ]', 'b', 'gấp'],
    ['Nhà', 'Nhà', '6500000', '2026-10-03', '[x]', '', ''],
    ['Trà', 'Ăn uống', '', '2026-10-04 14:30', '', 'c', ''],
    ['Sách', 'Sách', 'abc', '', '[ ]', '', '5'],
  ],
  views: [{}],
});

const prices = (): RichTable => ({
  name: 'gia',
  columns: [{ name: 'Mã', type: 'text' }, { name: 'Giá', type: 'number' }],
  rows: [['A', '10'], ['B', '20'], ['c', '30']],
  views: [{}],
});

/** A value as one short string, so expectations read as a table. */
function show(v: Value | Problem): string {
  if (v instanceof Problem) return `!${v.code}:${v.key}`;
  if (v === null) return 'blank';
  if (v instanceof FError) return `#${v.code}`;
  if (v instanceof FDate) return `D:${toText(v)}`;
  if (Array.isArray(v)) return `[${v.map(show).join('|')}]`;
  if (typeof v === 'string') return JSON.stringify(v);
  return toText(v);
}

const calc = (expr: string, row = 0) => show(evaluateAt(expenses(), expr, row, { gia: prices() }, NOW));

const cases = (rows: [string, string, number?][]) =>
  it.each(rows)('%s → %s', (expr, expected, row) => expect(calc(expr, row ?? 0)).toBe(expected));

describe('literals and arithmetic', () => {
  cases([
    ['1', '1'], ['1.5', '1.5'], ['.5', '0.5'], ['1e3', '1000'], ['"chữ"', '"chữ"'],
    ['"a \\"b\\""', '"a \\"b\\""'], ['true', 'TRUE'], ['FALSE', 'FALSE'],
    ['1 + 2', '3'], ['5 - 7', '-2'], ['3 * 4', '12'], ['7 / 2', '3.5'], ['7 % 3', '1'],
    ['-7 % 3', '2'], ['2 ^ 10', '1024'], ['2 ^ 3 ^ 2', '512'], ['-2 ^ 2', '4'], ['-(2 ^ 2)', '-4'],
    ['1 + 2 * 3', '7'], ['(1 + 2) * 3', '9'], ['10 - 2 - 3', '5'], ['100 / 10 / 2', '5'],
    ['+5', '5'], ['--5', '5'], ['0.1 + 0.2', '0.3'], ['1 / 3 * 3', '1'],
    ['1 / 0', '#DIV0'], ['5 % 0', '#DIV0'], ['(-8) ^ 0.5', '#VALUE'],
    ['"a" & "b"', '"ab"'], ['"n=" & 1.5', '"n=1.5"'], ['1 & 2', '"12"'], ['"x" & true', '"xTRUE"'],
    ['"2" * 3', '6'], ['"abc" * 3', '#TYPE'], ['true + 1', '2'],
    ['[Số tiền] * 0.1', '4500'], ['[Số tiền] / 1000', '45'],
    ['[Số tiền] + 1', 'blank', 3], ['[Số tiền] * 2', '#TYPE', 4],
    ['[Ghi chú] * 2', '10', 4], ['[Khoản] & " - " & [Loại]', '"Cà phê - Ăn uống"'],
    ['[Số tiền] - [Số tiền]', '0'], ['ROUND([Số tiền] / 7, 2)', '6428.57'],
    ['1 // a comment\n + 1', '2'], ['// nothing but\n42', '42'],
  ]);
});

describe('comparison and logic', () => {
  cases([
    ['1 = 1', 'TRUE'], ['1 == 1', 'TRUE'], ['1 != 2', 'TRUE'], ['1 <> 1', 'FALSE'], ['2 > 1', 'TRUE'],
    ['2 >= 2', 'TRUE'], ['1 < 1', 'FALSE'], ['1 <= 1', 'TRUE'],
    ['"a" = "A"', 'TRUE'], ['"Ă" = "ă"', 'TRUE'], ['"a" < "b"', 'TRUE'], ['"b" > "a"', 'TRUE'],
    ['1 = "1"', 'TRUE'], ['1 = "x"', 'FALSE'], ['1 != "x"', 'TRUE'], ['1 < "x"', 'FALSE'], ['1 > "x"', 'FALSE'],
    ['[Loại] = "ăn uống"', 'TRUE'], ['[Số tiền] > 50000', 'FALSE'], ['[Số tiền] > 50000', 'TRUE', 1],
    ['[Xong]', 'TRUE'], ['[Xong]', 'FALSE', 1], ['[Xong]', 'FALSE', 3],
    ['[Ghi chú] = ""', 'TRUE'], ['[Số tiền] = ""', 'TRUE', 3],
    ['true and false', 'FALSE'], ['true or false', 'TRUE'], ['not true', 'FALSE'],
    ['NOT 1 = 2', 'TRUE'], ['not 1 = 1 or true', 'TRUE'], ['true and not false', 'TRUE'],
    ['1 < 2 and 2 < 3', 'TRUE'], ['1 > 2 or 2 > 3', 'FALSE'], ['TRUE AND TRUE', 'TRUE'],
    ['[Ngày] < DATE(2026, 10, 2)', 'TRUE'], ['[Ngày] = DATE(2026, 10, 1)', 'TRUE'],
    ['1/0 = 1', '#DIV0'], ['0 and true', 'FALSE'], ['"x" and true', 'TRUE'],
    ['[Ngày] < TODAY()', 'blank', 4], ['[Số tiền] > 0', 'blank', 3], ['[Số tiền] = ""', 'TRUE', 3],
    ['IF([Ngày] < TODAY(), "trễ", "")', '""', 4], ['COUNTIF(table[Ngày] < TODAY())', '4'],
  ]);
});

describe('logic functions', () => {
  cases([
    ['IF(true, 1, 2)', '1'], ['IF(false, 1, 2)', '2'], ['IF(false, 1)', 'blank'],
    ['IF(1 > 0, "có", "không")', '"có"'], ['IF(true, 1, 1/0)', '1'], ['IF(1/0, 1, 2)', '#DIV0'],
    ['if([Xong], "x", "")', '"x"'], ['IF([Số tiền] > 1000000, "lớn", "nhỏ")', '"lớn"', 2],
    ['IFS(false, 1, true, 2)', '2'], ['IFS(false, 1, false, 2, 3)', '3'], ['IFS(false, 1)', 'blank'],
    ['SWITCH([Loại], "Nhà", 1, "Đi lại", 2, 0)', '2', 1], ['SWITCH([Loại], "Nhà", 1, 0)', '0'],
    ['SWITCH(3, 1, "a", 2, "b")', 'blank'],
    ['AND(true, true)', 'TRUE'], ['AND(true, false)', 'FALSE'], ['AND(LIST(true, true))', 'TRUE'],
    ['OR(false, false)', 'FALSE'], ['OR(false, 1)', 'TRUE'], ['NOT(false)', 'TRUE'],
    ['ISBLANK([Ghi chú])', 'TRUE'], ['ISBLANK([Khoản])', 'FALSE'], ['ISBLANK([Số tiền])', 'TRUE', 3],
    ['ISBLANK(LIST())', 'TRUE'], ['ISERROR(1/0)', 'TRUE'], ['ISERROR(1)', 'FALSE'],
    ['ISNUMBER([Số tiền])', 'TRUE'], ['ISNUMBER([Số tiền])', 'FALSE', 4],
    ['IFERROR(1/0, 0)', '0'], ['IFERROR(5, 0)', '5'], ['IFERROR([Số tiền] * 2, -1)', '-1', 4],
    ['COALESCE([Số tiền], 0)', '0', 3], ['COALESCE([Số tiền], 0)', '45000'], ['COALESCE("", "", "z")', '"z"'],
  ]);
});

describe('number functions', () => {
  cases([
    ['ROUND(2.5)', '3'], ['ROUND(-2.5)', '-3'], ['ROUND(1.005, 2)', '1.01'], ['ROUND(1234, -2)', '1200'],
    ['ROUND(2.345, 1)', '2.3'], ['ROUNDUP(1.01)', '2'], ['ROUNDUP(-1.01)', '-2'], ['ROUNDDOWN(1.99)', '1'],
    ['ROUNDDOWN(-1.99, 1)', '-1.9'], ['ROUND([Số tiền])', 'blank', 3],
    ['FLOOR(7.8)', '7'], ['FLOOR(23, 5)', '20'], ['CEILING(7.1)', '8'], ['CEILING(21, 5)', '25'],
    ['CEILING(4.2, 0.5)', '4.5'], ['INT(-1.5)', '-2'], ['ABS(-3)', '3'], ['SQRT(16)', '4'], ['SQRT(-1)', '#VALUE'],
    ['MOD(10, 3)', '1'], ['MOD(-3, 2)', '1'], ['MOD(3, -2)', '-1'], ['MOD(1, 0)', '#DIV0'],
    ['POWER(2, 8)', '256'], ['POWER(9, 0.5)', '3'], ['CLAMP(15, 0, 10)', '10'], ['CLAMP(-1, 0, 10)', '0'],
    ['CLAMP(5, 0, 10)', '5'], ['SIGN(-4)', '-1'], ['EXP(0)', '1'], ['LN(1)', '0'], ['LOG10(1000)', '3'],
    ['ROUND(PI(), 4)', '3.1416'], ['ABS("x")', '#TYPE'],
  ]);
});

describe('aggregates', () => {
  cases([
    ['SUM(1, 2, 3)', '6'], ['SUM(table[Số tiền])', '6607000'], ['SUM(LIST())', '0'],
    ['SUM([Số tiền], 1)', '45001'], ['SUM(chi[Số tiền])', '6607000'],
    ['AVERAGE(table[Số tiền])', '2202333.33333'], ['AVG(2, 4)', '3'], ['AVERAGE(LIST())', 'blank'],
    ['MEDIAN(table[Số tiền])', '62000'], ['MEDIAN(1, 2, 3, 4)', '2.5'],
    ['MIN(table[Số tiền])', '45000'], ['MAX(table[Số tiền])', '6500000'], ['MIN(3, 1, 2)', '1'],
    ['COUNT(table[Số tiền])', '3'], ['COUNTA(table[Số tiền])', '4'], ['COUNTA(table[Ghi chú])', '2'],
    ['COUNTUNIQUE(table[Loại])', '4'], ['COUNTUNIQUE("a", "A", "b")', '2'],
    ['ROUND(STDEV(2, 4, 4, 4, 5, 5, 7, 9), 4)', '2.1381'], ['STDEV(1)', '#DIV0'],
    ['[Số tiền] / SUM(table[Số tiền])', '0.00681095807477'],
    ['ROUND([Số tiền] / SUM(table[Số tiền]) * 100, 1)', '98.4', 2],
    ['SUM(table[Xong])', '2'], ['COUNT(LIST(1, "a", 2))', '2'], ['SUM(LIST(1, 1/0))', '#DIV0'],
    ['SUM(table[Số tiền] * 2)', '#TYPE'], ['SUM(IFERROR(table[Số tiền] * 2, 0))', '13214000'], ['MAX(LIST())', 'blank'],
  ]);
});

describe('conditional aggregates and lists', () => {
  cases([
    ['SUMIF(table[Số tiền], table[Loại] = "Ăn uống")', '45000'],
    ['SUMIF(table[Số tiền], table[Loại] = [Loại])', '6500000', 2],
    ['SUMIF(table[Số tiền], table[Xong])', '6545000'],
    ['SUMIF(table[Số tiền], table[Số tiền] > 50000 and table[Xong])', '6500000'],
    ['AVERAGEIF(table[Số tiền], table[Loại] != "Nhà")', '53500'],
    ['AVERAGEIF(table[Số tiền], table[Loại] = "x")', 'blank'],
    ['COUNTIF(table[Loại] = "Ăn uống")', '2'], ['COUNTIF(table[Xong])', '2'],
    ['COUNTIF(table[Ngày] >= DATE(2026, 10, 3))', '2'],
    ['FILTER(table[Khoản], table[Loại] = "Ăn uống")', '["Cà phê"|"Trà"]'],
    ['JOIN(FILTER(table[Khoản], table[Xong]))', '"Cà phê, Nhà"'],
    ['UNIQUE(table[Loại])', '["Ăn uống"|"Đi lại"|"Nhà"|"Sách"]'], ['UNIQUE(LIST("a", "A"))', '["a"]'],
    ['SORT(LIST(3, 1, 2))', '[1|2|3]'], ['SORT(LIST(3, 1, 2), true)', '[3|2|1]'],
    ['SORT(LIST("b", "a"))', '["a"|"b"]'], ['LIST(1, 2)', '[1|2]'], ['LIST(1, LIST(2, 3))', '[1|2|3]'],
    ['INDEX(table[Khoản], 2)', '"Grab"'], ['INDEX(table[Khoản], 9)', '#VALUE'],
    ['LEN(table[Khoản])', '5'], ['CONTAINS(table[Loại], "nhà")', 'TRUE'],
    ['table[Số tiền] > 50000', '[FALSE|TRUE|TRUE|blank|FALSE]'],
    ['[Nhãn]', '["a"|"b"]'], ['CONTAINS([Nhãn], "b")', 'TRUE'], ['CONTAINS([Nhãn], "z")', 'FALSE'],
    ['LEN([Nhãn])', '2'], ['COUNTA([Nhãn])', '0', 2],
  ]);
});

describe('lookup', () => {
  cases([
    ['LOOKUP("B", gia[Mã], gia[Giá])', '20'], ['LOOKUP("b", gia[Mã], gia[Giá])', '20'],
    ['LOOKUP("C", gia[Mã], gia[Giá])', '30'], ['LOOKUP("Z", gia[Mã], gia[Giá])', '#VALUE'],
    ['LOOKUP("Z", gia[Mã], gia[Giá], 0)', '0'], ['LOOKUP("Nhà", table[Khoản], table[Số tiền])', '6500000'],
    ['LOOKUP(2, LIST(1, 2), LIST("x", "y"))', '"y"'], ['SUM(gia[Giá])', '60'],
  ]);
});

describe('text', () => {
  cases([
    ['CONCAT("a", 1, true)', '"a1TRUE"'], ['CONCATENATE("x", "y")', '"xy"'], ['CONCAT(LIST("a", "b"))', '"ab"'],
    ['JOIN(LIST("a", "b"), "-")', '"a-b"'], ['JOIN(LIST("a", "", "b"))', '"a, b"'], ['TEXTJOIN(LIST(1, 2))', '"1, 2"'],
    ['LEN("tiếng việt")', '10'], ['LEN("")', '0'], ['LEFT("abc", 2)', '"ab"'], ['LEFT("abc")', '"a"'],
    ['RIGHT("abc", 2)', '"bc"'], ['RIGHT("abc", 9)', '"abc"'], ['MID("abcdef", 2, 3)', '"bcd"'], ['MID("abc", 0, 1)', '#VALUE'],
    ['UPPER("đường")', '"ĐƯỜNG"'], ['LOWER("ĐƯỜNG")', '"đường"'], ['TRIM("  a   b ")', '"a b"'],
    ['REPLACE("a-b-c", "-", "+")', '"a+b+c"'], ['SUBSTITUTE("aa", "a", "b")', '"bb"'], ['REPLACE("abc", "", "x")', '"abc"'],
    ['CONTAINS("Việc gấp", "GẤP")', 'TRUE'], ['CONTAINS("abc", "z")', 'FALSE'], ['STARTSWITH("Hà Nội", "hà")', 'TRUE'],
    ['SPLIT("a, b,c")', '["a"|"b"|"c"]'], ['SPLIT("a;b", ";")', '["a"|"b"]'], ['SPLIT("")', '[]'],
    ['TEXT(1234.5, "#,##0.00")', '"1,234.50"'], ['TEXT(0.256, "0.0%")', '"25.6%"'], ['TEXT(3.14159, "0.00")', '"3.14"'],
    ['TEXT(42, "0")', '"42"'], ['TEXT(-1234, "#,##0")', '"-1,234"'], ['TEXT([Ngày], "MM/YYYY")', '"10/2026"'],
    ['TEXT([Ngày], "DD/MM/YYYY")', '"01/10/2026"'], ['TEXT([Ngày], "D/M/YY")', '"4/10/26"', 3],
    ['TEXT([Ngày], "HH:mm")', '"14:30"', 3], ['TEXT(5)', '"5"'], ['VALUE("1,234")', '1234'], ['VALUE("x")', '#VALUE'],
    ['UPPER([Khoản])', '"CÀ PHÊ"'], ['LEFT([Khoản], 2) & "…"', '"Cà…"'],
  ]);
});

describe('dates', () => {
  cases([
    ['DATE(2026, 10, 1)', 'D:2026-10-01'], ['DATE(2026, 2, 30)', '#VALUE'], ['DATE(2026, 13, 1)', '#VALUE'],
    ['TODAY()', 'D:2026-10-08'], ['NOW()', 'D:2026-10-08 09:30'],
    ['YEAR([Ngày])', '2026'], ['MONTH([Ngày])', '10'], ['DAY([Ngày])', '1'], ['HOUR([Ngày])', '14', 3], ['MINUTE([Ngày])', '30', 3],
    ['YEAR([Ngày])', 'blank', 4], ['WEEKDAY(DATE(2026, 10, 8))', '5'], ['WEEKDAY(DATE(2026, 10, 4))', '1'],
    ['WEEKNUM(DATE(2026, 1, 1))', '1'], ['WEEKNUM(DATE(2026, 10, 8))', '41'], ['WEEKNUM(DATE(2027, 1, 1))', '1'],
    ['DAYS(DATE(2026, 10, 8), [Ngày])', '7'], ['DAYS([Ngày], DATE(2026, 10, 8))', '-7'], ['DAYS([Ngày], TODAY())', '-4', 3],
    ['TODAY() - [Ngày]', '7'], ['[Ngày] + 30', 'D:2026-10-31'], ['30 + [Ngày]', 'D:2026-10-31'], ['[Ngày] - 1', 'D:2026-09-30'],
    ['[Ngày] * 2', '#TYPE'], ['[Ngày] > TODAY()', 'FALSE'], ['[Ngày] < TODAY()', 'TRUE'],
    ['DATEADD([Ngày], 1, "month")', 'D:2026-11-01'], ['DATEADD(DATE(2026, 1, 31), 1, "months")', 'D:2026-02-28'],
    ['DATEADD(DATE(2024, 2, 29), 1, "year")', 'D:2025-02-28'], ['DATEADD([Ngày], 2, "weeks")', 'D:2026-10-15'],
    ['DATEADD([Ngày], 3)', 'D:2026-10-04'], ['DATEADD([Ngày], 1, "fortnight")', '#VALUE'],
    ['DATEADD([Ngày], 2, "hours")', 'D:2026-10-04 16:30', 3],
    ['EOMONTH([Ngày])', 'D:2026-10-31'], ['EOMONTH(DATE(2024, 1, 15), 1)', 'D:2024-02-29'], ['EOMONTH([Ngày], -1)', 'D:2026-09-30'],
    ['NETWORKDAYS(DATE(2026, 10, 5), DATE(2026, 10, 11))', '5'], ['NETWORKDAYS(DATE(2026, 10, 11), DATE(2026, 10, 5))', '-5'],
    ['YEAR("2026-03-04")', '2026'], ['YEAR("hôm qua")', '#TYPE'], ['DATE(2026, 10, 1) = "2026-10-01"', 'TRUE'],
  ]);
});

describe('rows and variables', () => {
  cases([
    ['ROW()', '1'], ['ROW()', '3', 2], ['PREV([Số tiền])', 'blank'], ['PREV([Số tiền])', '45000', 1],
    ['PREV([Số tiền], 0)', '0'], ['PREV([Khoản] & "!")', '"Cà phê!"', 1], ['PREV(ROW())', '1', 1],
    ['PREV(PREV([Khoản]))', '"Cà phê"', 2],
    ['LET(x, 2, x * 3)', '6'], ['LET(x, 2, y, x + 1, x * y)', '6'], ['LET(net, [Số tiền] * 0.9, ROUND(net))', '40500'],
    ['LET(X, 1, x + 1)', '2'], ['LET(x, 1, LET(x, 2, x) + x)', '3'], ['LET(Khoản, 5, Khoản)', '5'],
  ]);
});

describe('references', () => {
  const bare = (): RichTable => ({
    columns: [{ name: 'Gia', type: 'number' }, { name: 'SL', type: 'number' }],
    rows: [['10', '3']],
    views: [{}],
  });
  it.each([
    ['Gia * SL', '30'], ['gia', '!NAME:unknown_name'], ['[Gia] * SL', '30'], ['table[Gia]', '[10]'],
    ['SUM(table[SL])', '3'], ['[Nope]', '!NAME:unknown_column'], ['table[Nope]', '!NAME:unknown_column'],
    ['other[Gia]', '!NAME:unknown_table'], ['table', '!NAME:table_needs_column'], ['SUM', '!NAME:function_needs_parens'],
  ])('%s → %s', (expr, expected) => expect(show(evaluateAt(bare(), expr, 0, {}, NOW))).toBe(expected));

  cases([
    ['chi[Số tiền]', '[45000|62000|6500000|blank|"abc"]'], ['gia[Nope]', '!NAME:unknown_column_in'],
  ]);
});

describe('problems found before running', () => {
  cases([
    ['', '!VALUE:syntax.empty'], ['1 +', '!VALUE:syntax.unexpected_end'], ['(1', '!VALUE:syntax.expected_close'],
    ['SUM(1, 2', '!VALUE:syntax.expected_close_call'], ['"abc', '!VALUE:syntax.unclosed_string'],
    ['[Số tiền', '!VALUE:syntax.unclosed_bracket'], ['1 2', '!VALUE:syntax.unexpected_token'], ['1 $ 2', '!VALUE:syntax.unexpected_char'],
    ['NOPE(1)', '!NAME:unknown_function'], ['ROUND()', '!VALUE:arity'], ['IF(1)', '!VALUE:arity'], ['TODAY(1)', '!VALUE:arity'],
    ['LET(x, 1)', '!VALUE:arity'], ['LET(x, 1, y, 2)', '!VALUE:let_shape'], ['LET(1, 2, 3)', '!VALUE:let_name'],
    ['LET(x, 1, y)', '!NAME:unknown_name'], ['x + 1', '!NAME:unknown_name'], [', 1', '!VALUE:syntax.unexpected_token'],
  ]);

  it('says where the problem is', () => {
    const p = evaluateAt(expenses(), 'SUM(1, NOPE(2))', 0, {}, NOW) as Problem;
    expect(p.span).toEqual({ from: 7, to: 11 });
    const q = evaluateAt(expenses(), '[Số tiền] + [Thuế]', 0, {}, NOW) as Problem;
    expect(q.span).toEqual({ from: 12, to: 18 });
    expect(q.params).toEqual({ name: 'Thuế' });
  });
});

describe('the tokenizer and the parser', () => {
  it('keeps comments, spans and the two spellings of not-equal', () => {
    expect(tokenize('a <> b // x').map((t) => `${t.t}:${t.v}`)).toEqual(['ident:a', 'op:!=', 'ident:b', 'comment:// x', 'end:']);
    expect(tokenize('[Số tiền]')[0]).toEqual({ t: 'ref', v: 'Số tiền', s: { from: 0, to: 9 } });
  });

  it('tells a whole column from a column and a space', () => {
    expect(parse('table[A]').k).toBe('col');
    expect(() => parse('table [A]')).toThrow(SyntaxProblem);
  });

  it('upper-cases function names and keeps bare names as written', () => {
    const tree = parse('round(Giá)');
    expect(tree.k === 'call' && tree.name).toBe('ROUND');
    expect(tree.k === 'call' && tree.args[0]).toMatchObject({ k: 'ident', name: 'Giá' });
  });

  it('reads and, or and not as words, in any case', () => {
    expect(parse('a AND b Or NOT c')).toMatchObject({ k: 'bin', op: 'or' });
  });
});

describe('renaming a column inside a formula', () => {
  it.each([
    ['[Giá] * 2', '[Giá bán] * 2'],
    ['Giá * 2', '[Giá bán] * 2'],
    ['SUM(table[Giá]) // [Giá] in a comment', 'SUM(table[Giá bán]) // [Giá] in a comment'],
    ['LOOKUP(x, gia[Giá], gia[Giá])', 'LOOKUP(x, gia[Giá], gia[Giá])'],
    ['self[Giá]', 'self[Giá bán]'],
    ['GIÁ(1)', 'GIÁ(1)'],
    ['[Giá]+[Giá]', '[Giá bán]+[Giá bán]'],
    ['"[Giá]"', '"[Giá]"'],
  ])('%s → %s', (before, after) => expect(renameInFormula(before, 'Giá', 'Giá bán', 'self')).toBe(after));

  it.each([
    ['LET(Giá, 5, Giá * [Giá])', 'LET(Giá, 5, Giá * [Giá bán])'],
    ['LET(x, Giá, x + Giá)', 'LET(x, [Giá bán], x + [Giá bán])'],
    ['LET(Giá, Giá * 2, (Giá))', 'LET(Giá, [Giá bán] * 2, (Giá))'],
    ['LET(Giá, 1, Giá) + Giá', 'LET(Giá, 1, Giá) + [Giá bán]'],
    ['LET(x, 1, LET(Giá, x, Giá + table[Giá]))', 'LET(x, 1, LET(Giá, x, Giá + table[Giá bán]))'],
  ])('leaves a LET name spelt like the column alone: %s → %s', (before, after) =>
    expect(renameInFormula(before, 'Giá', 'Giá bán', 'self')).toBe(after));

  it('writes a one-word name bare where it was bare', () => {
    expect(renameInFormula('Giá * SL', 'Giá', 'Price')).toBe('Price * SL');
  });
});

describe('computing a table', () => {
  const ledger = (): RichTable => ({
    columns: [
      { name: 'Thu', type: 'number' },
      { name: 'Chi', type: 'number' },
      { name: 'Số dư', type: 'formula', expr: 'PREV([Số dư], 0) + COALESCE([Thu], 0) - COALESCE([Chi], 0)' },
      { name: 'Tỉ lệ', type: 'formula', expr: 'ROUND([Chi] / SUM(table[Chi]), 2)' },
      { name: 'Lớn', type: 'formula', expr: '[Chi] > 100' },
    ],
    rows: [['1000', '', '', '', ''], ['', '200', '', '', ''], ['50', '100', '', '', '']],
    views: [{}],
  });

  it('runs a balance down the rows and writes every value', () => {
    const { table, problems } = computeTable(ledger(), {}, NOW);
    expect(problems.size).toBe(0);
    expect(table.rows.map((r) => r.slice(2))).toEqual([
      // No Chi in the first row: neither large nor small.
      ['1000', '', ''], ['800', '0.67', '[x]'], ['750', '0.33', '[ ]'],
    ]);
  });

  it('gives back the same table when nothing is stale', () => {
    const once = computeTable(ledger(), {}, NOW).table;
    expect(computeTable(once, {}, NOW).table).toBe(once);
  });

  it('replaces only the rows whose values changed', () => {
    const once = computeTable(ledger(), {}, NOW).table;
    const edited = { ...once, rows: once.rows.map((r, i) => (i === 2 ? ['50', '150', ...r.slice(2)] : r)) };
    const again = computeTable(edited, {}, NOW).table;
    expect(again.rows[0]).toBe(edited.rows[0]);
    expect(again.rows[2][2]).toBe('700');
    // The share column reads the whole of Chi, so row 1 changed too.
    expect(again.rows[1]).not.toBe(edited.rows[1]);
  });

  it('computes a column after the columns it reads, whatever their order', () => {
    const t: RichTable = {
      columns: [
        { name: 'C', type: 'formula', expr: '[B] * 2' },
        { name: 'B', type: 'formula', expr: 'SUM(table[A]) + [A]' },
        { name: 'A', type: 'number' },
      ],
      rows: [['', '', '1'], ['', '', '2']],
      views: [{}],
    };
    expect(computeTable(t, {}, NOW).table.rows).toEqual([['8', '4', '1'], ['10', '5', '2']]);
  });

  it('calls a ring of columns a cycle, and only the ring', () => {
    const t: RichTable = {
      columns: [
        { name: 'A', type: 'formula', expr: '[B] + 1' },
        { name: 'B', type: 'formula', expr: '[A] + 1' },
        { name: 'C', type: 'formula', expr: 'SUM(table[C])' },
        { name: 'D', type: 'formula', expr: '1' },
      ],
      rows: [['', '', '', '']],
      views: [{}],
    };
    const { table, problems } = computeTable(t, {}, NOW);
    expect(table.rows[0]).toEqual(['#CYCLE', '#CYCLE', '#CYCLE', '1']);
    expect(problems.get(0)?.params).toEqual({ names: 'A, B, C' });
  });

  it('writes an error as its code, and a bad formula into every cell', () => {
    const t: RichTable = {
      columns: [{ name: 'A', type: 'number' }, { name: 'B', type: 'formula', expr: '10 / [A]' }, { name: 'C', type: 'formula', expr: '[Z]' }],
      rows: [['0', '', ''], ['5', '', '']],
      views: [{}],
    };
    const { table, errors } = computeTable(t, {}, NOW);
    expect(table.rows).toEqual([['0', '#DIV0', '#NAME'], ['5', '2', '#NAME']]);
    expect([...errors.keys()]).toEqual(['0:1']);
  });

  it('reads another table of the note by its name', () => {
    const t: RichTable = {
      columns: [{ name: 'Mã', type: 'text' }, { name: 'Giá', type: 'formula', expr: 'LOOKUP([Mã], gia[Mã], gia[Giá], 0)' }],
      rows: [['B', ''], ['Q', '']],
      views: [{}],
    };
    expect(computeTable(t, { gia: prices() }, NOW).table.rows).toEqual([['B', '20'], ['Q', '0']]);
    expect(computeTable(t, {}, NOW).table.rows).toEqual([['B', '#NAME'], ['Q', '#NAME']]);
  });

  it('reads a formula column from another formula as what it computed', () => {
    const t: RichTable = {
      columns: [
        { name: 'D', type: 'formula', expr: 'DATE(2026, 1, 1) + ROW()' },
        { name: 'E', type: 'formula', expr: 'MONTH([D]) & "/" & DAY([D])' },
      ],
      rows: [['', ''], ['', '']],
      views: [{}],
    };
    expect(computeTable(t, {}, NOW).table.rows).toEqual([['2026-01-02', '1/2'], ['2026-01-03', '1/3']]);
  });

  it('runs every example in the design document', () => {
    const t: RichTable = {
      name: 'don',
      columns: [
        { name: 'Đơn giá', type: 'number' }, { name: 'Số lượng', type: 'number' }, { name: 'Giảm giá', type: 'number' },
        { name: 'Số tiền', type: 'number' }, { name: 'Hạn', type: 'date' }, { name: 'Xong', type: 'checkbox' },
        { name: 'Loại', type: 'select' }, { name: 'Thu', type: 'number' }, { name: 'Chi', type: 'number' },
        { name: 'Mã', type: 'text' }, { name: 'Gross', type: 'number' }, { name: 'Thuế', type: 'number' },
        { name: 'Ngày trả', type: 'date' }, { name: 'Ngày mượn', type: 'date' }, { name: 'Ngày', type: 'date' },
        { name: 'Số dư', type: 'formula', expr: 'PREV([Số dư], 0) + [Thu] - [Chi]' },
      ],
      rows: [['100', '3', '0.1', '50000', '2026-10-01', '', 'Nhà', '500', '100', 'B', '1000', '50', '2026-10-20', '2026-10-05', '2026-10-01', '']],
      views: [{}],
    };
    const at = (expr: string) => show(evaluateAt(t, expr, 0, { gia: prices() }, NOW));
    expect(at('[Số tiền] * 0.1')).toBe('5000');
    expect(at('ROUND([Đơn giá] * [Số lượng] * (1 - [Giảm giá]), 0)')).toBe('270');
    expect(at('IF([Hạn] < TODAY() and not [Xong], "Trễ", "")')).toBe('"Trễ"');
    expect(at('[Số tiền] / SUM(table[Số tiền])')).toBe('1');
    expect(at('PREV([Số dư], 0) + [Thu] - [Chi]')).toBe('400');
    expect(at('SUMIF(table[Số tiền], table[Loại] = [Loại])')).toBe('50000');
    expect(at('LOOKUP([Mã], gia[Mã], gia[Giá])')).toBe('20');
    expect(at('LET(net, [Gross] - [Thuế], IF(net < 0, 0, net))')).toBe('950');
    expect(at('DAYS([Ngày trả], [Ngày mượn])')).toBe('15');
    expect(at('TEXT([Ngày], "MM/YYYY")')).toBe('"10/2026"');
  });
});

describe('the function reference', () => {
  it('documents every function, and nothing that is not one', () => {
    const aliases = new Set(['AVG', 'SUBSTITUTE', 'CONCATENATE', 'TEXTJOIN']);
    const named = Object.keys(FUNCTIONS).filter((n) => !aliases.has(n)).sort();
    expect(Object.keys(FUNCTION_DOCS).sort()).toEqual(named);
  });
});

describe('remembering whole-column parts from row to row', () => {
  const grouped = (n: number): RichTable => ({
    columns: [
      { name: 'Loại', type: 'text' }, { name: 'Giá', type: 'number' },
      { name: 'Phần', type: 'formula', expr: 'ROUND([Giá] / SUMIF(table[Giá], table[Loại] = [Loại]), 4)' },
      { name: 'Hạng', type: 'formula', expr: 'COUNTIF(table[Giá] > [Giá]) + 1' },
      { name: 'Lũy kế', type: 'formula', expr: 'PREV([Lũy kế], 0) + SUMIF(table[Giá], table[Loại] = [Loại]) / 1000' },
    ],
    rows: Array.from({ length: n }, (_, i) => [i % 2 ? 'A' : 'B', String(10 + (i % 7)), '', '', '']),
    views: [{}],
  });

  it('gives the same answers as computing every row afresh', () => {
    const { table } = computeTable(grouped(12), {}, NOW);
    for (let r = 0; r < 12; r++) {
      expect(table.rows[r][2]).toBe(show(evaluateAt(grouped(12), 'ROUND([Giá] / SUMIF(table[Giá], table[Loại] = [Loại]), 4)', r, {}, NOW)));
      expect(table.rows[r][3]).toBe(show(evaluateAt(grouped(12), 'COUNTIF(table[Giá] > [Giá]) + 1', r, {}, NOW)));
    }
    // PREV keeps the running total out of the memo; the SUMIF inside it is still remembered.
    expect(table.rows[0][4]).toBe('0.076');
    expect(table.rows[1][4]).toBe('0.151');
  });

  it('computes a share of the group over two thousand rows quickly', () => {
    const t0 = performance.now();
    computeTable(grouped(2000), {}, NOW);
    expect(performance.now() - t0).toBeLessThan(400);
  });
});

describe('money', () => {
  const near = (expr: string, expected: number, digits = 2) => {
    const v = evaluateAt(expenses(), expr, 0, {}, NOW);
    expect(typeof v).toBe('number');
    expect(v as number).toBeCloseTo(expected, digits);
  };
  // Excel's own documented examples.
  it('pays a loan off evenly', () => near('PMT(0.1 / 12, 360, 100000)', -877.57));
  it('pays nothing in interest at a zero rate', () => near('PMT(0, 10, 1000)', -100));
  it('grows savings', () => near('FV(0.06 / 12, 10, -200, -500, 1)', 2581.40));
  it('discounts an annuity', () => near('PV(0.08 / 12, 240, 500)', -59777.15));
  it('nets present values', () => near('NPV(0.1, -10000, 3000, 4200, 6800)', 1188.44));
  it('finds the internal rate', () => near('IRR(LIST(-70000, 12000, 15000, 18000, 21000, 26000))', 0.086631, 5));
  it('refuses flows that never change sign', () => {
    expect(show(evaluateAt(expenses(), 'IRR(LIST(1, 2, 3))', 0, {}, NOW))).toBe('#VALUE');
  });
});

describe('a table named with hyphens', () => {
  const named: RichTable = { ...prices(), name: 'bang-gia' };
  it('is read as a name, not a subtraction', () => {
    expect(show(evaluateAt(expenses(), 'SUM(bang-gia[Giá])', 0, { 'bang-gia': named }, NOW))).toBe('60');
    expect(tokenize('a-b').map((t) => t.v)).toEqual(['a', '-', 'b', '']);
    expect(tokenize('a-b[X]').map((t) => t.v)).toEqual(['a-b', 'X', '']);
  });
});

describe('exactness', () => {
  it('keeps a long whole number exact, and drops only float noise', () => {
    expect(toText(1234567890123)).toBe('1234567890123');
    expect(toText(0.1 + 0.2)).toBe('0.3');
    expect(calc('1234567890123 + 1')).toBe('1234567890124');
  });

  it('remembers two amounts that print alike as two', () => {
    const table: RichTable = {
      columns: [{ name: 'X', type: 'number' }, { name: 'N', type: 'formula', expr: 'COUNTIF(table[X] = [X])' }],
      rows: [['1234567890123', ''], ['1234567890124', '']],
      views: [{}],
    };
    expect(computeTable(table).table.rows.map((r) => r[1])).toEqual(['1', '1']);
  });

  it('writes a formula that comes out blank as blank, not as the file had it', () => {
    const table: RichTable = {
      columns: [{ name: 'X', type: 'text' }, { name: 'Y', type: 'formula', expr: 'IF([X] = "a", 1)' }, { name: 'Z', type: 'formula', expr: 'ISBLANK([Y])' }],
      rows: [['b', 'stale', '']],
      views: [{}],
    };
    expect(computeTable(table).table.rows[0]).toEqual(['b', '', '[x]']);
  });

  it('does not rewrite a value that only differs in how a cell stores it', () => {
    const table: RichTable = {
      columns: [{ name: 'X', type: 'text' }, { name: 'Y', type: 'formula', expr: '" " & [X] & "<br>b "' }],
      rows: [['a', 'a\nb']],
      views: [{}],
    };
    expect(computeTable(table).table).toBe(table);
  });
});

it('keeps the cents of a large amount', () => {
  expect(toText(1234567890123.25)).toBe('1234567890123.25');
});

describe('a condition over a list holds when any item does', () => {
  cases([
    ['[Nhãn] = "b"', '[FALSE|TRUE]'], ['IF([Nhãn] = "b", 1, 0)', '1'], ['IF([Nhãn] = "b", 1, 0)', '0', 3],
    ['IF([Nhãn] = "z", 1, 0)', '0'], ['IF([Nhãn] = "c", 1, 0)', '0', 2],
    ['IF(LIST(false, false), 1, 0)', '0'], ['IF(LIST(), 1, 0)', '0'], ['IF(LIST(false, 1/0), 1, 0)', '#DIV0'],
    ['COUNTIF(table[Nhãn] = "b")', '2'], ['SUMIF(table[Số tiền], table[Nhãn] = "b")', '107000'],
    ['FILTER(table[Khoản], table[Nhãn] = "b")', '["Cà phê"|"Grab"]'],
    // A mask still counts its true items, one by one.
    ['COUNTIF(LIST(true, false, true))', '2'], ['COUNTIF(table[Loại] = "Ăn uống")', '2'],
    ['([Nhãn] = "a") and true', '[TRUE|FALSE]'], ['IF(([Nhãn] = "a") and true, 1, 0)', '1'],
  ]);
});

describe('TEXT patterns, as Excel writes them', () => {
  cases([
    ['TEXT([Ngày], "dd/mm/yyyy")', '"01/10/2026"'], ['TEXT([Ngày], "d/m/yy")', '"4/10/26"', 3],
    ['TEXT([Ngày], "hh:mm")', '"14:30"', 3], ['TEXT([Ngày], "HH:MM")', '"14:30"', 3], ['TEXT([Ngày], "h:mm")', '"14:30"', 3],
    ['TEXT([Ngày], "mm:ss")', '"30:00"', 3], ['TEXT([Ngày], "dd/MM hh:mm")', '"04/10 14:30"', 3],
    ['TEXT([Ngày], "d MMM yyyy")', '"1 Oct 2026"'], ['TEXT([Ngày], "MMMM yyyy")', '"October 2026"'],
    ['TEXT([Ngày], "mmm")', '"Oct"'], ['TEXT([Ngày], "dd \\"tháng\\" mm")', '"01 tháng 10"'],
    ['TEXT(5, "#.##")', '"5"'], ['TEXT(3.14159, "#.##")', '"3.14"'], ['TEXT(2.5, "#.##")', '"2.5"'],
    ['TEXT(1.5, "0.0#")', '"1.5"'], ['TEXT(1.567, "0.0#")', '"1.57"'], ['TEXT(1234567.891, "#,##0.00")', '"1,234,567.89"'],
    ['TEXT(7, "000")', '"007"'], ['TEXT(-0.001, "0.00")', '"0.00"'], ['TEXT(-0.004, "#,##0.00")', '"0.00"'],
    ['TEXT(-0.4, "0")', '"0"'], ['TEXT(-0.0001, "0.0%")', '"0.0%"'], ['TEXT(-1.5, "0.00")', '"-1.50"'],
  ]);
});

describe('PREV of a LET name is its value at the row above', () => {
  cases([
    ['LET(x, [Số tiền], PREV(x))', '45000', 1], ['LET(x, [Khoản], PREV(x & "!"))', '"Cà phê!"', 1],
    ['LET(r, ROW(), PREV(r * 10))', '20', 2], ['LET(x, [Số tiền], y, x * 2, PREV(y))', '90000', 1],
    ['LET(x, [Số tiền], PREV(LET(y, x + 1, y)))', '45001', 1], ['LET(x, [Số tiền], PREV(PREV(x)))', '45000', 2],
    ['LET(x, 2, PREV(x))', '2', 1], ['LET(x, [Số tiền], PREV(x, 0))', '0'],
    ['LET(x, [Số tiền], x - PREV(x))', '17000', 1], ['LET(x, 1, LET(x, [Khoản], PREV(x)))', '"Cà phê"', 1],
  ]);

  it('runs a balance through a LET name', () => {
    const t: RichTable = {
      columns: [
        { name: 'Thu', type: 'number' },
        { name: 'Δ', type: 'formula', expr: 'LET(d, COALESCE([Thu], 0), d - PREV(d, 0))' },
      ],
      rows: [['10', ''], ['15', ''], ['', '']],
      views: [{}],
    };
    expect(computeTable(t, {}, NOW).table.rows.map((r) => r[1])).toEqual(['10', '5', '-15']);
  });
});

describe('LOOKUP with a blank key', () => {
  cases([
    ['LOOKUP([Số tiền], table[Số tiền], table[Khoản])', 'blank', 3],
    ['LOOKUP("", table[Ghi chú], table[Khoản], "none")', '"none"'],
    ['LOOKUP([Ghi chú], table[Ghi chú], table[Khoản])', 'blank'],
  ]);
});

describe('text that reads like an error code', () => {
  it.each(['#DIV0', '#NAME', '#DIV/0', '#VALUE'])('%s stays text', (raw) => expect(autoValue(raw)).toBe(raw));

  it('is not an error in a formula that reads it', () => {
    const t: RichTable = {
      columns: [{ name: 'X', type: 'text' }, { name: 'Y', type: 'formula', expr: 'IF(ISERROR([X]), "err", [X] & "!")' }],
      rows: [['#NAME', ''], ['#DIV0', '']],
      views: [{}],
    };
    expect(computeTable(t, {}, NOW).table.rows.map((r) => r[1])).toEqual(['#NAME!', '#DIV0!']);
  });

  it('is an error when another table’s formula column stored it — what a stored #NAME almost always is', () => {
    const other: RichTable = { name: 'o', columns: [{ name: 'F', type: 'formula', expr: '1 / 0' }], rows: [['#DIV0']], views: [{}] };
    expect(show(evaluateAt(expenses(), 'ISERROR(INDEX(o[F], 1))', 0, { o: other }, NOW))).toBe('TRUE');
  });
});

describe('number functions over a list, item by item', () => {
  cases([
    ['ROUND(LIST(1.25, 2.5), 1)', '[1.3|2.5]'], ['ROUNDUP(LIST(1.1, -1.1))', '[2|-2]'], ['ROUNDDOWN(LIST(1.9, ""))', '[1|blank]'],
    ['SUM(ROUND(IFERROR(table[Số tiền] / 7, 0)))', '943857'], ['FLOOR(LIST(7.8, 2.2))', '[7|2]'], ['CEILING(LIST(21, 4), 5)', '[25|5]'],
    ['SQRT(LIST(4, 9))', '[2|3]'], ['MOD(LIST(5, 7), 3)', '[2|1]'], ['POWER(LIST(2, 3), 2)', '[4|9]'],
    ['CLAMP(LIST(-1, 5, 15), 0, 10)', '[0|5|10]'], ['ROUND(LIST(1, "x"))', '[1|#TYPE]'],
  ]);
});

describe('text written decomposed (NFD) is the same text', () => {
  const nfd = (s: string) => s.normalize('NFD');
  cases([
    [`"${nfd('Việt')}" = "Việt"`, 'TRUE'], [`"${nfd('Việt')}" != "việt"`, 'FALSE'],
    [`CONTAINS("${nfd('Tiếng Việt')}", "việt")`, 'TRUE'], [`STARTSWITH("${nfd('Việt Nam')}", "việt")`, 'TRUE'],
    [`SUBSTITUTE("${nfd('Việt Nam')}", "Việt", "VN")`, '"VN Nam"'], [`REPLACE("Việt Nam", "${nfd('Việt')}", "VN")`, '"VN Nam"'],
    [`LEN("${nfd('Việt')}")`, '4'], [`LEFT("${nfd('Đường')}", 2)`, '"Đư"'], [`COUNTUNIQUE("Việt", "${nfd('Việt')}")`, '1'],
    [`LOOKUP("${nfd('Việt')}", LIST("Việt"), LIST(1))`, '1'], [`SWITCH("${nfd('Nhà')}", "Nhà", 1, 0)`, '1'],
    [`CONTAINS(LIST("${nfd('Nhà')}"), "nhà")`, 'TRUE'],
  ]);
});

describe('large and long inputs', () => {
  it('finds the least and greatest of two hundred thousand values', () => {
    const xs: Value[] = Array.from({ length: 200_000 }, (_, i) => i - 7);
    expect(FUNCTIONS.MIN.call!([xs], {} as never)).toBe(-7);
    expect(FUNCTIONS.MAX.call!([xs], {} as never)).toBe(199_992);
  });

  it('counts working days by whole weeks, as one day at a time would', () => {
    const brute = (a: number, b: number) => {
      let n = 0;
      for (let t = Math.min(a, b); t <= Math.max(a, b); t += 86_400_000) if (![0, 6].includes(new Date(t).getUTCDay())) n++;
      return a <= b ? n : -n;
    };
    for (let start = 0; start < 14; start++) {
      for (let len = -20; len <= 20; len++) {
        const a = FDate.of(2026, 10, 1 + start);
        const b = new FDate(a.ms + len * 86_400_000);
        expect(FUNCTIONS.NETWORKDAYS.call!([a, b], {} as never)).toBe(brute(a.ms, b.ms));
      }
    }
  });

  it('counts a century of working days at once', () => {
    const t0 = performance.now();
    expect(calc('NETWORKDAYS(DATE(1900, 1, 1), DATE(2100, 1, 1))')).toBe('52180');
    expect(performance.now() - t0).toBeLessThan(50);
  });

  cases([
    ['NETWORKDAYS(DATE(2026, 1, 1), DATE(2026, 12, 31))', '261'],
    ['NETWORKDAYS(DATE(2026, 10, 10), DATE(2026, 10, 10))', '0'], ['NETWORKDAYS(DATE(2026, 10, 8), DATE(2026, 10, 8))', '1'],
    ['NETWORKDAYS(DATE(2026, 10, 5), DATE(2026, 10, 11), LIST(DATE(2026, 10, 6), DATE(2026, 10, 10), DATE(2026, 10, 6), DATE(2026, 11, 1)))', '4'],
    ['NETWORKDAYS(DATE(2026, 10, 11), DATE(2026, 10, 5), DATE(2026, 10, 6))', '-4'],
    ['NETWORKDAYS(DATE(2026, 10, 5), DATE(2026, 10, 11), LIST())', '5'],
  ]);
});

describe('WEEKNUM as Excel counts it', () => {
  cases([
    ['WEEKNUM(DATE(2027, 1, 1))', '1'], ['WEEKNUM(DATE(2027, 1, 3))', '2'], ['WEEKNUM(DATE(2027, 1, 3), 2)', '1'],
    ['WEEKNUM(DATE(2027, 1, 4), 2)', '2'], ['WEEKNUM(DATE(2026, 1, 4))', '2'], ['WEEKNUM(DATE(2026, 1, 4), 11)', '1'],
    ['WEEKNUM(DATE(2026, 1, 4), 17)', '2'], ['WEEKNUM(DATE(2026, 12, 31))', '53'],
    ['WEEKNUM(DATE(2027, 1, 1), 21)', '53'], ['ISOWEEKNUM(DATE(2027, 1, 1))', '53'], ['ISOWEEKNUM(DATE(2026, 10, 8))', '41'],
    ['WEEKNUM(DATE(2026, 1, 1), 3)', '#VALUE'], ['WEEKNUM([Ngày])', 'blank', 4], ['ISOWEEKNUM([Ngày])', 'blank', 4],
  ]);
});
