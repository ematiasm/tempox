# v1.5-credit-note — tasks (Engram mirror)

Full document: `odd/tasks/v1.5-credit-note.md` (specs authority, verbatim user log).
Engram topic key: `tempox/odd/v1.5-credit-note/tasks`.

**State: closed.** T1–T8 done with their work-unit commits; T9 closed with the native review as an **accepted risk** the owner chose explicitly (document L20). Branch `feat/v1.5-credit-note`, 8 commits on `23a2b67`. Nothing pushed: push and PR creation remain the owner's decision.

## Specs (one line each — the document is authoritative)

S1 credit note is a new document (`credit_notes` + `credit_note_items`), the corrected sale is never written.
S2 history guards extend to both new tables.
S3 cumulative per-line refund guard.
S4 `SELECT ... FOR UPDATE` on the sale serializes refunds.
S5 one transaction: header + items + one positive movement per line.
S6 `stock_movements.reason` gains `'credit_note'`; coherence checks updated; append-only untouched.
S7 404/400/409 ladder; a rejected refund writes nothing.
S8 `GET /api/sales` carries `credit_notes` per sale.
S9 `/api/cash` = confirmed sales − credit notes.
S10 the reversal action lives inside the expanded document, behind an inline two-step confirm.
S11 per-line refund quantity, prefilled with the remaining amount.
S12 the day shows the note under the sale; `Anulado` only when fully refunded.
S13 every new string in `en.ts`, `fr.ts`, `types.ts`.
S14 docs follow the behavior.
S15 test-first per unit, plus a preserved-behavior test for every touched existing path.
S16 e2e reversal flow and the stamp.
S17 work-unit commits, tests and docs beside the behavior.

## Tasks

| ID | Task | Specs | Route | Status | Commit |
|---|---|---|---|---|---|
| T1 | Backend: migration `0007` — tables, history guards, `credit_note` reason | S1, S2, S6 | parent (writer launch refused; worked inline) | done | `97a8f49` |
| T2 | Backend: `POST /api/sales/{id}/credit-notes` | S3, S4, S5, S7 | parent | done | `97a8f49` |
| T3 | Backend: list projection + net cash | S8, S9 | parent | done | `97a8f49` |
| T4 | Backend: integration coverage | S15 | parent | done | `97a8f49` |
| T5 | Frontend: client + types + unit tests | S8, S11 | parent | done | `f6d850e` |
| T6 | Frontend: ledger — refund fields, separated action, notes, day total | S10–S13 | `gentle-ai-worker` (context backstop) | done | `f6d850e` |
| T7 | Frontend: e2e reversal flow | S16 | `gentle-ai-worker` (context backstop) | done | `f6d850e` |
| T8 | Docs | S14 | parent | done | `5514c2e` |
| T9 | Seam check + independent verification + closure | S17 | parent | done (review accepted-risk) | `523015b`, `49a259e`, `2ecc646` |

## Corrections the verification forced (both real, both closed)

- **S3 was genuinely unmet.** One request naming the same `sale_item_id` twice bypassed the cumulative guard (2×6 against a line of 10 → `201`, refunded 12). The row lock was correct and did not help: the second entry is not a second transaction. Fixed by summing the running total inside the loop; RED reproduced against the pre-fix handler; pinned by `v1_5_one_request_cannot_over_refund_a_line_by_repeating_it` and a boundary test. Commits `91f996b`, `523015b`.
- **The docs overclaimed, so the schema moved.** `architecture.md` claimed the new tables reject `UPDATE`/`DELETE`/`TRUNCATE`; only the economics were guarded, so rewriting `created_at` (which decides the note's day in the ledger) and deleting a childless note were accepted. Fixed in migration `0008` — not by editing the applied `0007`. Commit `91f996b`.

## Accepted risk (owner's decision, document L20)

The two-lens native review (`risk`, `reliability`) was created (lineage `review-a75a3be004742020`, tier medium) and is owed, but its reviewer slots are unreachable: the facade cannot deliver the `consent/v3` envelope, and a lineage created around the facade has no registered capture route. Filed with evidence as `Gentleman-Programming/gentle-shell#2346`. The owner chose to accept the candidate unreviewed on the strength of the independent verification that did run — which found the two defects above rather than rubber-stamping. The code under review is byte-identical to the code that was verified.

## Log (tail)

L1 (user, verbatim): "ahora si empecemos con la parte v1.5 del plan, pero solo la parte de anular (nota de credito) es decir, revertir una venta"
L2–L6 (user decisions, 2026-10-16): new-document credit note; action inside the expanded row; reversal only (es-AR later); partial cumulative refunds; `ANULADO` stamp on the sale with the note rendered under it.
L13–L15 (verification): an independent verifier found S3 unmet and the docs overclaiming; a bounded recheck closed both.
L17–L19 (review lifecycle): the facade could not deliver the consent envelope; the grant was applied through the native CLI and the review was created; the defect was filed as gentle-shell#2346.
L20 (user, verbatim): "2" — accept the candidate unreviewed.
