disposition: fix

# Finish review — /purchases and /products

**Substitution disclosed:** this harness has no subagent capability, so the Impeccable Finish Reviewer ran inline. Both roles — output contract and fix application — are played by the same agent; the review below was written before any fix was applied, from captures taken before reading any contract.

**Inputs missing from the packet**, named as check 1 requires:

- No approved comp. This is a code-led build: `.impeccable/mocks/decision/` holds the direction round's dealt hand (`assigned.html`, `canon.html`, `challenger-labanotation.html`, `model-pick.html`, `working-sheet.html`, `ledger-doc.html`, `command-line.html`), which check 1 exempts explicitly. No comp round ran, so there is no `approved comp` and no `.impeccable/build/state.json`.
- No QUALITY BAR card exists anywhere under `.impeccable/`. Check 3 therefore cannot score commitment against the world's card, and says so rather than inventing a bar.

Consequence, applied per check 2: with no approved comp, **TYPE** and **MATERIAL** do not lapse — they are judged against the contract's OWN-WORLD. **GROUND** narrows rather than lapses: OWN-WORLD names the colors, so sampled pixels against `DESIGN.md` are the authority.

## persistence

**Pass, with one disclosure.**

`PRODUCT.md` exists and describes the shop that exists: two people (owner and one employee), in-system reports, "nada es sagrado" as the standing rule against frozen decisions. `.impeccable/surfaces/purchases.md` and `.impeccable/surfaces/products.md` both exist and both open by stating this is an **extension**, inheriting `DESIGN.md` with no direction roll.

`DESIGN.md` predates this build — it is an extension — and check 1 requires it to match the built world. It does: the nine declared colors are the ones the captures show, and the Named Rules ("The Balance Owns the Accent Rule", "The State Is Readable Rule") are the rules the two surfaces actually follow.

The disclosure: the FORM seed key. `sales.md` carries seed `1bde3384`, `purchases.md` inherits by reference, but `products.md` — which I wrote in this session — carries **no seed key and no FORM block at all**. Check 4 makes a missing seed key a material finding ahead of any craft point. It is real, and it is mine.

## fidelity

No element matrix against an approved comp: there is no comp. The matrix below is against OWN-WORLD as `DESIGN.md` states it, with the captures as the only evidence.

| Element | State | Evidence |
|---|---|---|
| **GROUND** | match | `rgb(251, 252, 253)` sampled on `h1` and `body` on both surfaces, equal to `/sales`. Cold near-white, no drift toward warm cream. |
| **TYPE** | match | Plex Condensed in headings, tabular figures in every money column, amounts right-aligned and column-aligned. No system display face, no monospace costume. |
| **MATERIAL** | match | Flat surfaces, no faked physicality: no CSS bevels, no embossing, no stamped-metal. The previous world's rotated rubber stamp is gone from `/products`; what replaced it is a plain `DERIVADO` label. |
| Containment by surface | match | Two closed panels, own 1px edge each, 24px gap, 8px radius. Both surfaces. |
| Header document/register count | match | `4 COMPROBANTES` on purchases, `50+ LISTADOS` on products. The `+` is the honest form after the R3-001 correction. |
| Primary action treatment | match | Solid ink bar at the panel foot, dashed outline plus stated reason when disabled, `F2` printed. Identical on both. |
| Period footings (purchases) | match | `COMPRADO $210,00 · DEVUELTO $50,00 · COSTO DEL PERÍODO $160,00`. Arithmetic exact, from the server. |
| State as text plus tick (purchases) | match | `| CONFIRMADA` — a 2px tick and a word, not a filled pill. |
| **Mobile table overflow** | **contradicted** | Table scrolls horizontally (`scrollWidth 438 > clientWidth 294` on purchases; `385 > 334` on products) with **no visible affordance**. The capture shows `EST` truncated mid-word on purchases and `S` on products — a reader cannot tell a column is cut rather than absent. |
| Products mobile: stock column absent from first view | **contradicted** | `STOCK` is the one number the surface exists to report and it sits beyond the horizontal cut at 390px. The row shows product, unit, price; the shelf count needs a swipe nobody is told about. |
| Empty vertical space, purchases desktop | **contradicted** | The draft panel's action sits at `y≈395` and the panel continues to `y≈874`; roughly 480px of empty surface below a disabled action, while the ledger's four rows occupy the right column. The panel is a column that fills its track, but its content does not fill the panel. |
| ProductPicker aliases | **added without approval** | `ProductPicker.svelte` still carries 2 transitional alias uses; `DESIGN.md` says a new component must not use a name from the transitional block. Not visible in either capture (the picker is closed), so this is a code-truth finding, not a visual one. |

## ceiling

**Unscored — no QUALITY BAR card exists.** Naming it rather than inventing a bar, per check 3.

What the captures do show, judged only against the craft floor's Refuse list in check 6: no kicker or eyebrow, no gradient text, no hard offset shadows, no glyph icons, no side stripes, no nested cards. The accent appears on exactly two jobs across both surfaces — the active rail destination, and the focus ring on the purchase search field. The balance on `/sales` keeps its own.

The world's unused native devices, for the record, are motion and density: both surfaces are entirely static, and the statement world could carry a settling transition on a newly recorded document without decoration.

## material_fixes

Ordered, most material first. Fidelity failures ahead of craft.

1. **Mobile horizontal scroll is invisible (both surfaces).** The tables scroll but nothing says so. Add the scroll affordance the sales ledger already earned: an edge fade at the cut and a count of what is beyond it. The desktop capture shows this pattern working on `/sales`; it was not carried to `/purchases` or `/products`. — check 2, mobile overflow row.
2. **`STOCK` is unreachable in the first view on `/products` at 390px.** The surface's primary fact sits past the cut. Either drop `UNIDAD` to a secondary treatment so `STOCK` fits, or surface stock in the row's primary line. A reader must not have to discover a swipe to learn the count. — check 2, products mobile row.
3. **Purchases desktop: the draft panel's empty lower half.** The action lands at ~395 and the panel runs to 874. Either the panel stops where its work stops, or the emptiness carries something true. An action floating above 480px of nothing reads as a page that ran out of content. — check 2, empty vertical space row.
4. **`products.md` has no FORM block and no seed key.** Check 4 makes this material ahead of craft. The brief records direction for the other two surfaces and, for this one, only structure. Add the FORM/seed or record explicitly that it inherits `sales.md`'s seed by reference, as `purchases.md` does. — check 4.
5. **Transitional aliases still live in `ProductPicker.svelte` (2 uses).** `DESIGN.md` names the block as debt a new component must not touch. Declared for a later slice by the owner, but the finding belongs on the record. — check 2, ProductPicker row.

## keep

Keep the two closed panels, the ink action bar at the panel foot, the honest `50+` count, the derived-stock label, and the accent's restraint to two jobs per surface. Fixing the mobile affordance must not introduce a second accent use to do it.
