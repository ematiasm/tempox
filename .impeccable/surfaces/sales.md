---
version: 1
slug: "sales"
primary_target: "sales"
related_targets: ["products"]
---

## Surface brief — Tempox v1a

Scope: `/sales` (primary) + `/products` (related) + shell navigation. Mode: operate.
Audience: shop owner and employees at a fixed desktop PC in the shop; keyboard-first, mobile secondary.
Job: record a multi-item sale in one motion; manage the product catalog and see stock at a glance.
Primary action: Record sale. Proof: the live v1 API — 11 unit tests and 9 integration tests green.
Constraints: English UI until the es-AR i18n layer; money displayed en-US; volumes 500+ products / 50+ sales per day, so type-ahead search and scoped lists; clean-DB launch with honest empty states.
Memorable moment: the stamp that lands on a confirmed sale.

## Direction contract

THESIS: Every event wears its state on its face — a sale is a document with its stamp, not a row with a status pill. The category default it refuses: the generic admin CRUD where state is a colored badge nobody reads.
OWN-WORLD: Notebook-paper ground, hairline ledger rules, and stamp inks only — crimson for confirmed, indigo for neutral marks, near-black for ink. One condensed humanist for figures and headings, the system sans for text, tabular figures on every amount.
STORY: The cashier records the sale once; the stamp lands CONFIRMED while stock and cash have already moved. The day reads as stamped documents, and an ANULADO is visible the instant it exists.
FIRST VIEWPORT: Rail navigation with Sales and Products (the active tab extended); a crimson band holds the single next action, Record sale. On /sales: the draft sale sheet at the head of the book — type-ahead product field, quantity, price ($/unit), customer with inline create, its lines set exactly like the lines of a stamped sale, and the live total closing it — with the day's ledger below, newest first, each document opening onto its own lines.
FORM: Grounded candidate 7 of the operate seed (key 3abfcb71), raised with the kept disciplines of the declined challengers: destructive actions isolated by empty space, tab-rail navigation, a ranked state legend, and one saturated action panel.
COMPOSITION: the ledger document (surface seed key 17aa8db8, locked 2026-10-14). The day's documents are the screen, not a view of it: the draft sale is a sheet standing at the head of the book and the ledger follows it as stamped documents, newest first, each opening onto its lines. Refused from the same hand: the command line (entry owns the top band and the day drops to a card grid — clearer about the recording motion, but a 50-sale day pushes far further down the page) and the working sheet (one sheet, two flush columns, no cards — the book a clerk would recognise, but nothing announces which half is the draft). The cash card moves to the foot of the ledger in this composition; the direction's palette, type and stamp are unchanged.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance.
