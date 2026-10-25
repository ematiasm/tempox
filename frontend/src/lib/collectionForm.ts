/**
 * The collection form's arithmetic: what was received, what the cashier allocated by hand,
 * and what the rule will therefore spend.
 *
 * `odd/tasks/v5-payments.md` S9. The form in the balance box has three inputs — the method,
 * the amount received, and one optional amount per unpaid document — and this module turns
 * them into a request. It is pure, so the rules below can be read and tested without a
 * component, a network or a locale.
 *
 * **It computes the remainder and refuses to predict its destination.** The server owns
 * which documents the remainder closes (`S3`: manual first, then oldest first, and never back
 * into a document the cashier named). Reproducing that here would be a second implementation
 * of one rule, and this repository has paid for that class three times in a single slice
 * (`V7b` of `v5-customer-account.md`: a server bug, a render bug and a test that compared a
 * value with itself were all the same shape). So the form shows a figure and a sentence, and
 * the server's `applied` — with its `source` on every row — is the only statement of what
 * happened.
 *
 * **Every amount is read by the active locale's own parser**, `parseAmount`. That is not a
 * detail: this UI is es-AR and shows `$ 1.234,56`, so a hand-rolled parser that accepted only
 * a dot would reject the very shape the shop writes. The first version of this module did
 * exactly that, and the defect was silent rather than loud — an unparseable line was dropped
 * from the request while the form still submitted, so the money the cashier allocated was
 * spent by the rule on other documents (review `review-b6bebae2ab91c89f`, `R3-001` / `R4-1`,
 * both CRITICAL).
 *
 * **Nothing unparseable is ever dropped.** A line is either blank — "not allocated", the
 * ordinary state of a form being filled — or an amount, and an amount that does not parse
 * stops the submit and names the field. "Blank" and "garbage" are different facts, and
 * collapsing them is what turned a typo into a plan the cashier never made.
 *
 * `remainder` can be **negative**, and that is a feature: a cashier who allocates 120 of 100
 * has made a real mistake, and clamping it would let the form claim a plan the request will
 * not reproduce. The flag says so and the gate refuses it.
 */
import { parseAmount } from './money.js';

/** One unpaid document, as the account payload lists it. */
export type CollectionLine = {
  /** The obligation (a client) or the payable (a supplier) — the document's `id`. */
  debtId: string;
  /** What that document still owes, as the account payload reports it. */
  outstanding: string;
  /** What the cashier typed for it, in **their own locale's shape**. Blank means unallocated. */
  amount: string;
};

export type CollectionDraft = {
  methodId: string;
  /** What arrived, or what is being paid out, in the cashier's own shape. */
  total: string;
  lines: CollectionLine[];
};

/** Why the submit is not ready. One value per field, so the hint names one thing. */
export type CollectionBlocker =
  | 'method'
  | 'total'
  | 'lineUnreadable'
  | 'overAllocated'
  | 'lineOverDebt'
  | 'duplicateLine'
  | 'lineNotPositive';

export type CollectionPreview = {
  /** The amount received, as typed. */
  total: string;
  /** What the cashier allocated by hand, as a wire amount. `0.00` when nothing was typed. */
  manualTotal: string;
  /** `total − manualTotal`. Negative when the plan exceeds the money, and never clamped. */
  remainder: string;
  /** True when the plan asks for more than arrived. */
  overAllocated: boolean;
  /** The ids of documents whose own amount exceeds what they owe. */
  lineOverDebt: string[];
  /** The ids of lines holding text that is not an amount in the active locale. */
  lineUnreadable: string[];
  /** How many lines the cashier actually filled in. */
  allocatedLines: number;
};

// ── Money, as whole cents ───────────────────────────────────────────────────
//
// The wire format is a decimal string with two decimals; the locale's `parseAmount` reads
// what a person types. What is needed on top of `money.ts` is a subtraction that can go
// **negative** and stay exact, which is why this stays in `BigInt` cents: `0.1 + 0.2` is why
// every figure in this repository is cents. These helpers are deliberately local rather than
// generalising `money.ts`, whose contract is "the API's amount string" and whose callers all
// assume a non-negative total.

/** A **wire** amount string into whole cents, or `null` when it is not one. */
function wireCents(amount: string): bigint | null {
  const text = amount.trim();
  if (!/^-?\d+(\.\d{1,2})?$/.test(text)) return null;
  const negative = text.startsWith('-');
  const [whole, fraction = ''] = (negative ? text.slice(1) : text).split('.');
  const value = BigInt(whole) * 100n + BigInt(fraction.padEnd(2, '0'));
  return negative ? -value : value;
}

/** Whole cents back into the wire's shape: exactly two decimals, sign preserved. */
function amount(value: bigint): string {
  const negative = value < 0n;
  const digits = (negative ? -value : value).toString().padStart(3, '0');
  return `${negative ? '-' : ''}${digits.slice(0, -2)}.${digits.slice(-2)}`;
}

/**
 * What one line's text means, and there are three answers, not two.
 *
 * - `{ kind: 'blank' }` — the cashier has not touched it. Not an allocation, and not an error.
 * - `{ kind: 'cents' }` — a real allocation, in the active locale's shape.
 * - `{ kind: 'unreadable' }` — text that is not money in this locale. **A refusal**, because
 *   dropping it would spend the money elsewhere without saying so.
 */
type LineReading = { kind: 'blank' } | { kind: 'cents'; cents: bigint } | { kind: 'unreadable' };

function readLine(line: CollectionLine): LineReading {
  if (line.amount.trim() === '') return { kind: 'blank' };
  // The locale's parser, not a regex here: `parseAmount` is what every other amount field in
  // this application uses, and it is the only thing that knows es-AR writes `1.234,56`.
  const parsed = parseAmount(line.amount);
  if (parsed === null) return { kind: 'unreadable' };
  const cents = wireCents(parsed);
  if (cents === null) return { kind: 'unreadable' };
  return { kind: 'cents', cents };
}

/**
 * The figures the form shows. Never throws: an unreadable line contributes nothing **and is
 * reported**, which is what lets the gate refuse it by name.
 */
export function allocationPreview(draft: CollectionDraft): CollectionPreview {
  const totalText = parseAmount(draft.total);
  const totalCents = (totalText === null ? null : wireCents(totalText)) ?? 0n;
  let manualCents = 0n;
  let allocatedLines = 0;
  const lineOverDebt: string[] = [];
  const lineUnreadable: string[] = [];

  for (const line of draft.lines) {
    const reading = readLine(line);
    if (reading.kind === 'blank') continue;
    if (reading.kind === 'unreadable') {
      lineUnreadable.push(line.debtId);
      continue;
    }
    allocatedLines += 1;
    manualCents += reading.cents;
    const outstanding = wireCents(line.outstanding);
    // A line above its own document's outstanding is a **separate** fault from the plan
    // exceeding the total: one can happen with room to spare (allocate 150 of 500 against a
    // document that owes 100). Reporting it as the total's fault would name the wrong field.
    if (outstanding !== null && reading.cents > outstanding) lineOverDebt.push(line.debtId);
  }

  const remainderCents = totalCents - manualCents;
  return {
    total: amount(totalCents),
    manualTotal: amount(manualCents),
    remainder: amount(remainderCents),
    overAllocated: remainderCents < 0n,
    lineOverDebt,
    lineUnreadable,
    allocatedLines
  };
}

export type CollectionGate = { ok: true } | { ok: false; reason: CollectionBlocker };

/**
 * Whether the form can be submitted, and if not, which single field to name.
 *
 * The order is the order the fields appear in, and the first problem wins: an operator told
 * about two problems at once fixes neither reliably. Each check corresponds to something the
 * **server** refuses, so this gate never refuses work the server would accept — the tempting
 * extra check ("the total must not exceed what is owed") is exactly that mistake: money beyond
 * every debt is a credit balance (`S3.4`), which is a real state.
 */
export function canSubmitCollection(draft: CollectionDraft): CollectionGate {
  if (draft.methodId.trim() === '') return { ok: false, reason: 'method' };

  const totalText = parseAmount(draft.total);
  const total = totalText === null ? null : wireCents(totalText);
  if (total === null || total <= 0n) return { ok: false, reason: 'total' };

  // **Every filled line is read, and an unreadable one refuses the submit** (`R3-001` /
  // `R4-1`). The order inside the loop matters: unreadable before duplicate, because a line
  // whose text is not money says nothing about which document it was meant for.
  const seen = new Set<string>();
  for (const line of draft.lines) {
    const reading = readLine(line);
    if (reading.kind === 'blank') continue;
    if (reading.kind === 'unreadable') return { ok: false, reason: 'lineUnreadable' };
    // **A typed zero is an allocation, and the server refuses it** (`S4` checks every manual
    // amount: an amount that is not positive is a `400`). A blank line is the opposite — the
    // ordinary state of a form being filled — and collapsing the two is the mistake this loop
    // cannot make, because `readLine` distinguishes them.
    if (reading.cents <= 0n) return { ok: false, reason: 'lineNotPositive' };
    if (seen.has(line.debtId)) return { ok: false, reason: 'duplicateLine' };
    seen.add(line.debtId);
  }

  const preview = allocationPreview(draft);
  if (preview.overAllocated) return { ok: false, reason: 'overAllocated' };
  if (preview.lineOverDebt.length > 0) return { ok: false, reason: 'lineOverDebt' };

  return { ok: true };
}

/**
 * The request body the form sends, or `null` when the form is not ready.
 *
 * The lines are **only the ones the cashier filled in**: sending a blank line as `0.00` would
 * be an allocation the server refuses (`S4` checks every manual amount), and sending every
 * document with an amount would be the form deciding the plan. An **unreadable** line cannot
 * reach here at all — the gate refuses it — which is the difference between "nothing was
 * typed" and "something was typed that I did not understand".
 */
export function collectionRequest(draft: CollectionDraft): {
  payment_method_id: string;
  total: string;
  manual: { debt_id: string; amount: string }[];
} | null {
  if (!canSubmitCollection(draft).ok) return null;
  return {
    payment_method_id: draft.methodId,
    total: allocationPreview(draft).total,
    manual: draft.lines.flatMap((line) => {
      const reading = readLine(line);
      if (reading.kind !== 'cents') return [];
      return [{ debt_id: line.debtId, amount: amount(reading.cents) }];
    })
  };
}
