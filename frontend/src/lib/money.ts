/**
 * Money, quantities and dates, at the call site components already use.
 *
 * The implementations live in `locale.ts`, one per language, because formatting
 * is a property of the language on screen and not of the app. These exports are
 * the active locale's functions, so a component never has to thread a language
 * through a computation to get the right `$ 1.234,56`.
 *
 * What stays here and not in `locale.ts` is the arithmetic: a line total and a
 * sum are exact decimal operations on the wire format, and they are the same
 * operation in every language. Formatting happens once, at the edge, on the
 * string that arithmetic produced.
 */
import { lang } from './i18n/index.svelte.js';
import type { Amount } from './locale';
import type { Unit } from './api';

export type { Amount };

/** Formats an exact decimal amount as currency, in the active language. */
export function formatMoney(amount: Amount): string {
  return lang.fmt.money(amount);
}

/** Validates and normalizes cashier-entered text to the API's amount string. */
export function parseAmount(text: string): Amount | null {
  return lang.fmt.parseAmount(text);
}

/** A whole, positive count of base units — or `null`. */
export function parseQuantity(text: string): number | null {
  const normalized = text.trim();
  // No decimal point: a gram product's quantity is 1 or 2 grams, never 1.5.
  // Every language writes a quantity as digits, so this does not depend on the
  // active locale -- and it must not: a stray thousands separator here would be
  // a quantity nobody typed.
  if (!/^\d+$/.test(normalized)) return null;
  const value = Number(normalized);
  if (!Number.isSafeInteger(value) || value < 1) return null;
  return value;
}

/**
 * Reads a catalog form's initial-stock field into what the API expects.
 *
 * Deliberately NOT `parseQuantity`, and the reason is the same one
 * `parseReturnQuantity` carries on the other side of the shop: that function
 * answers `null` for `0` because a movement of zero units is not a movement.
 * An initial stock of zero is not a movement either — it is the shelf before
 * anything arrives, which is the ordinary state of a product nobody has bought
 * yet. Wiring the sale parser here marked a typed `0` invalid, disabled the
 * create button, and explained the refusal with a message about the name and the
 * price. Nothing was sent, so the screen looked disconnected from the API.
 *
 * So: blank and `0` mean zero; a positive whole number means itself; anything
 * else is `null` and the caller refuses the create. Digits only, like every other
 * count in this interface — a quantity has no fractional part and no sign.
 */
export function parseInitialStock(text: string): number | null {
  const normalized = text.trim();
  if (normalized === '') return 0;
  if (!/^\d+$/.test(normalized)) return null;
  const value = Number(normalized);
  if (!Number.isSafeInteger(value)) return null;
  return value;
}

/** A quantity with the backend's unit, rendered in the active language. */
export function formatQuantity(quantity: number, unit: Unit): string {
  return lang.fmt.quantityWithUnit(quantity, unit);
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

/**
 * A calendar day for the ledger's scope chip, in the active language.
 *
 * `zone` is the shop's IANA timezone. Pass it whenever it is known: the chip
 * names the shop's day, and a browser on UTC would otherwise label it a day late
 * (S19).
 */
export function formatDay(date: Date, zone?: string): string {
  return lang.fmt.day(date, zone);
}

/**
 * A wall-clock time for a document's stamp, in the active language.
 *
 * `zone` is the shop's IANA timezone, as `GET /api/day` reported it. Pass it:
 * the ledger's day is the shop's day, so its hours must be the shop's hours
 * (S19).
 */
export function formatTime(date: Date, zone?: string): string {
  return lang.fmt.time(date, zone);
}
