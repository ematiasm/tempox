/**
 * The three boxes of a party's account panel, derived in one place.
 *
 * `v5-customer-account.md` S3. The panel shows who the party is, the two balance
 * figures, and their unpaid documents — and this module is the shape a component
 * reads instead of reaching into the API types, which differ per side (`owed` for a
 * customer, `owed_to_them` for a supplier) only because a shared field name would let
 * a screen label "what we owe" as "what they owe".
 *
 * **Two rules live here because a component computing them inline would get them
 * wrong**, and both are ones this feature already paid for:
 *
 * 1. **The two figures are never netted.** They can both be non-zero: a client owing
 *    1000 on one sale may be holding 500 from an overpayment. `showsBothFigures` is
 *    what the component reads to decide whether to render the second line at all — a
 *    component comparing against `'0.00'` by hand is how that comparison drifts.
 * 2. **A party who owes nothing is a real account.** `owed: '0.00'` is a fact, not an
 *    absence; the not-found state comes from the API as `null` and is the caller's to
 *    render. Collapsing the two makes a settled client look deleted.
 */
export type AccountLike = {
  id: string;
  name: string;
  owed: string;
  inFavour: string;
  /**
   * The unpaid documents. **`id` is the obligation and `sourceDocumentId` is the sale or
   * purchase** — different rows, and only the second is a document a cashier can look up,
   * which is why the panel renders that one.
   */
  unpaid: {
    id: string;
    sourceDocumentId: string;
    total: string;
    outstanding: string;
  }[];
};

export type AccountBoxes = {
  id: string;
  name: string;
  owed: string;
  inFavour: string;
  unpaidCount: number;
  /** True when there is something to say on **both** sides of the balance. */
  showsBothFigures: boolean;
};

/** A figure that carries no money. Compared as a string, because that is what the wire sends. */
function isZero(amount: string): boolean {
  return Number(amount) === 0;
}

export function accountBoxes(account: AccountLike): AccountBoxes {
  return {
    id: account.id,
    name: account.name,
    owed: account.owed,
    inFavour: account.inFavour,
    unpaidCount: account.unpaid.length,
    showsBothFigures: !isZero(account.owed) && !isZero(account.inFavour)
  };
}
