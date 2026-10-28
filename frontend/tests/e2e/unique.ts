import type { Locator, Page } from '@playwright/test';

/**
 * Finding a spec's own document in a ledger every spec writes to (#52).
 *
 * The ledgers are append-only and shared: `sale`, `purchase` and `supplier-return`
 * all record documents in the same development database, and CI runs the whole
 * suite against one of them. That makes position a lie -- `locator(...).first()` is
 * "the newest document", which is a spec's own only when nothing recorded one
 * since. It passed locally for months and failed on CI, where the order differs.
 *
 * Two attempts to replace position failed for the same reason: they selected by a
 * *derived* value instead of an identity.
 *
 * 1. **The product name.** A ledger row renders the time, the id, the party, the
 *    item count, the status and the total. The product name lives in the lines
 *    behind it, which only exist once the row is opened -- it was never on the row.
 *
 * 2. **The total.** Even with a run-unique price, a total is not unique: 100.00 x 7
 *    and 175.00 x 4 both come to 700.00. The price could not be made unique across
 *    worker processes either, because `GITHUB_RUN_ID` is identical for all of them
 *    and a per-process counter resets in each.
 *
 * The id is the identity. Both draft screens now carry `data-doc-id` on the banner
 * they render after recording, so a spec reads its own document's id and then
 * finds exactly that row -- no arithmetic, no uniqueness budget, no dependence on
 * what any other run left behind.
 */

/**
 * The id of the document a draft screen just recorded.
 *
 * Throws rather than returning `null`: a spec that cannot learn its own document's
 * id must stop, not go on to assert against whichever row happens to be first.
 */
export async function recordedDocumentId(page: Page): Promise<string> {
  const id = await page.locator('[data-doc-id]').first().getAttribute('data-doc-id');
  if (!id) {
    throw new Error(
      'No recorded document id on the draft screen. It reports one after recording, ' +
        'so its absence means the record did not land, or the banner stopped exposing it.'
    );
  }
  return id;
}

/**
 * The money the shop renders for an amount string, e.g. `"4,37"` -> `"$ 4,37"`.
 *
 * Above a thousand the shop groups thousands with a period (`$ 2.596,16`), which is
 * es-AR formatting and not what `Number` would print. Assertions that compare
 * against a computed amount have to go through this, or they fail on the runs whose
 * value happens to be large enough.
 */
export function money(amount: string): string {
  const [whole, decimals] = amount.split(',');
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, '.');
  return `$ ${grouped}${decimals === undefined ? '' : `,${decimals}`}`;
}

/** A line total for `quantity` units at `price`, in the shop's own shape. */
export function lineTotal(price: string, quantity: number): string {
  const cents = Math.round(Number(price.replace(',', '.')) * 100) * quantity;
  return (cents / 100).toFixed(2).replace('.', ',');
}

/**
 * One line in the sale draft, wherever the draft decides to put it.
 *
 * The draft stopped being a table when the receipt-pad direction landed: a line
 * is now a printed row on the slip, not a `<tr>`. Twenty-three assertions across
 * `sale.spec.ts` and `method.spec.ts` pointed at `table.lines tbody tr`, so the
 * markup change would have rewritten all of them in place and the next one would
 * have done it again.
 *
 * The contract these selectors encode is deliberately narrow, because it is the
 * only thing a test should be allowed to know about the draft's markup:
 *
 * - A line is an element carrying `data-draft-line`.
 * - Inside it, the quantity and price fields are still found by their accessible
 *   names. Those are the labels the cashier reads, so a rename is a user-visible
 *   change and *should* break a test, not get quietly absorbed here.
 *
 * The quantity field's name starts with `Cant` because `sale.spec.ts` matches it
 * by prefix (`input[aria-label^="Cant"]`), which is also how it distinguishes the
 * quantity from the price inside a row that holds both.
 */
export function saleDraftLine(page: Page, hasText?: string): Locator {
  const all = page.locator('[data-draft-line]');
  return hasText === undefined ? all.first() : all.filter({ hasText });
}

/** Every line currently printed on the draft slip. */
export function saleDraftLines(page: Page): Locator {
  return page.locator('[data-draft-line]');
}

/**
 * One recorded document in the day's book, by its id.
 *
 * Found by id and never by position or total (#52): every spec writes to the same
 * append-only ledger, so `first()` means "the newest document in the whole
 * suite", not "mine". `recordedDocumentId` is how a spec learns its own id.
 *
 * The id travels in `data-doc-row`, which replaced the `aria-controls` attribute
 * the table used. `aria-controls` was doing two jobs — naming the expandable
 * region and incidentally being the only place the full id appeared — and the
 * second job is now explicit instead of a side effect of an accessibility
 * attribute.
 */
export function daySlip(page: Page, id: string): Locator {
  return page.locator(`[data-doc-row="${id}"]`);
}

/**
 * The lines panel a document opens, by the document's id.
 *
 * Still scoped by document, because two open documents must not be confused for
 * one another: the panel holds the refund form and every assertion about it is
 * about *that* sale.
 */
export function daySlipDetail(page: Page, id: string): Locator {
  return page.locator(`[data-doc-detail="${id}"]`);
}
