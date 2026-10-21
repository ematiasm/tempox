## verdict

Scoring round. Recaptures were read from the same paths as the review round
(`.impeccable/review/purchases-{desktop,mobile}.png`,
`.impeccable/review/products-{desktop,mobile}.png`); no round-stamped names.

1. **Mobile horizontal scroll is invisible — resolved, on the second attempt.**
   The first attempt was a false success and the review caught it: the finding
   asked for "an edge fade at the cut **and a count of what is beyond it**", and
   only the fade shipped while this document marked it resolved. A gradient says
   _there is more_ without saying whether it is one column or five.

   The count now exists (`overflowsX` reports the number of headers past the
   cut; both surfaces render `N columnas más →`). Verified at 320px, where an
   overflow is real: `data-hidden-columns="1"` and the text reads
   `1 columna más →`.

   Verifying it also exposed that neither table overflows at 390px any more, and
   that the earlier "resolved" had a second falsehood behind it: `UNIDAD` was
   hidden on the `<th>` but not on the `<td>`, so the column kept its 134px and
   the table still scrolled with zero hidden headers — a swipe that revealed
   nothing. Fixed; measured after: `scrolls: false` on both surfaces at 390px,
   which is the correct state, so the cue does not appear at all there.

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
   the first measurement _passed_ while the layout was still broken
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

---

**Correction (second pass, after native review R3-001).** This document's first
version claimed finding 1 resolved while delivering only half of it. The native
review flagged that as a CRITICAL false success claim, and it was right: the
reviewer that wrote the finding and the builder that marked it resolved were the
same agent, and the builder graded itself against its own summary rather than
against the text of the finding. The count is implemented and measured now; the
`UNIDAD` mismatch it exposed is fixed too. Recorded here rather than silently
rewritten, because a verdict is evidence and evidence that gets quietly edited
after the fact is not evidence.
