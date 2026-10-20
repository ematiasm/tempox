/**
 * What a draft form holds, and what of it is money yet (the line rule both
 * draft screens share).
 *
 * This lives here, not inside `SaleDraft.svelte` or `PurchaseDraft.svelte`, for
 * the same reason `salesRefund.ts` and `supplierReturns.ts` do: inside a component
 * no unit test can reach it. Before this module the rule "a line counts only when
 * both fields parse" was a `$derived` in `SaleDraft`, and "the form is ready only
 * when every line parses" was a different `$derived` in `PurchaseDraft` — two
 * copies of one rule, neither reachable by a test, and neither covering the case
 * of a field holding text that does not parse.
 *
 * Every function is pure and takes plain strings, so the components keep owning
 * the reactive state and only ask this module what it means. Nothing here knows
 * that Svelte exists.
 */
import type { Amount } from './locale.js';
import { lineTotalAmount, parseAmount, parseQuantity, sumAmounts } from './money.js';

/**
 * One line as the form holds it: the two fields as typed text.
 *
 * The text is the source of truth for each field's own validity. The parsed
 * values only exist once they parse, which is why this type carries strings and
 * never numbers.
 */
export type DraftLine = {
  readonly qtyText: string;
  readonly priceText: string;
};

/** One line once both of its fields parse. Only these are money. */
export type ParsedLine = {
  readonly quantity: number;
  /** The wire form (`10.00`), not the text the cashier typed. */
  readonly unitPrice: Amount;
};

/**
 * A line's parse result: `null` until both fields are readable.
 *
 * This is the one place a line's validity is decided; every other function here
 * asks it rather than re-deriving the rule, so there is a single answer to "is
 * this line money yet".
 *
 * A half-typed line is not money: it is either an empty field the cashier has not
 * reached yet, or text that will never parse. Neither is a line to record, and
 * neither may contribute to the preview total.
 */
export function parseDraftLine(line: DraftLine): ParsedLine | null {
  const quantity = parseQuantity(line.qtyText);
  const unitPrice = parseAmount(line.priceText);
  if (quantity === null || unitPrice === null) return null;
  return { quantity, unitPrice };
}

/**
 * The lines that count, in the order the form holds them.
 *
 * `null`s are dropped rather than failing the whole form, which is what the sales
 * sheet needs: a cashier mid-typing a second line must still see the total of the
 * first, and Ctrl+Enter still records what is complete.
 */
export function completeLines<T extends DraftLine>(lines: readonly T[]): (T & ParsedLine)[] {
  const complete: (T & ParsedLine)[] = [];
  for (const line of lines) {
    const parsed = parseDraftLine(line);
    if (parsed !== null) complete.push({ ...line, ...parsed });
  }
  return complete;
}

/**
 * Whether a line holds text that does not parse.
 *
 * Deliberately **not** `parseDraftLine(line) === null`: an empty field is not an
 * error, it is a field the cashier has not reached. Only text that is present and
 * unreadable is a mistake worth telling them about. Without this distinction the
 * form would claim "something you typed is invalid" the instant a new empty line
 * appeared, and the warning an operator learns to ignore is worse than none.
 */
export function isInvalidLine(line: DraftLine): boolean {
  return (
    (parseQuantity(line.qtyText) === null && line.qtyText.trim() !== '') ||
    (parseAmount(line.priceText) === null && line.priceText.trim() !== '')
  );
}

/** Whether any line in the form is invalid. */
export function hasInvalidLine(lines: readonly DraftLine[]): boolean {
  return lines.some(isInvalidLine);
}

/**
 * The exact sum of a set of complete lines, as an `Amount`.
 *
 * Deliberately delegates the cent arithmetic to `sumAmounts` instead of walking
 * `BigInt` by hand: that helper already sums whole cents exactly, already refuses
 * a malformed amount, and is already covered by its own cases. Two copies of the
 * same addition is what this module exists to stop.
 *
 * The one thing this adds on top is the line totals themselves: it asks
 * `lineTotalAmount` for each, and **refuses** rather than skips if one comes back
 * `null`. `lineTotalAmount` returns `null` for a non-positive quantity or a
 * malformed price, neither of which `parseDraftLine` can produce -- so reaching
 * that branch means the two parsers have started to disagree. Skipping would
 * quietly under-report the preview total, and a wrong total that looks plausible
 * is worse than a dash. `null` propagates so the caller shows one.
 *
 * Returns `'0.00'` for no lines, because an empty draft previews as zero and not
 * as a missing figure.
 */
export function draftTotal(complete: readonly ParsedLine[]): Amount | null {
  const totals: Amount[] = [];
  for (const line of complete) {
    const lineTotal = lineTotalAmount(line.quantity, line.unitPrice);
    if (lineTotal === null) return null;
    totals.push(lineTotal);
  }
  return sumAmounts(totals);
}

/**
 * Whether a draft holds something recordable.
 *
 * **At least one** complete line, not all of them: that is the sales sheet's
 * rule, where a half-typed extra line must not block recording the completed
 * ones. The purchase sheet requires every line to parse and guards with
 * `everyLineComplete` instead, because a purchase is a document from a supplier
 * and recording a partial one would be wrong.
 */
export function canRecord(lines: readonly DraftLine[], submitting: boolean): boolean {
  return completeLines(lines).length > 0 && !submitting;
}

/** Whether every line parses, and there is at least one. The purchase sheet's rule. */
export function everyLineComplete(lines: readonly DraftLine[]): boolean {
  return lines.length > 0 && lines.every((line) => parseDraftLine(line) !== null);
}

/** How many whole units the complete lines amount to, for the draft's summary. */
export function unitCountOf(complete: readonly ParsedLine[]): number {
  return complete.reduce((sum, line) => sum + line.quantity, 0);
}

/** How many complete lines are bonus lines, i.e. priced at zero. */
export function bonusCountOf(complete: readonly ParsedLine[]): number {
  return complete.filter((line) => line.unitPrice === '0.00').length;
}
