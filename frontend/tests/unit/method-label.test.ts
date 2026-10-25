import { describe, expect, it } from 'vitest';
import { methodLabel } from '#lib/methodLabel.js';

const es = { onAccount: 'A cuenta', unknown: '—' };
const en = { onAccount: 'On account', unknown: '—' };

/**
 * A document that moved no money says so, instead of rendering an empty cell.
 *
 * Before this, both ledgers interpolated `payment_method_name` straight into the
 * table. Since v4 a sale or a purchase **may have no method**: it was recorded on
 * account, the debt lives in `obligations`/`payables`, and the server answers
 * `null`. Interpolated, that is an empty `<td>` — indistinguishable from a broken
 * join or a missing row, which is the class of defect a ledger exists to rule out.
 *
 * The rule is one function with two outputs, so both ledgers and any future one
 * agree about what "no rail" looks like. The tests below pin the two shipped
 * languages and the case that matters: `null` must never fall back to something
 * that reads like a rail.
 */

describe('methodLabel names the missing rail instead of leaving a hole', () => {
  it('returns the joined name when the document named a method', () => {
    expect(methodLabel('cash', es)).toBe('cash');
    expect(methodLabel('transferencia', en)).toBe('transferencia');
  });

  it('says the money did not move when the method is null', () => {
    // The whole point: a null is a **claim** (`v4-payables.md` S2), not a gap. The
    // label says which claim it is, and it is not a rail's name.
    expect(methodLabel(null, es)).toBe('A cuenta');
    expect(methodLabel(null, en)).toBe('On account');
  });

  it('never renders empty or the string "null"', () => {
    for (const lang of [es, en]) {
      for (const value of [null, undefined, '']) {
        const label = methodLabel(value, lang);
        expect(label).not.toBe('');
        expect(label).not.toBe('null');
        expect(label).not.toBe('undefined');
      }
    }
  });

  it('does not invent an on-account label for a blank name', () => {
    // A blank *name* is a different defect — a method row with an empty string —
    // and conflating it with "no method" would hide it. It falls back to the
    // unknown label, which reads as a problem rather than as a normal state.
    expect(methodLabel('', es)).toBe('—');
    expect(methodLabel('   ', es)).toBe('—');
  });
});
