---
version: 1
slug: "products"
primary_target: "products"
related_targets: ["sales", "purchases"]
---

## Surface brief — Tempox products (extension)

Scope: `/products`, the catalog and its create form. Mode: operate.

**This is an extension, not a new world.** `/sales` established the visual world and `DESIGN.md` records it; `/purchases` proved it survives a second surface. This surface inherits both and resolves only its own structure, content and states. No direction roll, no concept tournament, no `DESIGN.md` change.

Audience: the same two people.
Job: find a product and read what it costs and how many are on the shelf; create one when the shop starts selling something new.
Primary action: Nuevo producto. Proof: the live API, `GET /api/products`, and the existing e2e suites that already read `table.catalog` and `#new-name` from this page.

## What this surface is, and what it is not

Every other screen in this product records a **movement** — a sale, a purchase, a return. `/products` is the opposite kind of thing: it is the **register of what exists**. Nothing here happens; the screen reports a standing state.

That distinction, not decoration, decides the layout. A ledger reads down a day in time order and its signature column is a running figure. A register is scanned by name and its numbers are facts about a thing, not totals over a period. Copying the day ledger's shape here would be a mechanism imported where its reason does not hold.

What it shares with the other two surfaces is everything that made them one world: the ground, the ink, the one accent, the panels, the figures set in the condensed face, the action at the foot of the work it closes, no coloured band.

## Direction contract

Inherited from `/sales` (`.impeccable/surfaces/sales.md`) and ratified by `/purchases`. Not restated here: a copy would drift.

**FORM: inherited by reference from `/sales`'s direction roll (seed `1bde3384`)**, the same way `/purchases` inherits it. This surface ran no direction roll and no concept tournament — it is an extension, and the world was chosen before it existed. Recorded explicitly because the finish review (check 4) reads a brief with no FORM and no seed as a skipped roll, which is a material finding ahead of any craft point. It was missing here and present in both sibling briefs; the omission was mine, not the roll's.

What this surface resolves on its own, decided 2026-10-20 by the owner:

- **Two panels, side by side, mirroring the other two surfaces.** The catalog is the wide left column — it is the work. The create form is the narrow right column. The current page stacks them vertically with the form second, which buries the page's single action below a paginated table.

  *Reversal of an earlier decision, recorded because the log is append-only:* the form first sat on the left, on the argument that reading follows creating. The owner ruled against it — the catalog has 500+ rows and is what the screen is for. The form takes the narrow column and the action sits at its foot.

- **The catalog is a table, and it already is one.** `PRODUCTO · UNIDAD · PRECIO · STOCK` stays. This is the one surface where a table was already the right shape, so the change is not structural: it is the frame around it (panel edges, scrolling rows, a footer that reports the period rather than the page).

- **The count moves to the page header.** `/sales` and `/purchases` both carry the period's document count beside the heading. Here the count is not a period — it is how many products the shop sells, or how many a search matched. It goes to the header for the same reason: the size of what you are looking at is a fact about the screen, not a caption under a table.

- **Stock is derived, and the screen says so once.** It is never typed here; it arrives from the movement ledger. That rule already exists in the markup as a footer note and it stays, because it is the single most misread number on the page — a shop that believes stock is editable will try to fix it by typing.

- **The accent stays unspent except where it already is.** The catalog's stock cell uses the void tone for an empty shelf and a warning tone below ten. Neither is the accent. The accent belongs to the figures that carry a period, and there is no period here — so this screen is the one where the accent appears nowhere, and that is the correct amount.

- **F2 keeps meaning what it means.** It puts the caret in the create form's first field. The page's action prints it.

## States that must exist

The existing states are kept, not redesigned: error, loading skeleton, empty catalog, empty search result with its clear-search action, create-form validation, created confirmation, create error. The redesign changes the frame and the tokens, never which states the operator can reach.

## Not this surface's work

- Editing or deleting a product. The API does not offer it and this is not the slice that adds it.
- Any change to how stock is computed. It is derived, it stays derived.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, and — because this is the last surface — `shell.spec.ts` measuring ink and ground across **all three** destinations, the `/products` guard test deleted, and `.impeccable/surfaces/sales.md`'s forward-looking note about the remaining surface retired.
