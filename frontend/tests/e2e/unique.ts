/**
 * Finding a spec's own document in a ledger every spec writes to.
 *
 * The ledgers are append-only and shared: `sale`, `purchase` and `supplier-return`
 * all record documents in the same development database, and CI runs the whole
 * suite against one of them. That makes position a lie -- `locator(...).first()`
 * is "the newest document", which is a spec's own only when nothing recorded one
 * since. It passed locally for months and failed on CI, where the order differs
 * (#52).
 *
 * Position cannot be replaced by the product name either: a ledger row renders the
 * time, the id, the party, the item count, the status and the **total**. The
 * product name lives in the lines behind it, which only exist once the row is
 * opened. So the total is the one field a test can select its own document by, and
 * it has to be unique against everything earlier runs left behind.
 *
 * `uniqueUnitPrice` derives a price from the clock for that reason. A fixed total
 * collides with every earlier run of the same spec -- measured while fixing #52:
 * `toHaveCount(1)` on a hard-coded `$ 77,48` resolved to three rows.
 */

/**
 * A price unlikely to repeat across a day of test runs, in the shop's own shape.
 *
 * Returns e.g. `"347,21"` -- comma decimal, because that is what the price field
 * parses and a period would be refused. The range is wide on purpose: the ledger
 * keeps every document ever recorded, so a narrow range would collide within a
 * single afternoon of runs.
 */
export function uniqueUnitPrice(seed: number = Date.now()): string {
  const cents = (seed % 89000) + 1000; // 10.00 .. 899.99
  return (cents / 100).toFixed(2).replace('.', ',');
}

/**
 * The money the shop renders for a price string, e.g. `"4,37"` -> `"$ 4,37"`.
 *
 * Above a thousand the shop groups thousands with a period (`$ 2.596,16`), which
 * is es-AR formatting and not what `Number` would print. A test asserting on a
 * computed total has to go through this, or it fails on the runs whose price
 * happens to be large enough -- which is what happened the first time this helper
 * had no grouping.
 */
export function money(price: string): string {
  const [whole, decimals] = price.split(',');
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, '.');
  return `$ ${grouped}${decimals === undefined ? '' : `,${decimals}`}`;
}

/** A line total for `quantity` units at `price`, in the shop's own shape. */
export function lineTotal(price: string, quantity: number): string {
  const cents = Math.round(Number(price.replace(',', '.')) * 100) * quantity;
  return (cents / 100).toFixed(2).replace('.', ',');
}
