import { describe, expect, it } from 'vitest';

/**
 * The collection form's arithmetic, and the one rule it must not implement.
 *
 * `odd/tasks/v5-payments.md` S9. The form has three inputs — the method, the amount
 * received, and an optional amount per unpaid document — and this module is what turns
 * them into a request. Two things live here because a component doing them inline gets
 * them wrong, and both are rules this feature already paid for:
 *
 * 1. **The plan is the cashier's, and the remainder is a figure, not a prediction.** The
 *    form shows `total − manual` and says where it goes ("the oldest"), and it must never
 *    claim *which* documents the server will close. That rule is the server's (`S3`), and
 *    a second implementation in a client is how the two disagree — three times in the
 *    previous slice (`V7b` of `v5-customer-account.md`).
 * 2. **A partial split is normal, so an empty or partial plan is valid.** Only the
 *    conditions the server would refuse are refused here, and the hint names the field —
 *    a disabled button with no sentence is the pattern this repository's sale draft
 *    already corrected.
 */

import {
  allocationPreview,
  canSubmitCollection,
  collectionRequest,
  type CollectionDraft,
  type CollectionLine
} from '#lib/collectionForm.js';

const line = (over: Partial<CollectionLine> = {}): CollectionLine => ({
  debtId: 'o-1',
  outstanding: '100.00',
  amount: '',
  ...over
});

const draft = (over: Partial<CollectionDraft> = {}): CollectionDraft => ({
  methodId: 'cash',
  total: '',
  lines: [],
  ...over
});

describe('the collection form preview', () => {
  it('reports the remainder the cashier did not allocate, as a figure', () => {
    // 250 received, 40 allocated by hand: 210 is what the rule will spend.
    const preview = allocationPreview(
      draft({
        total: '250,00',
        lines: [line({ amount: '40,00' }), line({ debtId: 'o-2', outstanding: '60.00' })]
      })
    );
    expect(preview.total).toBe('250.00');
    expect(preview.manualTotal).toBe('40.00');
    expect(preview.remainder).toBe('210.00');
    // **Not named.** Which documents the 210 closes is the server's answer, and this
    // module refuses to guess it even though it holds every figure needed to.
    expect(preview).not.toHaveProperty('documents');
    expect(preview).not.toHaveProperty('allocation');
  });

  it("treats an empty plan as valid, because the whole total is then the rule's", () => {
    const preview = allocationPreview(draft({ total: '100,00', lines: [] }));
    expect(preview.manualTotal).toBe('0.00');
    expect(preview.remainder).toBe('100.00');
  });

  it('says the plan exceeds the money rather than silently clamping it', () => {
    // A cashier who allocates 120 of 100 has made a real mistake and must see it: a
    // form that quietly reduced the figure to 100 would send a request that differs
    // from what was typed.
    const preview = allocationPreview(
      draft({
        total: '100,00',
        lines: [line({ amount: '100,00' }), line({ debtId: 'o-2', amount: '20,00' })]
      })
    );
    expect(preview.manualTotal).toBe('120.00');
    expect(preview.remainder).toBe('-20.00');
    expect(preview.overAllocated).toBe(true);
  });

  it('flags a line above what that document owes, without touching the total', () => {
    // The server answers 409 for this; the form's job is to say so before the request.
    const preview = allocationPreview(
      draft({ total: '500,00', lines: [line({ outstanding: '100.00', amount: '150,00' })] })
    );
    expect(preview.lineOverDebt).toEqual(['o-1']);
    expect(preview.remainder).toBe('350.00');
  });

  it('counts a blank line as unallocated, and a typed zero as a refusal', () => {
    // **The difference the form must keep**: a line the cashier has not filled in is not an
    // allocation, while a typed `0` *is* one and the server refuses it (`S4` checks every
    // manual amount, and `0` is not positive). Collapsing the two — the easy implementation,
    // `cents(amount) ?? 0n` — is invisible in the figures and visible here: the blank line
    // must not count as allocated, and the typed zero must stop the form.
    const blank = allocationPreview(
      draft({
        total: '100,00',
        lines: [line({ amount: '' }), line({ debtId: 'o-2', amount: '20,00' })]
      })
    );
    expect(blank.allocatedLines).toBe(1);
    expect(blank.manualTotal).toBe('20.00');

    expect(
      canSubmitCollection(draft({ total: '100,00', lines: [line({ amount: '0,00' })] }))
    ).toEqual({ ok: false, reason: 'lineNotPositive' });
  });

  it('refuses an amount it cannot read instead of dropping it from the request', () => {
    // **`R3-001` / `R4-1` of review `review-b6bebae2ab91c89f`, both CRITICAL.** The first
    // version of this module had a dot-only parser and skipped what it could not read, so a
    // cashier who typed a value it rejected got a valid request with that allocation missing —
    // and the rule spent that money on documents they never named. Silence, not a refusal.
    //
    // Garbage is therefore not "nothing was typed": it stops the submit and names the field.
    const preview = allocationPreview(
      draft({ total: '100,00', lines: [line({ amount: 'cuarenta' })] })
    );
    expect(preview.lineUnreadable).toEqual(['o-1']);
    expect(
      canSubmitCollection(draft({ total: '100,00', lines: [line({ amount: 'cuarenta' })] }))
    ).toEqual({ ok: false, reason: 'lineUnreadable' });
    expect(
      collectionRequest(draft({ total: '100,00', lines: [line({ amount: 'cuarenta' })] }))
    ).toBeNull();
  });

  it('reads an amount the way the shop writes it, not the way the wire does', () => {
    // **The other half of `R3-001`.** The UI is es-AR and renders `$ 1.234,56`; a parser that
    // only accepted `1234.56` rejected the very shape this application shows, which is how the
    // defect above stayed invisible in a green suite. `parseAmount` is the locale's own reader,
    // and this test is what pins the module to it.
    const preview = allocationPreview(
      draft({
        total: '1.234,56',
        lines: [line({ outstanding: '1000.00', amount: '45,00' })]
      })
    );
    expect(preview.manualTotal).toBe('45.00');
    expect(preview.remainder).toBe('1189.56');
    expect(canSubmitCollection(draft({ total: '45,00' }))).toEqual({ ok: true });
  });

  it('still ignores a genuinely blank line, because that is a form being filled', () => {
    const preview = allocationPreview(
      draft({
        total: '100,00',
        lines: [line({ amount: '' }), line({ debtId: 'o-2', amount: '20,00' })]
      })
    );
    expect(preview.manualTotal).toBe('20.00');
    expect(preview.remainder).toBe('80.00');
    expect(preview.lineUnreadable).toEqual([]);
  });
});

describe('the collection form submission gate', () => {
  it('is ready for a total with no plan at all', () => {
    // The whole total goes to the rule: the ordinary one-click collection.
    expect(canSubmitCollection(draft({ total: '100,00' }))).toEqual({ ok: true });
  });

  it('names the field it is waiting for, in order', () => {
    // A disabled submit with no sentence is the pattern the sale draft already
    // corrected; each reason names one field, and the first one wins so the operator
    // is never told about two problems at once.
    // `draft()` already names a method, so the first thing missing is the money.
    expect(canSubmitCollection(draft())).toEqual({ ok: false, reason: 'total' });
    expect(canSubmitCollection(draft({ methodId: '' }))).toEqual({
      ok: false,
      reason: 'method'
    });
    expect(canSubmitCollection(draft({ methodId: '', total: '10,00' }))).toEqual({
      ok: false,
      reason: 'method'
    });
    expect(canSubmitCollection(draft({ methodId: 'cash', total: '' }))).toEqual({
      ok: false,
      reason: 'total'
    });
  });

  it('refuses a total that is not money, and a plan that exceeds it', () => {
    expect(canSubmitCollection(draft({ total: '0.00' }))).toEqual({ ok: false, reason: 'total' });
    expect(canSubmitCollection(draft({ total: 'abc' }))).toEqual({ ok: false, reason: 'total' });
    expect(
      canSubmitCollection(draft({ total: '100,00', lines: [line({ amount: '120,00' })] }))
    ).toEqual({ ok: false, reason: 'overAllocated' });
    expect(
      canSubmitCollection(
        draft({ total: '100,00', lines: [line({ outstanding: '50.00', amount: '60,00' })] })
      )
    ).toEqual({ ok: false, reason: 'lineOverDebt' });
  });

  it('refuses the same document named twice, which the server would refuse too', () => {
    const twice = canSubmitCollection(
      draft({
        total: '100,00',
        lines: [line({ amount: '10,00' }), line({ debtId: 'o-1', amount: '20,00' })]
      })
    );
    expect(twice).toEqual({ ok: false, reason: 'duplicateLine' });
  });
});

describe("the request body, in the shop's own notation", () => {
  it('sends a manual line typed the way the shop writes money', () => {
    // **The independent verification's probe, kept** (`V5`): the end-to-end shape of the
    // defect. Total 125, one line typed `45,00` as the UI itself renders amounts. The first
    // version of this module answered `{ok: true}` and sent `manual: []` — the cashier's 45
    // spent by the rule on documents they never named, with a success exit.
    //
    // It is asserted through `collectionRequest`, not `canSubmitCollection`, because the
    // request body is what reaches the server: a gate that says yes and a body that omits the
    // line is exactly the failure, and only the body can show it.
    const draft: CollectionDraft = {
      methodId: 'cash',
      total: '125',
      lines: [{ debtId: 'o-1', outstanding: '80.00', amount: '45,00' }]
    };
    const request = collectionRequest(draft);
    expect(request).not.toBeNull();
    expect(request!.manual).toEqual([{ debt_id: 'o-1', amount: '45.00' }]);
    expect(request!.total).toBe('125.00');
    expect(allocationPreview(draft).remainder).toBe('80.00');
  });

  it('accepts a grouped amount and a wire-shaped one, because both are typed', () => {
    // `1.234,56` is what the screen shows; `1234.56` is what a paste from elsewhere may carry.
    // The locale's parser takes the first; the second is the wire shape and is accepted too, so
    // a value copied out of an API response does not have to be retyped.
    expect(canSubmitCollection({ methodId: 'cash', total: '1.234,56', lines: [] })).toEqual({
      ok: true
    });
    expect(allocationPreview({ methodId: 'cash', total: '1.234,56', lines: [] }).total).toBe(
      '1234.56'
    );
  });
});
