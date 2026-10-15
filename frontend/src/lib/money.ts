/**
 * Money and quantity, as the screen needs them.
 *
 * These are the only places the UI turns typed text into numbers, so they are
 * strict on purpose: a validator that guesses turns a typo into a recorded sale.
 *
 * The rules come from `plan-v1a.md` "Pricing semantics", which is a wire-format
 * contract, not a preference:
 *   - prices are integer cents **per base unit**
 *   - quantities are integers in the product's base unit
 *   - the client never rescales either number, so nothing here divides by 1000
 *     or multiplies by 100 to "convert" a unit
 */

import type { Unit } from './api';

const MONEY = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD'
});

const COUNT = new Intl.NumberFormat('en-US');

/** Integer cents as en-US money (`2500` → `$25.00`). */
export function formatCents(cents: number): string {
  return MONEY.format(cents / 100);
}

/**
 * What a cashier typed, as integer cents — or `null` if it is not an amount.
 *
 * Accepts `25`, `25.00`, `0.01`, `1,234.56`, and `0` (a bonus line is a real
 * line). Refuses sub-cent values such as `0.005` rather than rounding them:
 * the schema stores cents per base unit, so `0.005` is not expressible, and
 * silently choosing `0.01` or `0.00` would record a price nobody typed.
 *
 * Parsed as text rather than through `parseFloat` so no binary float ever
 * touches the amount: `Math.round(19.99 * 100)` is the kind of arithmetic that
 * loses a cent on a busy day.
 */
export function parseAmountToCents(text: string): number | null {
  const normalized = text.replace(/[$,\s]/g, '');
  if (!/^\d+(\.\d{1,2})?$/.test(normalized)) return null;

  const [whole, fraction = ''] = normalized.split('.');
  const cents = Number(whole) * 100 + Number(fraction.padEnd(2, '0'));
  // Guard the ceiling: a number too large for exact integer math would start
  // rounding in a place nobody would notice.
  return Number.isSafeInteger(cents) ? cents : null;
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

/** `quantity × unit_price`, floored at zero. */
export function lineTotal(quantity: number, unitPriceCents: number): number {
  if (quantity <= 0 || unitPriceCents <= 0) return 0;
  const total = quantity * unitPriceCents;
  return Number.isSafeInteger(total) ? total : 0;
}

/**
 * A quantity with the backend's unit, rendered verbatim.
 *
 * `1500 g` stays `1,500 g`: converting to kilograms here would invent a unit
 * the API never agreed to send, and the ledger would stop matching the shelf.
 * `unit` products show a bare number, because "3 unit" is not how anyone talks.
 */
export function formatQuantity(quantity: number, unit: Unit): string {
  const count = COUNT.format(quantity);
  return unit === 'unit' ? count : `${count} ${unit}`;
}
