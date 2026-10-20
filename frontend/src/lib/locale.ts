import type { Unit } from '#lib/api.js';

/**
 * How one language writes numbers, money and dates.
 *
 * The formatters are built from the tag that is passed in, never from a
 * module-level `Intl` instance that a caller could mutate. Two reasons, and the
 * second is the one that bites:
 *
 * 1. A test can ask for any locale without arranging global state first.
 * 2. The app switches languages at runtime. A formatter cached at import time
 *    would keep formatting money in the language the page was first loaded in,
 *    which is the kind of bug that only shows up after someone toggles twice.
 *
 * Money is never a float on this path. `formatMoney` groups the integer digits
 * it was given and emits the two fraction digits verbatim; nothing here parses
 * an amount into a `number`, so `184300.00` cannot become `184300.00000000003`.
 */

/** A decimal string with exactly two decimals, as the API sends and accepts. */
export type Amount = string;

/** The date/time pieces the ledger renders. */
export type DateStyle = 'day-chip' | 'time';

export interface LocaleFormat {
  /**
   * Formats an exact decimal amount as currency, in this locale.
   *
   * `en` renders `$1,234.56`; `es-AR` renders `$ 1.234,56`. The sign goes
   * before the symbol in both, matching `Intl`.
   */
  money(amount: Amount): string;

  /**
   * Reads cashier-entered text as an amount, in this locale.
   *
   * Returns the wire form (`.` decimal, no grouping, two decimals) or `null`.
   * `null` is what stops a misread value from reaching an immutable ledger.
   */
  parseAmount(text: string): Amount | null;

  /** A whole, positive count of base units. */
  quantity(value: number): string;

  /** A count with its unit label — `1.500 g` — never converted. */
  quantityWithUnit(value: number, unit: Unit): string;

  /** The unit as a person reads it. `g`/`ml` are symbols and do not translate. */
  unit(unit: Unit): string;

  /** The `$ / unit` label a price field carries, so a number cannot be misread. */
  priceUnit(unit: Unit): string;

  /**
   * An example amount, written the way this locale types one.
   *
   * A placeholder that shows `0.00` in a form that reads `1.234,56` teaches the
   * wrong shape: the cashier copies it, the parser (correctly) refuses it, and
   * the form sits disabled with no explanation (S5). Derived from the same
   * `money`/`parseAmount` pair, so the hint and the parser cannot disagree.
   */
  amountHint(): string;

  /**
   * The wire form of an amount, written for a field this locale reads.
   *
   * `parseAmount`'s mirror image, and the reason it exists: a value taken from
   * the API (`25.00`) and dropped into an es-AR field is rejected by that
   * field's own parser. Anything pre-filling an amount field goes through here
   * so that what the form shows can always be parsed back (S5).
   */
  amountInput(amount: Amount): string;

  /**
   * A calendar day, for the ledger's scope chip.
   *
   * `zone` is the shop's IANA timezone and must be passed wherever it is known:
   * the day chip *is* the shop's day, so rendering it from the browser's clock
   * would label the shop's 16 Oct as 17 Oct on a UTC machine while showing
   * 16-Oct rows underneath (S19). Unlike `time`, `zone` is listed before the
   * fallback so callers cannot omit it by accident and get a plausible answer.
   */
  day(date: Date, zone?: string): string;

  /**
   * The name of a calendar month, for the ledger's chip when the page is scoped
   * to a month. "octubre de 2026", not a day: a month page is not about one day.
   */
  month(date: Date, zone?: string): string;

  /**
   * A wall-clock time, for a document's stamp.
   *
   * `zone` names the shop's IANA timezone. It is required of every caller that
   * knows it: a document inside a shop day must read on the shop's clock, or the
   * page shows a 22:30 sale as 01:30 because the machine opening it is set to
   * UTC (S19). Omitted, it falls back to the browser's zone, which is only
   * correct when the two happen to agree.
   */
  time(date: Date, zone?: string): string;
}

/** Building blocks shared by both locales: the *rules*, not the symbols. */
function parseAmountWith(text: string, group: string, decimal: string): Amount | null {
  // A single leading currency symbol is an artifact of a previous render, not
  // kind of the number. It is stripped so that a value copied off the screen
  // can be pasted back.
  const normalized = text
    .trim()
    .replace(/^\$\s*/, '')
    .trim();
  const groupRe = group === '.' ? '\\.' : ',';
  const decRe = decimal === ',' ? ',' : '\\.';

  // Integer part: either ungrouped digits, or correctly grouped thousands.
  // Correct grouping is what keeps `1.234,56` readable while refusing
  // `1.23.4,56`.
  const grouped = new RegExp(`^\\d{1,3}(?:${groupRe}\\d{3})+(?:${decRe}\\d{1,2})?$`);
  const plain = new RegExp(`^\\d+(?:${decRe}\\d{1,2})?$`);
  if (!grouped.test(normalized) && !plain.test(normalized)) return null;

  const withoutGroups = normalized.split(group).join('');
  const [wholeRaw, fractionRaw = ''] = withoutGroups.split(decimal);
  const whole = wholeRaw.replace(/^0+(?=\d)/, '');
  return `${whole}.${fractionRaw.padEnd(2, '0')}`;
}

/**
 * Groups the integer digits of an amount with `separator`, without ever turning
 * the amount into a number.
 *
 * `BigInt` does the grouping because the digit string can be longer than a float
 * can hold exactly, and `Intl.NumberFormat.format(BigInt)` is exact.
 */
function groupDigits(digits: string, separator: string): string {
  if (separator === ',') {
    return new Intl.NumberFormat('en-US').format(BigInt(digits));
  }
  // Any locale whose group separator is "." produces the same digit grouping.
  return new Intl.NumberFormat('es-AR').format(BigInt(digits));
}

function makeMoney(opts: {
  /** Between the symbol and the digits. `Intl` puts a space there in es-AR. */
  space: string;
  group: string;
  decimal: string;
}): (amount: Amount) => string {
  return (amount) => {
    const negative = amount.startsWith('-');
    const unsigned = negative ? amount.slice(1) : amount;
    const [whole, fraction] = unsigned.split('.');
    const grouped = groupDigits(whole, opts.group);
    const sign = negative ? '-' : '';
    return `${sign}$${opts.space}${grouped}${opts.decimal}${fraction}`;
  };
}

function makeQuantityFmt(group: string): (value: number) => string {
  return (value) => groupDigits(String(value), group);
}

function makeDateFmt(tag: string) {
  // `dateStyle: 'medium'`, not a field combination: in es-AR the loose fields
  // render `16 de oct de 2026`, while the medium style is the compact
  // `16 oct 2026` a chip needs. Same style, both languages, so the chip does
  // not change width class when the language does.
  //
  // Cached per zone, like the time formatter, for the same reason: `Intl`
  // construction dominates a per-row render, and the tz database is what makes
  // a DST zone correct without an offset this code would have to maintain.
  const byZone = new Map<string, Intl.DateTimeFormat>();
  return (date: Date, zone?: string) => {
    if (!zone) return new Intl.DateTimeFormat(tag, { dateStyle: 'medium' }).format(date);
    let fmt = byZone.get(zone);
    if (!fmt) {
      fmt = new Intl.DateTimeFormat(tag, { dateStyle: 'medium', timeZone: zone });
      byZone.set(zone, fmt);
    }
    return fmt.format(date);
  };
}

function makeMonthFmt(tag: string) {
  // A month chip names the month, not a day inside it. Same caching reason as
  // the day formatter, and the same `Intl` tz database so the month is the
  // shop's month rather than the browser's.
  const byZone = new Map<string, Intl.DateTimeFormat>();
  return (date: Date, zone?: string) => {
    const opts: Intl.DateTimeFormatOptions = { year: 'numeric', month: 'long' };
    if (!zone) return new Intl.DateTimeFormat(tag, opts).format(date);
    let fmt = byZone.get(zone);
    if (!fmt) {
      fmt = new Intl.DateTimeFormat(tag, { ...opts, timeZone: zone });
      byZone.set(zone, fmt);
    }
    return fmt.format(date);
  };
}

function makeTimeFmt(tag: string) {
  // The formatter is cached per zone because `Intl.DateTimeFormat` construction
  // is the expensive part and the ledger renders one per row. `Intl` itself
  // carries the tz database, so a zone whose rules change is handled without an
  // offset this code would have to maintain.
  const byZone = new Map<string, Intl.DateTimeFormat>();
  return (date: Date, zone?: string) => {
    const options: Intl.DateTimeFormatOptions = {
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    };
    if (!zone) return new Intl.DateTimeFormat(tag, options).format(date);
    let fmt = byZone.get(zone);
    if (!fmt) {
      fmt = new Intl.DateTimeFormat(tag, { ...options, timeZone: zone });
      byZone.set(zone, fmt);
    }
    return fmt.format(date);
  };
}

/** Spanish (Argentina): the shop's own language. */
export const esAR: LocaleFormat = (() => {
  const group = '.';
  const decimal = ',';
  const money = makeMoney({ space: ' ', group, decimal });
  const count = makeQuantityFmt(group);
  const unitLabel = (unit: Unit) => (unit === 'unit' ? 'unidad' : unit === 'g' ? 'g' : 'ml');
  return {
    money,
    parseAmount: (text) => parseAmountWith(text, group, decimal),
    quantity: count,
    // A unit product's quantity is already a count of items: the column header
    // says what it counts, so repeating a label here would only add noise. A
    // measured product carries its symbol, verbatim and never converted.
    quantityWithUnit: (value, unit) => (unit === 'unit' ? count(value) : `${count(value)} ${unit}`),
    unit: unitLabel,
    priceUnit: (unit) => `$ / ${unitLabel(unit)}`,
    amountHint: () => '0,00',
    amountInput: (amount) => amount.replace('.', ','),
    day: makeDateFmt('es-AR'),
    month: makeMonthFmt('es-AR'),
    time: makeTimeFmt('es-AR')
  };
})();

/** English: the language the specifications and the code are written in. */
export const en: LocaleFormat = (() => {
  const group = ',';
  const decimal = '.';
  const money = makeMoney({ space: '', group, decimal });
  const count = makeQuantityFmt(group);
  const unitLabel = (unit: Unit) => (unit === 'unit' ? 'unit' : unit === 'g' ? 'g' : 'ml');
  return {
    money,
    parseAmount: (text) => parseAmountWith(text, group, decimal),
    quantity: count,
    quantityWithUnit: (value, unit) => (unit === 'unit' ? count(value) : `${count(value)} ${unit}`),
    unit: unitLabel,
    priceUnit: (unit) => `$/${unitLabel(unit)}`,
    amountHint: () => '0.00',
    amountInput: (amount) => amount,
    day: makeDateFmt('en'),
    month: makeMonthFmt('en'),
    time: makeTimeFmt('en')
  };
})();
