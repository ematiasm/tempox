import { describe, expect, it } from 'vitest';
import { accountBoxes, type AccountLike } from '#lib/accountPanels.js';
import { allocationPreview } from '#lib/collectionForm.js';

/**
 * The three boxes of the side panel, and the two rules `DESIGN.md` puts at risk.
 *
 * `v5-customer-account.md` S3. The order is not cosmetic: identity answers "did I
 * click the right row?" before any number, the balance answers "how much", and the
 * document list is the detail behind that number.
 *
 * The tests below pin the two claims that would be silently wrong if a component
 * computed them inline: **the two figures are never netted**, and a client with
 * nothing owed is still a valid account rather than an empty one.
 */

const client = (over: Partial<AccountLike> = {}): AccountLike => ({
  id: 'c-1',
  name: 'Almacén Sur',
  owed: '1000.00',
  inFavour: '500.00',
  unpaid: [{ id: 'o-1', sourceDocumentId: 's-1', total: '1000.00', outstanding: '1000.00' }],
  ...over
});

describe('the account panel boxes', () => {
  it('reports both figures separately, never netted', () => {
    const boxes = accountBoxes(client());
    // 1000 owed and 500 in their favour is two facts. A single figure of 500 would
    // be true of neither: it would hide both the debt and the money we are holding.
    expect(boxes.owed).toBe('1000.00');
    expect(boxes.inFavour).toBe('500.00');
    expect(boxes.owed).not.toBe('500.00');
  });

  it('keeps a client who owes nothing as a real account, not an empty one', () => {
    // The panel must not read "0.00 owed" as "this client is not there" — the
    // not-found state is `null` from the API and is rendered by the caller.
    const boxes = accountBoxes(client({ owed: '0.00', inFavour: '0.00', unpaid: [] }));
    expect(boxes.owed).toBe('0.00');
    expect(boxes.unpaidCount).toBe(0);
    expect(boxes.name).toBe('Almacén Sur');
  });

  it('counts the unpaid documents it was given and nothing else', () => {
    const boxes = accountBoxes(
      client({
        unpaid: [
          { id: 'o-1', sourceDocumentId: 's-1', total: '10.00', outstanding: '10.00' },
          { id: 'o-2', sourceDocumentId: 's-2', total: '20.00', outstanding: '5.00' }
        ]
      })
    );
    expect(boxes.unpaidCount).toBe(2);
  });

  it('says so when a figure would be hidden behind the other one', () => {
    // The one presentation rule worth encoding: when both figures are non-zero the
    // panel must show both, and this flag is what a component reads to decide to
    // render the second box's "in their favour" line at all.
    expect(accountBoxes(client()).showsBothFigures).toBe(true);
    expect(accountBoxes(client({ inFavour: '0.00' })).showsBothFigures).toBe(false);
    expect(accountBoxes(client({ owed: '0.00' })).showsBothFigures).toBe(false);
  });
});

describe('the collection form never states what the server decides', () => {
  it('exposes no field that names which documents the remainder closes', () => {
    // **`S9` as a property of the module, not a promise in a comment.** The temptation in a
    // form that holds every figure is to predict the allocation so the cashier sees the plan
    // before submitting. That prediction is a second implementation of `S3`, and this
    // repository paid for that class three times in one slice (`V7b` of
    // `v5-customer-account.md`). The preview's shape is the assertion: it carries the
    // remainder as a figure and refuses to carry a destination.
    const preview = allocationPreview({
      methodId: 'cash',
      total: '250.00',
      lines: [{ debtId: 'o-1', outstanding: '100.00', amount: '40.00' }]
    });
    // `lineUnreadable` is the opposite kind of field and belongs here: it reports what the
    // form could **not** read, so the gate can refuse by name (`R3-001` of review
    // `review-b6bebae2ab91c89f`). It names no destination — it names the absence of a figure.
    expect(Object.keys(preview).sort()).toEqual([
      'allocatedLines',
      'lineOverDebt',
      'lineUnreadable',
      'manualTotal',
      'overAllocated',
      'remainder',
      'total'
    ]);
  });
});
