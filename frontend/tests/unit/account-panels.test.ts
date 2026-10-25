import { describe, expect, it } from 'vitest';
import { accountBoxes, type AccountLike } from '#lib/accountPanels.js';

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
