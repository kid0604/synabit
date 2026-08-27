import { ref } from 'vue';

/**
 * Money, and the two forms it takes.
 *
 * Everything the app stores and adds up is an **integer number of minor
 * units** — cents, xu, pence. Nothing anywhere multiplies or adds a
 * fractional currency value, because floating point cannot hold 0.1 and a
 * ledger that is out by a hundredth after ten thousand additions is a ledger
 * nobody trusts.
 *
 * Major units — the 12.50 a person types and reads — exist only at the two
 * edges: `parseAmountInput` on the way in, `formatCurrency` on the way out.
 *
 * Before this, every amount field read its input as `raw.replace(/\D/g, '')`.
 * That is exactly right for đồng, which has no subunit, and silently wrong for
 * every other currency the app offers: `12.50` was stored as 1250 major units,
 * a hundredfold error. It also swallowed the minus sign, so the credit card
 * account the app creates by default could not be opened with the balance it
 * actually has.
 */

export const currentCurrency = ref('USD');

/**
 * Whether the app is allowed to ask the network what a currency is worth.
 *
 * Off by default. Synabit's own description of itself is "zero telemetry, no
 * forced cloud accounts", and a request to a CDN the moment somebody picks a
 * foreign currency is a request they did not agree to. With this off the rate
 * has to be typed, which is a small cost for a feature most ledgers use once.
 */
export const allowRateLookup = ref(false);

// ---------------------------------------------------------------------------
// Scale
// ---------------------------------------------------------------------------

/**
 * Currencies whose minor unit is not a hundredth, from ISO 4217.
 *
 * Frozen on purpose, and not read from `Intl`. This number decides how a
 * stored amount is interpreted, so if it ever changed — because a device had
 * newer locale data than another — the same file would be read as two
 * different amounts on two machines. `Intl` is consulted for punctuation,
 * which is only ever cosmetic; never for this.
 */
const ZERO_DECIMAL = [
  'BIF', 'CLP', 'DJF', 'GNF', 'ISK', 'JPY', 'KMF', 'KRW', 'PYG', 'RWF',
  'UGX', 'UYI', 'VND', 'VUV', 'XAF', 'XOF', 'XPF',
];

const THREE_DECIMAL = ['BHD', 'IQD', 'JOD', 'KWD', 'LYD', 'OMR', 'TND'];

/** How many digits of minor unit a currency has. */
export const currencyScale = (currency: string): number => {
  const code = currency.toUpperCase();
  if (ZERO_DECIMAL.includes(code)) return 0;
  if (THREE_DECIMAL.includes(code)) return 3;
  return 2;
};

/**
 * The whole table, for handing to the storage migration.
 *
 * The migration multiplies every stored amount by a power of ten, so it has to
 * agree with this file exactly or it will scale a vault wrongly and there is no
 * way back. Passing the table rather than reimplementing it in Rust is what
 * makes that agreement structural instead of hopeful.
 */
export const currencyScaleTable = (): Record<string, number> => {
  const table: Record<string, number> = {};
  for (const code of ZERO_DECIMAL) table[code] = 0;
  for (const code of THREE_DECIMAL) table[code] = 3;
  return table;
};

/** Minor units as the number a person would say. Display only. */
export const toMajor = (minor: number, currency: string = currentCurrency.value): number =>
  minor / 10 ** currencyScale(currency);

/** The reverse, for a value that arrived as major units. */
export const toMinor = (major: number, currency: string = currentCurrency.value): number =>
  Math.round(major * 10 ** currencyScale(currency));

// ---------------------------------------------------------------------------
// Punctuation
// ---------------------------------------------------------------------------

/** The locale whose number formatting suits a currency. */
export const localeForCurrency = (currency: string): string => {
  if (currency === 'VND') return 'vi-VN';
  if (currency === 'EUR') return 'de-DE';
  if (currency === 'JPY') return 'ja-JP';
  if (currency === 'GBP') return 'en-GB';
  return 'en-US';
};

const separatorCache = new Map<string, string>();

/**
 * The character this locale puts before the fractional part.
 *
 * A full stop in English, a comma in Vietnamese and German — and in those two
 * the full stop is the *group* separator, so reading it as a decimal point
 * would turn `1.500.000` into one and a half.
 */
export const decimalSeparatorFor = (currency: string = currentCurrency.value): string => {
  const locale = localeForCurrency(currency);
  const cached = separatorCache.get(locale);
  if (cached) return cached;

  const found =
    new Intl.NumberFormat(locale)
      .formatToParts(1.1)
      .find((part) => part.type === 'decimal')?.value ?? '.';
  separatorCache.set(locale, found);
  return found;
};

const escapeForClass = (s: string) => s.replace(/[.*+?^${}()|[\]\\-]/g, '\\$&');

/** Split what somebody typed into a sign, whole digits and fraction digits. */
const dissect = (raw: string, currency: string) => {
  const scale = currencyScale(currency);
  const decimal = decimalSeparatorFor(currency);
  const negative = /^\s*-/.test(raw);

  // Anything that is not a digit or this locale's decimal mark is punctuation
  // the user or the formatter put there; group separators included.
  const keep = new RegExp(`[^0-9${escapeForClass(decimal)}]`, 'g');
  const cleaned = raw.replace(keep, '');

  const parts = cleaned.split(decimal);
  const whole = parts[0] ?? '';
  // A currency with no subunit has no fractional part, whatever was typed.
  const typedFraction = scale > 0 ? parts.slice(1).join('') : '';

  return {
    scale,
    decimal,
    negative,
    whole,
    // Digits past the currency's precision are dropped rather than rounded, so
    // that what the field shows and what is stored never disagree.
    fraction: typedFraction.slice(0, scale),
    hasDecimal: scale > 0 && parts.length > 1,
  };
};

// ---------------------------------------------------------------------------
// The edges
// ---------------------------------------------------------------------------

/**
 * What somebody typed into an amount field, in minor units.
 *
 * Understands the grouping the field itself applied, a decimal part, and a
 * leading minus. Whether a negative amount is *allowed* is the caller's
 * business: a transaction amount is not, because the sign is carried by the
 * transaction's type, while an opening balance is.
 */
export const parseAmountInput = (
  raw: string,
  currency: string = currentCurrency.value,
): number => {
  const { scale, negative, whole, fraction } = dissect(raw, currency);
  if (!whole && !fraction) return 0;

  // Built as a string rather than by multiplying, so a large amount cannot
  // pick up a rounding error on its way to being an integer.
  const minor = Number(`${whole || '0'}${fraction.padEnd(scale, '0')}`);
  return negative ? -minor : minor;
};

/**
 * The same digits, punctuated — for putting straight back into the field on
 * every keystroke.
 *
 * The fractional part is left exactly as typed rather than padded, so that
 * `12.` and `12.5` survive being typed through. Only the whole part is
 * grouped, which is also what keeps the caret from jumping.
 */
export const formatAmountInput = (
  raw: string,
  currency: string = currentCurrency.value,
): string => {
  const { decimal, negative, whole, fraction, hasDecimal } = dissect(raw, currency);
  if (!whole && !fraction && !hasDecimal) return negative ? '-' : '';

  const locale = localeForCurrency(currency);
  const grouped = Number(whole || '0').toLocaleString(locale, { maximumFractionDigits: 0 });
  const body = hasDecimal ? `${grouped}${decimal}${fraction}` : grouped;
  return negative ? `-${body}` : body;
};

/** A stored amount, as the text an amount field should start out holding. */
export const formatMinorForInput = (
  minor: number,
  currency: string = currentCurrency.value,
): string => {
  const scale = currencyScale(currency);
  const rounded = Math.round(minor);
  const negative = rounded < 0;
  const abs = Math.abs(rounded);

  const unit = 10 ** scale;
  const locale = localeForCurrency(currency);
  const grouped = Math.floor(abs / unit).toLocaleString(locale, { maximumFractionDigits: 0 });

  const body =
    scale > 0
      ? `${grouped}${decimalSeparatorFor(currency)}${String(abs % unit).padStart(scale, '0')}`
      : grouped;

  return negative ? `-${body}` : body;
};

/** An amount in minor units, as money. */
export const formatCurrency = (minor: number) => {
  const currency = currentCurrency.value;
  return new Intl.NumberFormat(localeForCurrency(currency), {
    style: 'currency',
    currency,
  }).format(toMajor(minor, currency));
};

/**
 * An amount in minor units, shortened for an axis label.
 *
 * Rounds to whole units first: a chart tick reading `1.2K` means twelve
 * hundred of something a person recognises, never a hundred and twenty
 * thousand cents.
 */
export const formatCompact = (minor: number, currency: string = currentCurrency.value): string => {
  const major = toMajor(minor, currency);
  const abs = Math.abs(major);
  if (abs >= 1_000_000) return `${(major / 1_000_000).toFixed(1)}M`;
  if (abs >= 1_000) return `${(major / 1_000).toFixed(0)}K`;
  return Math.round(major).toString();
};

/**
 * An amount converted between currencies, minor units to minor units.
 *
 * The rate is quoted in major units — 25,400 đồng to the dollar — so it cannot
 * be applied to minor units directly when the two scales differ.
 */
export const convertMinor = (
  minor: number,
  from: string,
  to: string,
  rate: number,
): number => Math.round(toMajor(minor, from) * rate * 10 ** currencyScale(to));

// ---------------------------------------------------------------------------
// The currency list
// ---------------------------------------------------------------------------

/** The handful most people will be looking for, offered before the rest. */
export const COMMON_CURRENCIES = ['USD', 'EUR', 'GBP', 'JPY', 'VND', 'AUD', 'CAD', 'CNY', 'INR', 'SGD'];

/**
 * Every currency this runtime can format.
 *
 * `Intl.supportedValuesOf` has been Baseline widely available since 2024 and
 * needs no fallback under the project's policy — but a WebView old enough to
 * lack it would render an empty picker, and the common list is a cheaper
 * answer to that than a polyfill.
 *
 * Reached for through a cast because the project compiles against the ES2020
 * library and this call arrived in ES2022; widening `lib` for one function
 * would change how every other file in the app type-checks.
 */
export const allCurrencies = (): string[] => {
  const supported = (Intl as unknown as {
    supportedValuesOf?: (key: 'currency') => string[];
  }).supportedValuesOf;

  return supported ? supported('currency') : [...COMMON_CURRENCIES].sort();
};

// ---------------------------------------------------------------------------
// Exchange rates
// ---------------------------------------------------------------------------

const CACHE_KEY = 'synabit_exchange_rates';

type RateCache = Record<string, Record<string, number>>;

const readCache = (): RateCache => {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? (JSON.parse(raw) as RateCache) : {};
  } catch {
    return {};
  }
};

/**
 * What one unit of `from` is worth in `to`, or `null` if nobody can say.
 *
 * Returns `null` rather than 1 when it does not know: a rate of 1 between two
 * different currencies is a wrong answer that looks like a right one, and it
 * would be written into the ledger as fact.
 *
 * Never reaches the network unless `allowRateLookup` was turned on. A rate
 * cached from a previous lookup is still offered, because it is already on
 * this machine and asking nobody for it costs nothing.
 */
export const fetchExchangeRate = async (
  fromCurrency: string,
  toCurrency: string,
): Promise<number | null> => {
  const from = fromCurrency.toLowerCase();
  const to = toCurrency.toLowerCase();
  if (from === to) return 1;

  const cached = readCache();
  const fromCache = cached[from]?.[to] ?? null;

  if (!allowRateLookup.value) return fromCache;

  try {
    const response = await fetch(
      `https://cdn.jsdelivr.net/npm/@fawazahmed0/currency-api@latest/v1/currencies/${from}.json`,
    );
    if (!response.ok) throw new Error(`Rate lookup failed: ${response.status}`);

    const data = await response.json();
    const rate = data?.[from]?.[to];
    if (typeof rate !== 'number') return fromCache;

    cached[from] = data[from];
    try {
      localStorage.setItem(CACHE_KEY, JSON.stringify(cached));
    } catch {
      // A full quota is not a reason to refuse the rate we just fetched.
    }
    return rate;
  } catch {
    return fromCache;
  }
};
