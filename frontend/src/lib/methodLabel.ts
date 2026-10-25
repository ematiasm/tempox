/**
 * What a ledger cell says about the rail a document moved money through.
 *
 * **The problem this exists for**: since v4 a sale or a purchase may have **no**
 * method. It was recorded on account, the debt lives in `obligations`/`payables`,
 * and the server answers `payment_method_name: null`. Interpolated into a table
 * cell that is an empty `<td>`, which reads as a broken join or a missing row —
 * exactly the kind of "is it empty or is it broken?" a ledger must not create.
 *
 * **One function, both ledgers.** `Ledger.svelte` and `PurchaseLedger.svelte`
 * render the same column from the same shape, so the rule for what "no rail" looks
 * like lives here instead of being written twice and drifting.
 *
 * The three cases are genuinely different and must not collapse into one:
 *
 * | input | what it is | what it says |
 * |---|---|---|
 * | a name | money moved through that rail | the rail's name |
 * | `null` / `undefined` | **a claim**: this document moved no money | the on-account label |
 * | `''` / blanks | a method whose name is empty — a defect | the unknown label |
 *
 * The last row is the one worth defending: a blank *name* is not the same fact as
 * no method at all, and quietly rendering it as "on account" would hide a broken
 * `payment_methods` row behind a normal-looking state.
 *
 * It takes the two strings rather than the whole `lang` object so it can be called
 * from a `.svelte` component (where `lang.t` is reactive state) and from a plain
 * test with equal ease — the caller decides where the words come from.
 */
/**
 * Whether a document's method cell is the on-account claim rather than a rail's name.
 *
 * **Exported separately because the class and the label must agree**, and computing
 * them from two different expressions is how they stop agreeing: the components used
 * `name === null` for the class and `methodLabel(...)` for the text, which disagree
 * for `undefined` (the label says "on account", `=== null` says it is not) and for a
 * blank name (the label says "—", `=== null` says "on account"). Recorded by review
 * `R3-003` of lineage `review-fe994c43a8bc4a9e`.
 */
export function isOnAccount(name: string | null | undefined): name is null | undefined {
  return name === null || name === undefined;
}

export function methodLabel(
  name: string | null | undefined,
  strings: { onAccount: string; unknown: string }
): string {
  // `null` and `undefined` are the claim. Checked before the blank case on purpose:
  // an empty string is a *present* value that happens to be empty, and treating it
  // as "no method" would erase the distinction the schema makes.
  if (isOnAccount(name)) {
    return strings.onAccount;
  }
  if (name.trim() === '') {
    return strings.unknown;
  }
  return name;
}
