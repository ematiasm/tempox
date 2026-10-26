# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The owner of a small retail business **and one real employee**, as of a 2026-10-20 interview that replaced the earlier "employees are future users" assumption. Two people work the same shop on different shifts: the employee is on the counter, recording sales and looking things up all day; the owner sits behind, reviewing, correcting and deciding. The owner is also the sole developer (beginner, building for their own business).

That second person is not hypothetical and not deferred to v5: it is the constraint that makes "who did this?" a real question this year, and it rules out any affordance only the owner knows how to use. What it does **not** change is the shell: both see one application, and the only difference is which destinations appear. The owner's review work has no screen of its own, by decision — the owner accepted that cost rather than introducing two modes.

## Product Purpose

Tempox replaces paper-plus-spreadsheet bookkeeping for a small business: register a sale once and stock, cash, and history update together. Success has two halves. For the shop: the owner stops double-entering the same event and can answer "what's left?" and "how much did I take?" without manual calculation. For the work: two people share the till without stepping on each other, and the record says who did what.

## Positioning

**Everything connected in one step**: a single sale writes stock, cash, and the record at once — no second entry anywhere. Built on an append-only ledger, so every number the system shows can be explained.

The order between those two claims is settled and it is not negotiable: **recording once is the reason the product exists, and every derived figure is a consequence of that.** Reports exist inside the system (decided 2026-10-20, see Capabilities), and the rule that keeps them honest is one line: **no report may ever justify asking for an extra field at write time.** A number the shop did not record is a number the shop does not get to see.

## Operating Context

Back office at a fixed PC in the shop: desktop-first, keyboard-friendly, mobile secondary. Stack in use: PostgreSQL in Docker, Rust/Axum API, SvelteKit 3 with `@sveltejs/adapter-node` 6 on Vite 8 and Svelte 5; the owner runs everything locally.

## Capabilities and Constraints

- v1 backend shipped and tested: multi-item sales with snapshot lines, products (list price + base unit + initial stock), customers (name-only, seeded `walk-in`), derived cash, derived stock.
- **Purchases shipped (v2 backend)**: recording a purchase raises stock and lowers cash, mirroring a sale in the opposite direction. Suppliers are name-only and seeded with `one-off-supplier`, exactly as customers are seeded with `walk-in`. Cash on hand is what came in from sales, less what was refunded, less what was spent — one number, derived, never stored, and net of a supplier return that brings money back to the till.
- **Supplier returns shipped (v2.5 backend)**: a wrong purchase is corrected by returning the goods, not by editing or deleting it. The return is its own document: stock leaves, the supplier's money comes back, and the purchase keeps every figure it was recorded with. What a purchase still owes — or how much of it came back — is read from its returns, never written onto it.
- The database enforces: decimal money with an exact whole-cents guard, exact totals, immutable events (no edit/delete — a mistaken sale is reversed by a credit note, in whole or in part, never edited).
- **The three surfaces shipped (v1a, v2a)**: `/sales` records a multi-item sale and reads the day's stamped ledger with its cash foot; `/products` is the paginated catalog with search and create; `/purchases` records a purchase with its supplier picker inside the draft, reads the day's purchases, and carries each purchase's returns under the purchase they correct. The rail names those three jobs. `/notes` exists but stays out of the rail, reachable by URL only.
- Compile-time checked queries. The crate compiles with **no database at all**, from the `.sqlx/` offline cache (`SQLX_OFFLINE=true`); CI verifies that cache is current against a real PostgreSQL, so the image build never reaches one.
- **The UI speaks es-AR by default, with an English side** (v1.5). Money is ARS written `$ 1.234,56`, the ledger's day is the shop's own (server-side, `SHOP_TIMEZONE`), and `walk-in` displays as "Consumidor Final" while the stored value stays `walk-in`. The language choice is not persisted: the shop opens in its own language every time.
- Undecided: multi-currency (ARS only), auth mechanics for v5, credit sales scope.
- **Decided 2026-10-20 — two people, one shell, permissions only.** No separate counter mode and no separate owner mode; the employee sees fewer destinations and the owner sees more, in the same application. Roles are v5 work but the *second person* is not deferred: "who recorded this?" is a question the system must be able to answer this year.
- **Decided 2026-10-20 — financial reporting lives inside the system, not only in an export.** The system closes its own books: v6 is no longer purely "derived, always last" as a reason to defer it; it is a promised outcome. The recognised risk, accepted rather than dismissed: this can duplicate what the shop's accountant already does well. The export path is not the deliverable; the in-system report is.
- **Decided for v2**: a purchase is a fact with no status, and a purchase order (with partial reception) is a later, separate document rather than a state of this one. **Decided for v2.5**: returns to a supplier are their own document too, and what a purchase is worth back is read from the purchase line, never sent by the client — the shop records what happened, and recording what was merely asked for is a different job.

## Brand Commitments

Name: **Tempox**. No logo and no identity asset. No voice is committed as a product decision.

**The visual world is "The Computed Account Statement", committed 2026-10-20 and applied across all three surfaces.** `DESIGN.md` records it, and it is the visual authority from here on; it was rewritten from the built result, not from a proposal. (The impeccable toolchain that produced the surface briefs was removed on 2026-10-26; those briefs are recoverable from git history at `17e331b`.) "The Stamped Ledger" — paper ground, hairline rules, ink, one crimson accent on the stamp — is superseded and stands as **anti-reference**: evidence of what the subject is, never authority over what it becomes.

**Built and applied is not the same as settled.** This world carries no protection either: the rail, the composition, the panels, the balance column and the statement gesture are all reopenable under the same standing rule that nothing is sacred. A replacement world is not drift from this record — it is a decision this record already permits, to be made through the visual-world flow and recorded here afterwards.

## Evidence on Hand

Live API verified by curl, by integration tests against a real PostgreSQL, and by Playwright end to end over sales, purchases and supplier returns. There is no customer, benchmark, pricing or testimonial of any kind; future work must not invent one. The `/notes` demo is a walking skeleton left over from v0 — it is **not** proof of the product, and it stays out of the rail on purpose.

## Product Principles

1. **One action, one write**: the sale is the single entry point; stock and cash derive from it. A purchase is the same shape in the other direction — one action, one transaction, and the consequences derive. **No report may justify a new field at write time.**
2. **The ledger is the truth**: balances are sums, never stored fields.
3. **Immutable events**: a mistake is corrected by a reversal, never an edit — a credit note when a customer brings something back, a supplier return when the shop does.
4. **Explicit over clever**: a second person must be able to use it without training. This is now a live constraint, not a hypothetical one — an affordance only the owner understands is a defect.
5. **Trust is the register**: this is a work tool, never an engagement product. Delight is welcome where it does not read as a reward for doing the job — no confetti, no streaks, no achievement for recording a sale. Exuberance is not the enemy; gamification is.

## Accessibility & Inclusion

No product-specific requirement established; standard keyboard-and-screen-reader practice applies. Two constraints follow from the two-person decision above and are not negotiable: the employee operates at the counter under time pressure, so every control must be reachable and legible without training; and "who recorded this?" must be answerable from the record itself, not from memory.
