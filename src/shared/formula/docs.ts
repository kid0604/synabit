/**
 * What the formula editor says about each function as it is typed: how to
 * call it, an example, and one line on what it does. Kept here rather than in
 * the locale files: it is reference text, it changes with the functions, and
 * it would be most of their size.
 */
export interface FunctionDoc {
  sig: string;
  ex: string;
  vi: string;
  en: string;
  group: 'logic' | 'number' | 'aggregate' | 'list' | 'text' | 'date' | 'row';
}

export const FUNCTION_DOCS: Record<string, FunctionDoc> = {
  IF: { group: 'logic', sig: 'IF(condition, if_true, [if_false])', ex: 'IF([Số tiền] > 1000000, "lớn", "nhỏ")', vi: 'Chọn một trong hai giá trị theo điều kiện.', en: 'One of two values, by a condition.' },
  IFS: { group: 'logic', sig: 'IFS(cond1, value1, cond2, value2, …, [default])', ex: 'IFS([Điểm] >= 8, "Giỏi", [Điểm] >= 5, "Đạt", "Chưa đạt")', vi: 'Giá trị của điều kiện đúng đầu tiên.', en: 'The value of the first condition that holds.' },
  SWITCH: { group: 'logic', sig: 'SWITCH(value, match1, result1, match2, result2, …, [default])', ex: 'SWITCH([Loại], "Nhà", 1, "Đi lại", 2, 0)', vi: 'Kết quả ứng với giá trị khớp.', en: 'The result paired with the matching value.' },
  AND: { group: 'logic', sig: 'AND(a, b, …)', ex: 'AND([Xong], [Số tiền] > 0)', vi: 'Đúng khi mọi điều kiện đúng.', en: 'True when every condition is.' },
  OR: { group: 'logic', sig: 'OR(a, b, …)', ex: 'OR([Gấp], [Hạn] < TODAY())', vi: 'Đúng khi có một điều kiện đúng.', en: 'True when any condition is.' },
  NOT: { group: 'logic', sig: 'NOT(a)', ex: 'NOT([Xong])', vi: 'Đảo đúng/sai.', en: 'True for false, false for true.' },
  ISBLANK: { group: 'logic', sig: 'ISBLANK(value)', ex: 'ISBLANK([Hạn])', vi: 'Ô có trống không.', en: 'Whether a value is blank.' },
  ISERROR: { group: 'logic', sig: 'ISERROR(value)', ex: 'ISERROR([A] / [B])', vi: 'Giá trị có phải là lỗi không.', en: 'Whether a value is an error.' },
  ISNUMBER: { group: 'logic', sig: 'ISNUMBER(value)', ex: 'ISNUMBER([Số tiền])', vi: 'Giá trị có phải là số không.', en: 'Whether a value is a number.' },
  IFERROR: { group: 'logic', sig: 'IFERROR(value, if_error)', ex: 'IFERROR([A] / [B], 0)', vi: 'Giá trị, hoặc thứ thay thế khi nó là lỗi.', en: 'The value, or a stand-in when it is an error.' },
  COALESCE: { group: 'logic', sig: 'COALESCE(a, b, …)', ex: 'COALESCE([Số tiền], 0)', vi: 'Giá trị không trống đầu tiên.', en: 'The first value that is not blank.' },

  ROUND: { group: 'number', sig: 'ROUND(number, [digits])', ex: 'ROUND([Giá] * 1.1, 0)', vi: 'Làm tròn.', en: 'Round.' },
  ROUNDUP: { group: 'number', sig: 'ROUNDUP(number, [digits])', ex: 'ROUNDUP([Giờ], 0)', vi: 'Làm tròn lên (xa số 0).', en: 'Round away from zero.' },
  ROUNDDOWN: { group: 'number', sig: 'ROUNDDOWN(number, [digits])', ex: 'ROUNDDOWN([Giờ], 1)', vi: 'Làm tròn xuống (về số 0).', en: 'Round toward zero.' },
  FLOOR: { group: 'number', sig: 'FLOOR(number, [multiple])', ex: 'FLOOR([Giá], 1000)', vi: 'Xuống bội gần nhất.', en: 'Down to the nearest multiple.' },
  CEILING: { group: 'number', sig: 'CEILING(number, [multiple])', ex: 'CEILING([Giá], 1000)', vi: 'Lên bội gần nhất.', en: 'Up to the nearest multiple.' },
  INT: { group: 'number', sig: 'INT(number)', ex: 'INT([Giờ])', vi: 'Phần nguyên (xuống).', en: 'The whole number below.' },
  ABS: { group: 'number', sig: 'ABS(number)', ex: 'ABS([Chênh lệch])', vi: 'Giá trị tuyệt đối.', en: 'Absolute value.' },
  SQRT: { group: 'number', sig: 'SQRT(number)', ex: 'SQRT([Diện tích])', vi: 'Căn bậc hai.', en: 'Square root.' },
  MOD: { group: 'number', sig: 'MOD(number, divisor)', ex: 'MOD(ROW(), 2)', vi: 'Số dư của phép chia.', en: 'The remainder of a division.' },
  POWER: { group: 'number', sig: 'POWER(number, power)', ex: 'POWER(1 + [Lãi], [Năm])', vi: 'Luỹ thừa (như ^).', en: 'A power (like ^).' },
  CLAMP: { group: 'number', sig: 'CLAMP(number, low, high)', ex: 'CLAMP([Tiến độ], 0, 1)', vi: 'Giữ số trong một khoảng.', en: 'A number kept within a range.' },
  SIGN: { group: 'number', sig: 'SIGN(number)', ex: 'SIGN([Chênh lệch])', vi: '-1, 0 hoặc 1 theo dấu.', en: '-1, 0 or 1, by sign.' },
  EXP: { group: 'number', sig: 'EXP(number)', ex: 'EXP(1)', vi: 'e mũ số.', en: 'e to the power.' },
  LN: { group: 'number', sig: 'LN(number)', ex: 'LN([Tăng trưởng])', vi: 'Logarit tự nhiên.', en: 'Natural logarithm.' },
  LOG10: { group: 'number', sig: 'LOG10(number)', ex: 'LOG10(1000)', vi: 'Logarit cơ số 10.', en: 'Base-10 logarithm.' },
  PI: { group: 'number', sig: 'PI()', ex: 'PI() * [Bán kính] ^ 2', vi: 'Số π.', en: 'The number π.' },

  SUM: { group: 'aggregate', sig: 'SUM(a, b, …)', ex: 'SUM(table[Số tiền])', vi: 'Tổng.', en: 'Sum.' },
  AVERAGE: { group: 'aggregate', sig: 'AVERAGE(a, b, …)', ex: 'AVERAGE(table[Điểm])', vi: 'Trung bình.', en: 'Average.' },
  MEDIAN: { group: 'aggregate', sig: 'MEDIAN(a, b, …)', ex: 'MEDIAN(table[Giá])', vi: 'Trung vị.', en: 'Median.' },
  MIN: { group: 'aggregate', sig: 'MIN(a, b, …)', ex: 'MIN(table[Giá])', vi: 'Nhỏ nhất.', en: 'Smallest.' },
  MAX: { group: 'aggregate', sig: 'MAX(a, b, …)', ex: 'MAX([A], [B])', vi: 'Lớn nhất.', en: 'Largest.' },
  COUNT: { group: 'aggregate', sig: 'COUNT(a, b, …)', ex: 'COUNT(table[Số tiền])', vi: 'Đếm các số.', en: 'How many numbers.' },
  COUNTA: { group: 'aggregate', sig: 'COUNTA(a, b, …)', ex: 'COUNTA(table[Ghi chú])', vi: 'Đếm các giá trị không trống.', en: 'How many values are not blank.' },
  COUNTUNIQUE: { group: 'aggregate', sig: 'COUNTUNIQUE(a, b, …)', ex: 'COUNTUNIQUE(table[Loại])', vi: 'Đếm các giá trị khác nhau.', en: 'How many different values.' },
  STDEV: { group: 'aggregate', sig: 'STDEV(a, b, …)', ex: 'STDEV(table[Điểm])', vi: 'Độ lệch chuẩn (mẫu).', en: 'Standard deviation (sample).' },

  SUMIF: { group: 'list', sig: 'SUMIF(value, condition)', ex: 'SUMIF(table[Số tiền], table[Loại] = [Loại])', vi: 'Tổng những giá trị có điều kiện đúng.', en: 'The sum of the values whose condition holds.' },
  AVERAGEIF: { group: 'list', sig: 'AVERAGEIF(value, condition)', ex: 'AVERAGEIF(table[Điểm], table[Lớp] = "A")', vi: 'Trung bình những giá trị có điều kiện đúng.', en: 'The average of the values whose condition holds.' },
  COUNTIF: { group: 'list', sig: 'COUNTIF(condition)', ex: 'COUNTIF(table[Loại] = "Nhà")', vi: 'Đếm những hàng có điều kiện đúng.', en: 'How many rows the condition holds for.' },
  FILTER: { group: 'list', sig: 'FILTER(value, condition)', ex: 'FILTER(table[Khoản], table[Xong])', vi: 'Những giá trị có điều kiện đúng.', en: 'The values whose condition holds.' },
  UNIQUE: { group: 'list', sig: 'UNIQUE(list)', ex: 'UNIQUE(table[Loại])', vi: 'Danh sách bỏ trùng.', en: 'A list without repeats.' },
  SORT: { group: 'list', sig: 'SORT(list, [descending])', ex: 'SORT(table[Tên])', vi: 'Danh sách đã sắp xếp.', en: 'A list, sorted.' },
  LIST: { group: 'list', sig: 'LIST(a, b, …)', ex: 'LIST(1, 2, 3)', vi: 'Một danh sách.', en: 'A list.' },
  INDEX: { group: 'list', sig: 'INDEX(list, n)', ex: 'INDEX(table[Tên], 1)', vi: 'Phần tử thứ n (từ 1).', en: 'The nth item, counting from 1.' },
  LOOKUP: { group: 'list', sig: 'LOOKUP(key, keys, values, [default])', ex: 'LOOKUP([Mã], gia[Mã], gia[Giá])', vi: 'Tìm khoá trong một cột, trả giá trị cùng hàng ở cột khác.', en: 'Find a key in one column; the value beside it in another.' },

  CONCAT: { group: 'text', sig: 'CONCAT(a, b, …)', ex: 'CONCAT([Họ], " ", [Tên])', vi: 'Nối chữ (như &).', en: 'Join text (like &).' },
  JOIN: { group: 'text', sig: 'JOIN(list, [separator])', ex: 'JOIN([Nhãn], " · ")', vi: 'Nối các phần tử của một danh sách.', en: 'Join the items of a list.' },
  LEN: { group: 'text', sig: 'LEN(text_or_list)', ex: 'LEN([Ghi chú])', vi: 'Số ký tự, hoặc số phần tử.', en: 'How many characters, or items.' },
  LEFT: { group: 'text', sig: 'LEFT(text, [count])', ex: 'LEFT([Mã], 3)', vi: 'Các ký tự đầu.', en: 'The first characters.' },
  RIGHT: { group: 'text', sig: 'RIGHT(text, [count])', ex: 'RIGHT([Số TK], 4)', vi: 'Các ký tự cuối.', en: 'The last characters.' },
  MID: { group: 'text', sig: 'MID(text, start, count)', ex: 'MID([Mã], 2, 3)', vi: 'Các ký tự ở giữa (đếm từ 1).', en: 'Characters from the middle, counting from 1.' },
  UPPER: { group: 'text', sig: 'UPPER(text)', ex: 'UPPER([Mã])', vi: 'Viết hoa.', en: 'Upper case.' },
  LOWER: { group: 'text', sig: 'LOWER(text)', ex: 'LOWER([Email])', vi: 'Viết thường.', en: 'Lower case.' },
  TRIM: { group: 'text', sig: 'TRIM(text)', ex: 'TRIM([Tên])', vi: 'Bỏ khoảng trắng thừa.', en: 'Extra spaces removed.' },
  REPLACE: { group: 'text', sig: 'REPLACE(text, find, with)', ex: 'REPLACE([SĐT], " ", "")', vi: 'Thay mọi chỗ tìm thấy.', en: 'Every occurrence replaced.' },
  CONTAINS: { group: 'text', sig: 'CONTAINS(text_or_list, find)', ex: 'CONTAINS([Nhãn], "gấp")', vi: 'Có chứa không (không phân biệt hoa thường).', en: 'Whether it contains it, ignoring case.' },
  STARTSWITH: { group: 'text', sig: 'STARTSWITH(text, prefix)', ex: 'STARTSWITH([Mã], "HN")', vi: 'Có bắt đầu bằng không.', en: 'Whether it starts with it.' },
  SPLIT: { group: 'text', sig: 'SPLIT(text, [separator])', ex: 'SPLIT([Tác giả], ";")', vi: 'Tách thành danh sách.', en: 'Split into a list.' },
  TEXT: { group: 'text', sig: 'TEXT(value, [pattern])', ex: 'TEXT([Ngày], "MM/YYYY")', vi: 'Viết số hoặc ngày theo mẫu: "#,##0.00", "#.##", "0%", "dd/mm/yyyy", "d MMM yyyy", "HH:mm".', en: 'A number or date as text, by a pattern: "#,##0.00", "#.##", "0%", "dd/mm/yyyy", "d MMM yyyy", "HH:mm".' },
  VALUE: { group: 'text', sig: 'VALUE(text)', ex: 'VALUE([Mã số])', vi: 'Đọc chữ thành số.', en: 'Text read as a number.' },

  TODAY: { group: 'date', sig: 'TODAY()', ex: 'TODAY() - [Ngày]', vi: 'Hôm nay. Tính lại mỗi khi mở note.', en: 'Today. Recomputed whenever the note is opened.' },
  NOW: { group: 'date', sig: 'NOW()', ex: 'NOW()', vi: 'Bây giờ, đến phút.', en: 'Now, to the minute.' },
  DATE: { group: 'date', sig: 'DATE(year, month, date)', ex: 'DATE(2026, 10, 1)', vi: 'Một ngày.', en: 'A date.' },
  YEAR: { group: 'date', sig: 'YEAR(date)', ex: 'YEAR([Ngày])', vi: 'Năm.', en: 'The year.' },
  MONTH: { group: 'date', sig: 'MONTH(date)', ex: 'MONTH([Ngày])', vi: 'Tháng (1–12).', en: 'The month (1–12).' },
  DAY: { group: 'date', sig: 'DAY(date)', ex: 'DAY([Ngày])', vi: 'Ngày trong tháng.', en: 'The day of the month.' },
  HOUR: { group: 'date', sig: 'HOUR(datetime)', ex: 'HOUR([Lúc])', vi: 'Giờ.', en: 'The hour.' },
  MINUTE: { group: 'date', sig: 'MINUTE(datetime)', ex: 'MINUTE([Lúc])', vi: 'Phút.', en: 'The minute.' },
  WEEKDAY: { group: 'date', sig: 'WEEKDAY(date)', ex: 'WEEKDAY([Ngày])', vi: 'Thứ: 1 là Chủ nhật, 7 là thứ Bảy.', en: 'The day of the week: 1 is Sunday, 7 Saturday.' },
  WEEKNUM: { group: 'date', sig: 'WEEKNUM(date, [type])', ex: 'WEEKNUM([Ngày], 2)', vi: 'Tuần thứ mấy trong năm, như Excel: tuần 1 chứa ngày 1/1, tuần bắt đầu Chủ nhật; 2 là thứ Hai, 21 là tuần ISO.', en: 'The week of the year, as Excel counts it: week 1 holds 1 January, weeks start on Sunday; 2 starts them on Monday, 21 is the ISO week.' },
  ISOWEEKNUM: { group: 'date', sig: 'ISOWEEKNUM(date)', ex: 'ISOWEEKNUM([Ngày])', vi: 'Tuần ISO: bắt đầu thứ Hai, tuần 1 chứa thứ Năm đầu tiên của năm.', en: 'The ISO week: weeks start on Monday, week 1 holds the first Thursday of the year.' },
  DAYS: { group: 'date', sig: 'DAYS(end, start)', ex: 'DAYS([Ngày trả], [Ngày mượn])', vi: 'Số ngày giữa hai ngày.', en: 'The days between two dates.' },
  DATEADD: { group: 'date', sig: 'DATEADD(date, number, ["days"|"weeks"|"months"|"years"])', ex: 'DATEADD([Ngày], 1, "months")', vi: 'Cộng ngày, tuần, tháng hoặc năm.', en: 'Days, weeks, months or years added.' },
  EOMONTH: { group: 'date', sig: 'EOMONTH(date, [months])', ex: 'EOMONTH([Ngày])', vi: 'Ngày cuối tháng.', en: 'The last day of the month.' },
  NETWORKDAYS: { group: 'date', sig: 'NETWORKDAYS(start, end, [holidays])', ex: 'NETWORKDAYS([Bắt đầu], [Kết thúc], le[Ngày])', vi: 'Số ngày làm việc (thứ Hai–Sáu), trừ ngày nghỉ lễ nếu có.', en: 'Working days, Monday to Friday, less any holidays.' },

  PMT: { group: 'number', sig: 'PMT(rate, nper, pv, [fv], [type])', ex: 'PMT(0.1 / 12, 360, 100000)', vi: 'Khoản trả đều mỗi kỳ của một khoản vay (tiền trả ra là số âm).', en: 'The even payment per period on a loan (money paid out is negative).' },
  FV: { group: 'number', sig: 'FV(rate, nper, pmt, [pv], [type])', ex: 'FV(0.06 / 12, 120, -2000000)', vi: 'Giá trị tương lai của các khoản góp đều.', en: 'The future value of even payments.' },
  PV: { group: 'number', sig: 'PV(rate, nper, pmt, [fv], [type])', ex: 'PV(0.08 / 12, 240, 500)', vi: 'Giá trị hiện tại của các khoản trả đều.', en: 'The present value of even payments.' },
  NPV: { group: 'number', sig: 'NPV(rate, flow1, flow2, …)', ex: 'NPV(0.1, table[Dòng tiền])', vi: 'Giá trị hiện tại ròng của các dòng tiền, mỗi kỳ một khoản.', en: 'The net present value of cash flows, one per period.' },
  IRR: { group: 'number', sig: 'IRR(flows, [guess])', ex: 'IRR(table[Dòng tiền])', vi: 'Tỉ suất hoàn vốn nội bộ của các dòng tiền.', en: 'The internal rate of return of cash flows.' },
  PREV: { group: 'row', sig: 'PREV(value, [first_row])', ex: 'PREV([Số dư], 0) + [Thu] - [Chi]', vi: 'Giá trị ở hàng ngay trên — cho số dư cộng dồn.', en: 'The value at the row above — for running totals.' },
  ROW: { group: 'row', sig: 'ROW()', ex: 'ROW()', vi: 'Số thứ tự của hàng trong file.', en: 'The row number, in file order.' },
  LET: { group: 'row', sig: 'LET(name, value, …, result)', ex: 'LET(net, [Gross] - [Thuế], IF(net < 0, 0, net))', vi: 'Đặt tên cho một giá trị để dùng lại.', en: 'Name a value, to use it again.' },
};
