---
version: 2
slug: "sales"
primary_target: "sales"
related_targets: ["products"]
---

## Surface brief — Tempox v2-redesign

Scope: `/sales` (primary) + shell navigation. Mode: operate.
Audience: two people, one shell, permissions only — the employee is on the counter recording sales all day; the owner sits behind reviewing and correcting. Both at a fixed desktop PC; keyboard-first, mobile secondary. A second real person is live this year, so no affordance may depend on the owner's private knowledge.
Job: record a multi-item sale and read the day without calculation.
Primary action: Registrar venta. Proof: the live API and its integration tests; the real numbers this surface shows are sales, credit notes and the derived till.
Constraints: es-AR UI, ARS as `$ 1.234,56`, the shop's own day via `SHOP_TIMEZONE`; volumes of 50+ sales per day, so the day's list is scoped and scannable; the till is derived, never stored; a second person must be able to use it without training; no gamification of any kind.
Memorable moment: the sale joining the day's list as a computed row, with the running balance recomputed beside it.

## Direction contract

THESIS: The printed account statement is the only money document the shop already trusts, so the day reads as one — every row a computed figure, every balance recomputed. It refuses the category default of a CRUD screen where the state is a coloured pill nobody reads.
OWN-WORLD: Cold near-white ground `#fbfcfd`; one ink `#12212e`; secondary grey `#5f6f7d` (5.04:1 on the ground, not the `#6b7c8a` that failed AA); one accent `#0b5f8a` reserved to the running balance and the active destination, never decoration. Hairline rules `1px solid #e8ecf1` appear only where something is calculated. IBM Plex Sans Condensed for every figure with tabular numerals; IBM Plex Sans for prose. No gradients, no filled state pills: a state is text plus a 2px tick, and a cancelled amount is struck through.
STORY: The employee types the product, the sale records, and the day grows by one computed row with the till moving beside it. The owner scrolls the same list and can see, without arithmetic, what the day is worth and where it was corrected.
FIRST VIEWPORT: An 88px rail listing the destinations in condensed caps, active one marked by the accent as a left rule and accent text — not a filled block. To its right: a header line with the day's document count, then two closed panels separated by a 24px gap, not by air alone — each panel carries its own `1px` border, an 8px radius and `overflow:hidden`, so every area starts and stops on the page. Left panel (560px minimum): the draft sale — product search with its `F2` key hint, the entered lines with quantity, unit price and line total, an honest ghost row, the draft total, and the single primary action as a solid ink bar (never the accent). Right panel: the day's ledger, newest first, columns HORA · DOCUMENTO · CLIENTE · ESTADO · IMPORTE, with the running balance recomputed per row in the left margin — the signature that makes the column readable downward — and the till balance in the panel footer.
FORM: Assigned by the direction roll (seed `1bde3384`): the bank-statement world, which was also this run's top-ranked grounded candidate. Composition taken from the category-standard round, by the owner's explicit choice on 2026-10-20: containment by surface — closed panels with borders and radius — over the continuous flow of the same roll's first sketch.
Deliberate deviations from the locked sketch, decided 2026-10-22 and recorded here so they stop reading as drift:

- **Rail order is Ventas · Compras · Productos.** The sketch `.impeccable/mocks/decision/model-pick.html` lists Ventas · Productos · Compras. The order follows the work: sell, restock, then look after the catalogue. The sketch's order was its own copy of the v1a rail, never a decision about this world.
- **The draft column is 560px minimum, not the sketch's narrower draft.** The draft's table does not compress below 550px, so a smaller track let it escape its panel between 1350px and the old 1080px breakpoint. The contract's `FIRST VIEWPORT` carries this number now.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.

## Superseded

The v1a contract below is history. Its "Stamped Ledger" world (notebook paper, crimson stamp) is declared anti-reference in PRODUCT.md as of 2026-10-20; nothing in it binds this build.

THESIS: Every event wears its state on its face — a sale is a document with its stamp, not a row with a status pill.
OWN-WORLD: Notebook-paper ground, hairline ledger rules, stamp inks — crimson for confirmed, indigo for neutral marks, near-black for ink.
STORY: The cashier records the sale once; the stamp lands CONFIRMED while stock and cash have already moved.
FIRST VIEWPORT: Rail with Sales and Products (active tab extended); a crimson band holds Record sale; the draft sheet at the head of the book with the day's stamped ledger below.
FORM: Grounded candidate 7 of the operate seed (key 3abfcb71), raised with the kept disciplines of the declined challengers.
COMPOSITION: the ledger document (surface seed key 17aa8db8, locked 2026-10-14); the cash card at the foot of the ledger.
