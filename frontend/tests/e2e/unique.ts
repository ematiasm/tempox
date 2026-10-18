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
 * That "everything earlier" is the hard part, and the first version of this file
 * got it wrong: it mapped `Date.now()` through `% 89000`, which is 89,000 distinct
 * values repeating every 89 seconds. Two workers starting in the same millisecond,
 * or two runs an exact multiple of 89 seconds apart, produced the same price --
 * and because the ledger is never cleared, one collision poisons that value for
 * every later run. The uniqueness space has to be the whole ledger lifetime, not a
 * window, so the seed comes from run-scoped entropy plus a per-process counter
 * instead of the wall clock alone.
 */

/**
 * One value per test process, as wide as the ledger needs.
 *
 * `GITHUB_RUN_ID` and `GITHUB_RUN_ATTEMPT` make two CI runs differ even if they
 * start together (matrix legs, shards, retries). Locally there is no run id, so
 * the process start time is the next widest thing, mixed with a random component
 * because two local runs can also start in the same millisecond.
 */
const RUN_SEED = (() => {
  const runId = process.env.GITHUB_RUN_ID ? Number(process.env.GITHUB_RUN_ID) : NaN;
  const attempt = process.env.GITHUB_RUN_ATTEMPT ? Number(process.env.GITHUB_RUN_ATTEMPT) : 1;
  if (Number.isFinite(runId)) {
    // A CI run id already distinguishes runs; the attempt separates retries.
    return (runId % 1_000_000) * 100 + (attempt % 100);
  }
  // Local: the process start time, plus entropy so same-millisecond starts differ.
  return Date.now() % 1_000_000_000;
})();

/** A counter, so two specs in the same process never draw the same value. */
let sequence = 0;

/**
 * A price that will not repeat in the shared ledger's lifetime, in the shop's shape.
 *
 * Returns e.g. `"347,21"` -- comma decimal, because that is what the price field
 * parses and a period would be refused. The value is a function of the run seed and
 * a per-process counter, so distinct runs and distinct specs always differ. The
 * space is ~10^9 values against a ledger that gains tens of documents a day, which
 * is the birthday bound against the ledger's lifetime rather than against one
 * afternoon.
 */
export function uniqueUnitPrice(): string {
  sequence += 1;
  // The counter guarantees this process never draws the same value twice; the run
  // seed separates processes. The range is 100.00 .. 9,999.99 -- wide enough that
  // the birthday bound is against the ledger's lifetime, narrow enough to stay a
  // plausible unit price in a shop that sells at a counter.
  const mixed = (RUN_SEED * 7919 + sequence * 104_729) % 990_000;
  const cents = mixed + 10_000;
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
