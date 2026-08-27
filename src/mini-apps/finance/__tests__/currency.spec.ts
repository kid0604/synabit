import { describe, it, expect, afterEach } from 'vitest';
import {
  COMMON_CURRENCIES,
  allCurrencies,
  allowRateLookup,
  convertMinor,
  currencyScale,
  currencyScaleTable,
  currentCurrency,
  decimalSeparatorFor,
  formatAmountInput,
  formatCompact,
  formatCurrency,
  formatMinorForInput,
  localeForCurrency,
  parseAmountInput,
  toMajor,
  toMinor,
} from '../currency';

/**
 * The two tests that used to be `it.fails` here — a decimal amount and a
 * negative one — are ordinary tests now. They were roadmap 1.1, and this file
 * is where that work lands.
 */

const withCurrency = (code: string, body: () => void) => {
  const previous = currentCurrency.value;
  currentCurrency.value = code;
  try {
    body();
  } finally {
    currentCurrency.value = previous;
  }
};

afterEach(() => {
  allowRateLookup.value = false;
});

describe('currencyScale', () => {
  it('gives a hundredth to the currencies that have one', () => {
    expect(currencyScale('USD')).toBe(2);
    expect(currencyScale('EUR')).toBe(2);
    expect(currencyScale('GBP')).toBe(2);
  });

  it('gives no subunit to đồng, yen and won', () => {
    expect(currencyScale('VND')).toBe(0);
    expect(currencyScale('JPY')).toBe(0);
    expect(currencyScale('KRW')).toBe(0);
  });

  it('gives three digits to the dinars that use them', () => {
    expect(currencyScale('KWD')).toBe(3);
    expect(currencyScale('BHD')).toBe(3);
  });

  it('assumes a hundredth for a currency it has never heard of', () => {
    expect(currencyScale('ZZZ')).toBe(2);
  });

  it('does not care about case', () => {
    expect(currencyScale('vnd')).toBe(0);
  });
});

describe('currencyScaleTable', () => {
  /**
   * The storage migration multiplies by a power of ten taken from this table.
   * If it ever disagreed with `currencyScale`, a vault would be scaled wrongly
   * and there would be no way back.
   */
  it('says the same as currencyScale for every currency it names', () => {
    const table = currencyScaleTable();
    for (const [code, scale] of Object.entries(table)) {
      expect(currencyScale(code), code).toBe(scale);
    }
  });

  it('names every currency that is not the ordinary two digits', () => {
    const table = currencyScaleTable();
    expect(table.VND).toBe(0);
    expect(table.KWD).toBe(3);
    expect(table.USD).toBeUndefined();
  });
});

describe('toMinor and toMajor', () => {
  it('are inverses', () => {
    expect(toMajor(toMinor(12.5, 'USD'), 'USD')).toBe(12.5);
    expect(toMajor(toMinor(150000, 'VND'), 'VND')).toBe(150000);
  });

  it('leave a currency with no subunit alone', () => {
    expect(toMinor(150000, 'VND')).toBe(150000);
  });

  it('round a major amount finer than the currency can hold', () => {
    expect(toMinor(12.567, 'USD')).toBe(1257);
  });
});

describe('localeForCurrency', () => {
  it('gives each supported currency its own conventions', () => {
    expect(localeForCurrency('VND')).toBe('vi-VN');
    expect(localeForCurrency('EUR')).toBe('de-DE');
    expect(localeForCurrency('JPY')).toBe('ja-JP');
    expect(localeForCurrency('GBP')).toBe('en-GB');
  });

  it('falls back to US English for anything it does not know', () => {
    expect(localeForCurrency('USD')).toBe('en-US');
    expect(localeForCurrency('SEK')).toBe('en-US');
  });
});

describe('decimalSeparatorFor', () => {
  it('is a full stop in English and a comma in Vietnamese', () => {
    expect(decimalSeparatorFor('USD')).toBe('.');
    expect(decimalSeparatorFor('VND')).toBe(',');
    expect(decimalSeparatorFor('EUR')).toBe(',');
  });
});

describe('parseAmountInput', () => {
  it('reads a plain number', () => {
    expect(parseAmountInput('1500', 'USD')).toBe(150000);
  });

  it('reads a number the field has already grouped', () => {
    expect(parseAmountInput('1,500,000', 'USD')).toBe(150000000);
    expect(parseAmountInput('1.500.000', 'VND')).toBe(1500000);
  });

  it('treats an empty field as zero rather than as nothing', () => {
    expect(parseAmountInput('', 'USD')).toBe(0);
    expect(parseAmountInput('abc', 'USD')).toBe(0);
  });

  /** Roadmap 1.1. This was the hundredfold error. */
  it('keeps the minor units of a decimal amount', () => {
    expect(parseAmountInput('12.50', 'USD')).toBe(1250);
    expect(parseAmountInput('12.5', 'USD')).toBe(1250);
    expect(parseAmountInput('0.07', 'USD')).toBe(7);
  });

  /** Roadmap 1.1. A credit card opens with the balance it actually has. */
  it('keeps a negative amount negative', () => {
    expect(parseAmountInput('-5000', 'USD')).toBe(-500000);
    expect(parseAmountInput('-12.50', 'USD')).toBe(-1250);
  });

  /**
   * In Vietnamese the full stop groups and the comma divides. Reading the
   * full stop as a decimal point would turn one and a half million into one
   * and a half.
   */
  it('reads the separators the way the currency does', () => {
    expect(parseAmountInput('1.234,56', 'EUR')).toBe(123456);
    expect(parseAmountInput('1,234.56', 'USD')).toBe(123456);
  });

  /**
   * Đồng has no subunit, so the comma a Vietnamese keyboard puts before a
   * fraction has nothing to introduce. The digits after it are dropped, and
   * the field shows the same thing the parser read — `1.500` either way.
   */
  it('has no fractional part for a currency without one', () => {
    expect(parseAmountInput('1500,75', 'VND')).toBe(1500);
    expect(formatAmountInput('1500,75', 'VND')).toBe('1.500');
  });

  it('carries three digits for a currency that has three', () => {
    expect(parseAmountInput('1.234', 'KWD')).toBe(1234);
  });

  /** What the field shows and what is stored must never disagree. */
  it('drops digits finer than the currency can hold, rather than rounding up', () => {
    expect(parseAmountInput('12.999', 'USD')).toBe(1299);
  });

  it('reads an amount larger than any ledger will hold, exactly', () => {
    expect(parseAmountInput('999,999,999,999.99', 'USD')).toBe(99999999999999);
  });

  it('uses the vault currency when none is named', () => {
    withCurrency('VND', () => expect(parseAmountInput('1.500')).toBe(1500));
  });
});

describe('formatAmountInput', () => {
  it('groups digits the way the currency groups them', () => {
    expect(formatAmountInput('1500000', 'USD')).toBe('1,500,000');
    expect(formatAmountInput('1500000', 'VND')).toBe('1.500.000');
  });

  it('leaves a cleared field cleared instead of filling it with a zero', () => {
    expect(formatAmountInput('', 'USD')).toBe('');
    expect(formatAmountInput('abc', 'USD')).toBe('');
  });

  it('re-groups a value that was already grouped, without doubling it', () => {
    expect(formatAmountInput('1,500,000', 'USD')).toBe('1,500,000');
  });

  /**
   * Typing `12.50` means passing through `12.` and `12.5`. Padding or
   * stripping either of those takes the field away from the person using it.
   */
  it('lets a decimal amount be typed one character at a time', () => {
    expect(formatAmountInput('12', 'USD')).toBe('12');
    expect(formatAmountInput('12.', 'USD')).toBe('12.');
    expect(formatAmountInput('12.5', 'USD')).toBe('12.5');
    expect(formatAmountInput('12.50', 'USD')).toBe('12.50');
  });

  it('refuses a decimal point in a currency that has no subunit', () => {
    expect(formatAmountInput('1500.75', 'VND')).toBe('150.075');
  });

  it('keeps a minus sign while the rest is still being typed', () => {
    expect(formatAmountInput('-', 'USD')).toBe('-');
    expect(formatAmountInput('-5000', 'USD')).toBe('-5,000');
  });

  it('stops accepting fraction digits past the currency`s precision', () => {
    expect(formatAmountInput('12.5099', 'USD')).toBe('12.50');
  });
});

describe('formatMinorForInput', () => {
  it('shows a stored amount the way the field expects it back', () => {
    expect(formatMinorForInput(1250, 'USD')).toBe('12.50');
    expect(formatMinorForInput(150000000, 'USD')).toBe('1,500,000.00');
    expect(formatMinorForInput(1500000, 'VND')).toBe('1.500.000');
  });

  it('pads a fraction that would otherwise read as tens', () => {
    expect(formatMinorForInput(7, 'USD')).toBe('0.07');
  });

  it('shows a negative balance as negative', () => {
    expect(formatMinorForInput(-1250, 'USD')).toBe('-12.50');
  });

  it('round-trips through the parser', () => {
    for (const [minor, code] of [[1250, 'USD'], [-99, 'USD'], [1500000, 'VND'], [1234, 'KWD']] as const) {
      expect(parseAmountInput(formatMinorForInput(minor, code), code), code).toBe(minor);
    }
  });
});

describe('formatCurrency', () => {
  it('reads its argument as minor units', () => {
    withCurrency('USD', () => expect(formatCurrency(123450)).toBe('$1,234.50'));
  });

  it('shows a currency with no subunit without one', () => {
    withCurrency('VND', () => expect(formatCurrency(1500000)).toContain('1.500.000'));
  });
});

describe('formatCompact', () => {
  /** A tick reading `1.2K` has to mean twelve hundred dollars, not cents. */
  it('shortens whole units, not minor ones', () => {
    expect(formatCompact(120000, 'USD')).toBe('1K');
    expect(formatCompact(250000000, 'USD')).toBe('2.5M');
    expect(formatCompact(1500000, 'VND')).toBe('1.5M');
  });

  it('leaves a small amount alone', () => {
    expect(formatCompact(1250, 'USD')).toBe('13');
  });
});

describe('convertMinor', () => {
  /** The rate is quoted between whole units, so the two scales both matter. */
  it('crosses a scale boundary', () => {
    // One dollar buys 25,400 đồng.
    expect(convertMinor(100, 'USD', 'VND', 25_400)).toBe(25_400);
    // And back again.
    expect(convertMinor(25_400, 'VND', 'USD', 1 / 25_400)).toBe(100);
  });

  it('is the identity at a rate of one between like currencies', () => {
    expect(convertMinor(1250, 'USD', 'USD', 1)).toBe(1250);
  });
});

describe('allCurrencies', () => {
  it('offers more than the handful pinned to the top', () => {
    const all = allCurrencies();
    expect(all.length).toBeGreaterThan(COMMON_CURRENCIES.length);
    expect(all).toContain('USD');
    expect(all).toContain('VND');
  });
});

describe('fetchExchangeRate', () => {
  /**
   * The rest of this function talks to a CDN, which a test has no business
   * doing. What matters here is that it does not talk to one by default: the
   * app describes itself as local-first and zero-telemetry, and picking a
   * foreign currency is not consent to a network request.
   */
  it('asks nobody anything while rate lookup is off', async () => {
    const { fetchExchangeRate } = await import('../currency');
    const calls: string[] = [];
    const original = globalThis.fetch;
    globalThis.fetch = ((url: string) => {
      calls.push(String(url));
      return Promise.reject(new Error('should not be called'));
    }) as typeof fetch;

    try {
      expect(await fetchExchangeRate('USD', 'VND')).toBeNull();
      expect(calls).toEqual([]);
    } finally {
      globalThis.fetch = original;
    }
  });

  it('answers one for a currency against itself, without asking', async () => {
    const { fetchExchangeRate } = await import('../currency');
    expect(await fetchExchangeRate('USD', 'USD')).toBe(1);
  });
});
