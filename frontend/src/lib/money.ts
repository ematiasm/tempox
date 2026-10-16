import type { Unit } from './api';

const WHOLE_AMOUNT = new Intl.NumberFormat('en-US');

const COUNT = new Intl.NumberFormat('en-US');

/** A decimal string with exactly two decimals, as the API sends and accepts. */
export type Amount = string;

/** Formats an exact decimal amount as en-US currency without converting it to a number. */
export function formatMoney(amount: Amount): string {
  const negative = amount.startsWith('-');
  const unsigned = negative ? amount.slice(1) : amount;
  const [whole, fraction] = unsigned.split('.');
  const formattedWhole = WHOLE_AMOUNT.format(BigInt(whole));
  return `${negative ? '-$' : '$'}${formattedWhole}.${fraction}`;
}

/** Validates and normalizes cashier-entered text to the API's two-decimal amount string. */
export function parseAmount(text: string): Amount | null {
  const normalized = text.trim().replace(/^\$/, '');
  const match = /^(\d+|\d{1,3}(?:,\d{3})+)(?:\.(\d{1,2}))?$/.exec(normalized);
  if (!match) return null;

  const whole = match[1].replace(/,/g, '').replace(/^0+(?=\d)/, '');
  const fraction = (match[2] ?? '').padEnd(2, '0');
  return `${whole}.${fraction}`;
}

/** A whole, positive count of base units — or `null`. */
export function parseQuantity(text: string): number | null {
  const normalized = text.trim();
  // No decimal point: a gram product's quantity is 1 or 2 grams, never 1.5.
  if (!/^\d+$/.test(normalized)) return null;
  const value = Number(normalized);
  if (!Number.isSafeInteger(value) || value < 1) return null;
  return value;
}

/** A quantity with the backend's unit, rendered verbatim. */
export function formatQuantity(quantity: number, unit: Unit): string {
  const count = COUNT.format(quantity);
  return unit === 'unit' ? count : `${count} ${unit}`;
}

/** Computes an exact draft-sale line total; this preview is not the recorded total. */
export function lineTotalAmount(quantity: number, unitPrice: Amount): Amount | null {
  if (!Number.isSafeInteger(quantity) || quantity <= 0 || !/^\d+\.\d{2}$/.test(unitPrice)) {
    return null;
  }

  const [whole, fraction] = unitPrice.split('.');
  const cents = (BigInt(whole) * 100n + BigInt(fraction)) * BigInt(quantity);
  const digits = cents.toString().padStart(3, '0');
  return `${digits.slice(0, -2)}.${digits.slice(-2)}`;
}

/**
 * Adds amounts exactly, as a sum of whole cents.
 *
 * Used where a figure on screen genuinely has to be derived from other
 * amounts — the draft preview and the ledger's page footing. It stays in
 * `BigInt` cents so no float ever multiplies or divides money; the result is an
 * `Amount` like every other, and the recorded total still comes from the server.
 *
 * Returns `null` if any amount is not a well-formed `Amount`, so a caller cannot
 * silently sum garbage into a total.
 */
export function sumAmounts(amounts: readonly Amount[]): Amount | null {
  let cents = 0n;
  for (const amount of amounts) {
    if (!/^-?\d+\.\d{2}$/.test(amount)) return null;
    const negative = amount.startsWith('-');
    const [whole, fraction] = (negative ? amount.slice(1) : amount).split('.');
    const value = BigInt(whole) * 100n + BigInt(fraction);
    cents += negative ? -value : value;
  }
  const negative = cents < 0n;
  const digits = (negative ? -cents : cents).toString().padStart(3, '0');
  return `${negative ? '-' : ''}${digits.slice(0, -2)}.${digits.slice(-2)}`;
}
