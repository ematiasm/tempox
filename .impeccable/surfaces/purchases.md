---
version: 1
slug: "purchases"
primary_target: "purchases"
related_targets: ["products"]
---

## Surface brief — Tempox purchases (extension)

Scope: `/purchases` (primary) + its supplier picker. Mode: operate.

**This is an extension, not a new world.** `/sales` established the visual world and `DESIGN.md` records it; this surface inherits both and resolves only its own structure, content and states. No direction roll, no concept tournament, no `DESIGN.md` change.

Audience: the same two people. The owner records purchases and returns; the employee never does — buying is irreversible and stays with the person who answers for the money.
Job: record a purchase from one supplier in one motion, and read the period's buying without calculation.
Primary action: Registrar compra. Proof: the live API, its integration tests, and the same scope/período contract `/sales` already uses.
Constraints: es-AR, ARS, the shop's own day via `SHOP_TIMEZONE`; a purchase is a fact with no status, so a row is confirmed the moment it exists; supplier returns are a second document, never an edit; the supplier picker creates inside the draft rather than on its own screen.

## Direction contract

Inherited from `/sales` (`.impeccable/surfaces/sales.md`). Not restated here: a copy would drift.

What this surface resolves on its own, decided 2026-10-20 by the owner:

- **Structure: a table, mirroring `/sales` exactly.** The current surface is an expandable document list; it becomes the same table the sales ledger uses, with `COMPRA` where `VENTA` sits and `PROVEEDOR` where `CLIENTE` sits. The whole row opens the document's lines — the same `role="button"` row, the same chevron as affordance, the same keyboard reach.
- **No running balance.** The signature column stays `/sales`'s. In a purchase ledger a cumulative figure would answer a question the shop does not ask here — what it spent in a period is the footer's job, and a column of withdrawals invites reading it as a till. `SALDO` is absent on purpose, not forgotten.
- **The footer reports the period, from the server.** Same rule the sales fix established: a total derived from the loaded page undercounts. `GET /api/purchases` gains a summary the way `GET /api/sales` did, or the footer says plainly what set it covers.
- **Supplier returns stay where they are**: inside the expanded document, under the purchase they correct, as a second row rather than an edit.

Not this surface's work: `/products`, which remains the last un-redesigned screen.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, and the shell test that measures ink and ground across all three destinations un-skipped.
