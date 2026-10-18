/**
 * What is left to return on a purchase, and which return a form would send (S6).
 *
 * In the sales ledger this arithmetic lives inside its component, where no unit
 * test can reach it — and the credit note measured what that costs: `[{line, 6},
 * `{line, 6}]` against a line of 10 returned 12, because each entry was checked
 * on its own. The server now guards that, but a screen should not propose a plan
 * whose refusal is already known.
 *
 * Every function here is pure and takes the server's own arrays, so the component
 * derives what it displays from the record rather than from a local tally that
 * would drift the moment a return lands in another tab.
 */
import type { PurchaseItem, SupplierReturnWithItems } from './api.js';

/**
 * How much of each purchase line has already gone back, keyed by purchase-line
 * id.
 *
 * Sums across documents: partial returns are the normal case, so what went back
 * is never the last return but the total of all of them.
 */
export function returnedByLine(returns: readonly SupplierReturnWithItems[]): Map<string, number> {
  const returned = new Map<string, number>();
  for (const supplierReturn of returns) {
    for (const item of supplierReturn.items) {
      returned.set(
        item.purchase_item_id,
        (returned.get(item.purchase_item_id) ?? 0) + item.quantity
      );
    }
  }
  return returned;
}

/**
 * What a line still has to return. Zero means it is done.
 *
 * Clamped rather than trusted: the server prevents over-returning, so a negative
 * value here would mean the record and the screen disagree, and a negative
 * "left" would render as a quantity the operator could then send.
 */
export function returnableOf(
  itemId: string,
  bought: number,
  returned: ReadonlyMap<string, number>
): number {
  return Math.max(0, bought - (returned.get(itemId) ?? 0));
}

/**
 * Whether the purchase is fully reversed.
 *
 * A partial return is not a reversal: the purchase still stands, and the screen
 * must keep offering the action. As with a sale's annulment, this is derived from
 * the returns and never written onto the purchase.
 */
export function isFullyReturned(
  items: readonly PurchaseItem[],
  returns: readonly SupplierReturnWithItems[]
): boolean {
  if (items.length === 0) return false;
  const returned = returnedByLine(returns);
  return items.every((item) => returnableOf(item.id, item.quantity, returned) === 0);
}

/**
 * Reads one line's return field into what `planReturn` expects.
 *
 * Deliberately NOT `parseQuantity`, which is right for a quantity you sell or
 * buy and wrong here: it answers `null` for `0` and for an empty field, because a
 * movement of zero units is not a movement. But on this form zero is meaningful —
 * it is how the operator says "nothing from this line" while returning another.
 * Wiring `parseQuantity` here made a multi-line purchase impossible to return in
 * part: one emptied field turned the whole plan to `null` and no request left.
 *
 * So: blank and `0` mean zero; a positive whole number means itself; anything
 * else (a fraction, a negative, a stray letter) is `null` and the caller shows
 * the error. The server is still the authority on how much is left — this only
 * refuses what it already knows is not a number.
 */
export function parseReturnQuantity(text: string): number | null {
  const normalized = text.trim();
  if (normalized === '') return 0;
  // Digits only, like the shop's quantity fields: no decimal point, no sign, no
  // thousands separator. Every language writes a count as digits.
  if (!/^\d+$/.test(normalized)) return null;
  const value = Number(normalized);
  if (!Number.isSafeInteger(value)) return null;
  return value;
}

/**
 * The lines a return would send, or `null` when the form is not valid.
 *
 * A line already fully returned is skipped rather than rejected: the form
 * prefills every line, and a fully returned one prefills to zero, which means
 * "nothing from here" rather than a mistake to report. A zero for a line that
 * still has room is read the same way. Anything that is not a positive integer,
 * or more than the line has left, is `null` — the caller shows an error.
 *
 * A line absent from `requested` is skipped too. The conservative direction
 * matters: a missing entry can only ever under-return, never over-return.
 */
export function planReturn(
  items: readonly PurchaseItem[],
  returned: ReadonlyMap<string, number>,
  requested: ReadonlyMap<string, number | null>
): { purchase_item_id: string; quantity: number }[] | null {
  const plan: { purchase_item_id: string; quantity: number }[] = [];
  for (const item of items) {
    const remaining = returnableOf(item.id, item.quantity, returned);
    if (remaining === 0) continue;
    if (!requested.has(item.id)) continue;

    const quantity = requested.get(item.id);
    if (quantity === null || quantity === undefined) return null;
    if (quantity === 0) continue;
    if (!Number.isInteger(quantity) || quantity < 0) return null;
    if (quantity > remaining) return null;

    plan.push({ purchase_item_id: item.id, quantity });
  }
  return plan.length > 0 ? plan : null;
}
