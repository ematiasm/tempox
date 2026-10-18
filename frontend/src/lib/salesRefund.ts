/**
 * What a sale can still give back, and which refund a form would send (S1-S4).
 *
 * This arithmetic lives here, not inside `Ledger.svelte`, because inside the
 * component no unit test could reach it — and that cost was measured rather than
 * imagined. A multi-line sale could not be refunded one line at a time:
 * `parseQuantity` answers `null` for `0`, so the only way to say "nothing from
 * this line" was refused as an invalid quantity, while the field prefilled with
 * the full remaining amount. Leaving it alone submitted it, so confirming a
 * refund returned stock and money on lines the cashier never touched.
 *
 * The purchase side already learned this: `supplierReturns.ts` exists for the same
 * reason, and its header names the sales ledger as the place the arithmetic was
 * unreachable. This module is that extraction, plus the line-selection rule the
 * form needs so it never proposes a plan it must not send.
 *
 * Every function is pure and takes the server's own arrays, so the component
 * derives what it displays from the record rather than from a local tally that
 * would drift the moment a refund lands in another tab.
 */
import type { CreditNote, SaleItem } from './api.js';

/**
 * How much of each sale line has already come back, keyed by sale-line id.
 *
 * Sums across documents: partial refunds are the normal case, so what went back
 * is never the last credit note but the total of all of them.
 */
export function refundedByLine(notes: readonly CreditNote[]): Map<string, number> {
  const refunded = new Map<string, number>();
  for (const note of notes) {
    for (const item of note.items) {
      refunded.set(item.sale_item_id, (refunded.get(item.sale_item_id) ?? 0) + item.quantity);
    }
  }
  return refunded;
}

/**
 * What a line still has to give back. Zero means it is done.
 *
 * Clamped rather than trusted: the server prevents over-refunding, so a negative
 * value here would mean the record and the screen disagree, and a negative "left"
 * would render as a quantity the cashier could then send.
 */
export function refundableOf(
  itemId: string,
  sold: number,
  refunded: ReadonlyMap<string, number>
): number {
  return Math.max(0, sold - (refunded.get(itemId) ?? 0));
}

/**
 * Whether the sale is fully reversed.
 *
 * A partial refund is not an annulment: the document still stands, wearing its
 * confirmed stamp, with a note underneath it. As with a purchase's return, this is
 * derived from the notes and never written onto the sale.
 */
export function isFullyRefunded(items: readonly SaleItem[], notes: readonly CreditNote[]): boolean {
  if (items.length === 0) return false;
  const refunded = refundedByLine(notes);
  return items.every((item) => refundableOf(item.id, item.quantity, refunded) === 0);
}

/**
 * Reads one line's refund field into what `planRefund` expects.
 *
 * Deliberately NOT `parseQuantity`, which is right for a quantity you sell and
 * wrong here: it answers `null` for `0` and for an empty field, because a movement
 * of zero units is not a movement. But on this form zero is meaningful — it is how
 * the cashier says "nothing from this line" while returning another. Wiring
 * `parseQuantity` here made a multi-line sale impossible to refund in part: one
 * emptied field turned the whole plan to `null` and no request left.
 *
 * So: blank and `0` mean zero; a positive whole number means itself; anything else
 * (a fraction, a negative, a stray letter) is `null` and the caller shows the
 * error. The server is still the authority on how much is left — this only refuses
 * what it already knows is not a number.
 */
export function parseRefundQuantity(text: string): number | null {
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
 * The lines a refund would send, or `null` when the form is not valid.
 *
 * `selected` is the cashier's choice per line, made with the form's checkbox, and
 * it is the rule the old planner was missing: an unchecked line is **excluded**,
 * never sent at its prefilled amount. A line absent from `selected` counts as
 * unchecked — the conservative direction, because a missing entry can only ever
 * under-refund and an assumed "yes" would refund something nobody chose.
 *
 * A line already fully refunded is skipped rather than rejected: the form prefills
 * every line, and a fully refunded one has nothing left to offer. A selected line
 * whose quantity reads as zero is skipped too, so unchecking a line and leaving
 * its field at zero mean the same thing.
 *
 * Anything that is not a whole count, or more than the line has left, is `null` —
 * the caller shows the error. An empty plan is `null` as well: a refund of nothing
 * is not a document.
 */
export function planRefund(
  items: readonly SaleItem[],
  refunded: ReadonlyMap<string, number>,
  selected: ReadonlyMap<string, boolean>,
  requested: ReadonlyMap<string, number | null>
): { sale_item_id: string; quantity: number }[] | null {
  const plan: { sale_item_id: string; quantity: number }[] = [];
  for (const item of items) {
    const remaining = refundableOf(item.id, item.quantity, refunded);
    if (remaining === 0) continue;
    if (selected.get(item.id) !== true) continue;

    const quantity = requested.get(item.id);
    if (quantity === null || quantity === undefined) return null;
    if (quantity === 0) continue;
    if (!Number.isInteger(quantity) || quantity < 0) return null;
    if (quantity > remaining) return null;

    plan.push({ sale_item_id: item.id, quantity });
  }
  return plan.length > 0 ? plan : null;
}
