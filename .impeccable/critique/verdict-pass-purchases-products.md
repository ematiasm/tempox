## verdict

Scoring round. Recaptures were read from the same paths as the review round
(`.impeccable/review/purchases-{desktop,mobile}.png`,
`.impeccable/review/products-{desktop,mobile}.png`); no round-stamped names.

1. **Mobile horizontal scroll is invisible — resolved.** Measured after the fix:
   `/products` frame `scrollWidth 375 > clientWidth 334` with `overflows-x`
   applied and the fade visible in the capture at the right edge; `/purchases`
   scroller `scrollWidth 438 > clientWidth 294` with `overflows-x` applied and
   `ESTADO` now fading into the edge rather than cut mid-word at `EST`. The
   affordance is drawn from measurement, so a table that fits wears nothing.

2. **`STOCK` unreachable in the first view on `/products` at 390px — resolved.**
   `UNIDAD` is hidden at ≤620px; the measured header set is now
   `Producto · Precio de lista · Stock` all visible, with `Unidad` present in the
   DOM but `display:none`. The unit is still legible in the price cell
   (`$ / unidad`), so nothing the row needed was lost.

3. **Purchases desktop empty lower half — resolved.** `.action-foot` gained
   `margin-top: auto`; the capture shows the action at the panel foot
   (y≈831, previously y≈395) and both panels now start and end on the same lines.
   The ~480px of dead surface is gone.

4. **`products.md` missing FORM and seed key — resolved.** The brief now records
   `FORM: inherited by reference from /sales's direction roll (seed 1bde3384)`,
   matching the discipline `purchases.md` already used.

5. **Transitional aliases in `ProductPicker.svelte` — unresolved, by owner
   decision.** The owner declared it a later slice. Not a defect hidden; a scope
   choice on the record. It is not visible in any capture (the picker is closed),
   so it does not affect the visual disposition below.

### Regressions introduced by the fix batch

1. **`PurchaseLedger` briefly had two `table-scroll` blocks and two
   `use:overflowsX` directives.** Caught during verification, not shipped — but
   the first measurement *passed* while the layout was still broken
   (`scrollWidth === clientWidth` because the outer frame lacked `overflow`),
   which is worth recording: a layout defect that reads as "no overflow" is how
   a broken table signs its own approval. Both were removed; the final
   measurement reads the element that actually scrolls.

2. **A shared action now lives in `src/lib/overflowsX.ts`.** New surface area.
   It is 30 lines with one `ResizeObserver` and no dependencies, used by two
   components; if a third ever needs it the duplication it prevents was real.

## remaining

Item 5 stays open as declared scope, not as a defect of these two surfaces.

disposition: fix

**The `fix` is item 5 only, and it is already scheduled as its own slice by the
owner.** Items 1–4 are resolved and visibly so. Per the output contract, a
residual open item cannot recompute to `ship`, so this disposition covers the
five scored findings and not a fresh review of the whole surface.
