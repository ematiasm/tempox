# Finish review — `/sales` + `/products`

disposition: fix

Scope: the v1a surfaces on branch `feat/v1a-sales-screen`. Code-led build: no
approved comp exists, so there is no element matrix against one and no
comp-diff scores. The locked structure (`ledger-doc.html`, chosen by the user
from three dealt sketches of surface seed `17aa8db8`) rides as a critique
reference, not as a spec.

Evidence: `.impeccable/review/desktop.png` (1440), `mobile.png` (390),
`products-desktop.png` (1440). All three valid at capture time.

## persistence

PASS. DESIGN.md exists and describes the built world; `.impeccable/design.json`
carries the sidecar the panel reads. The surface brief holds the direction
contract with both seed keys: FORM carries `3abfcb71` (the direction roll) and
COMPOSITION names `17aa8db8` and the locked pick, including the cash card's move
to the foot.

No comp round is owed: this environment has no image generation, the plan
records no `buildPath`, and a code-led round is the only path. There is no
`.impeccable/build/state.json` and its absence is not a finding here.

## fidelity

No approved comp to measure against, so the check is against the contract's
FIRST VIEWPORT and COMPOSITION blocks and the chosen sketch's own anatomy.

| Element | Verdict |
|---|---|
| Rail with two destinations, active tab extended | match |
| Crimson band carrying the page's single next action | match |
| Draft sheet at the head of the book, live total closing it | match |
| Ledger below, newest first, documents opening onto their lines | match |
| Cash figures at the foot | match |
| The stamp, at three sizes | match |
| **TYPE** | match — condensed humanist on every figure and heading, system sans on text; tabular on every amount including inside the quantity and price inputs |
| **MATERIAL** | match — the stamp is real ink registration: 2px border, −6.5° rotation, `mix-blend-mode: multiply`, and a second impression of the same word offset a hair at 28% opacity. No faked physicality anywhere: no CSS bevel, no emboss, no stamped-metal effect |
| **GROUND** | match — sampled from the capture: page field `#f6f2e9`, the paper OWN-WORLD names. Not drifted toward the warm-cream prior |
| Picker match list opening on focus | **contradicted** → fixed |
| Two "Ctrl+Enter records" hints | **added without approval** → fixed |
| `F2` printed on the band action but bound to focus | **contradicted** → fixed |

The three defects above were introduced during this build's own hardening pass,
not inherited: the picker's `onfocus` was harmless until the route started
focusing the field on arrival.

## ceiling

Devices the world left unused:

- **No printed field for the ledger's own enumeration.** The footings are set as
  a four-column strip; a ruled page would have carried them as a true column of
  figures with a closing rule. Cosmetic, not structural.
- **The rail's counterpart in print is a tab index** — the active tab extends,
  which is the move, but the remaining tabs carry no extent cue (a real index
  scales tab height by section length). Unused.
- Motion is spent entirely on the stamp, which is the contract's own choice
  ("one authored moment"), so the hem is deliberate rather than a gap.

## material_fixes

All five found by this review are resolved and committed; the list is kept for
the record.

1. **Fixed** — the ledger's disclosure control was a text glyph (`▸`), which the
   floor refuses: a glyph inherits the font's metrics and renders differently on
   every machine. Now a drawn SVG path at the same stroke weight.
2. **Fixed** — opening `/sales` on a phone panned the whole page sideways: the
   ledger table laid out at 730px inside a 390px viewport. The table now scrolls
   inside its own frame, and the nav bar no longer overflows.
3. **Fixed** — `F2` was printed on the band button and bound to focus rather
   than to recording. It records now, and only places the caret when the sheet
   holds nothing recordable.
4. **Fixed** — the product list opened itself on arrival, unrolling eight
   options over the sheet's content the moment the screen was reached. It opens
   on input now.
5. **Fixed** — DESIGN.md asserted crimson appears in four places; the code uses
   it in seventeen, across two coherent roles (commitment and attention). The
   document was written from the intention rather than from the code; both it
   and the sidecar now describe what the build does.

## keep

The stamp is the system, not a decoration on it: its ink registration, its
reduced-motion fallback, its refusal to animate a whole ledger at once, and the
rule that a cancelled document is ink and struck through while crimson belongs
to what has been confirmed.

---

## Claim checks

| Claim | Result | Evidence |
|---|---|---|
| One line system, no texture layer | CONFIRMED | `--rule` / `--rule-strong` are the only line tokens; 20 + 16 uses across sheet and table; 0 `background-image` in `src/` |
| Crimson only in four places | REFUTED → documented honestly | 17 uses, two roles; DESIGN.md corrected |
| Annulled is ink and struck through | CONFIRMED | `color: var(--ink)` plus a 2px `::before` rule |
| Every amount tabular | CONFIRMED | `app.css` primitives plus `Ledger`, `SaleDraft`, and the figure inputs |
| Every printed shortcut is wired | CONFIRMED | 6 handlers; `F2` records, `Ctrl/Cmd+Enter` records, `Enter` picks |
| Exactly one `<h1>` per page | CONFIRMED | measured on both routes |
| Browser surfaces themed | CONFIRMED | `::selection`, `caret-color`, `scrollbar-color`, `:focus-visible` in `app.css` |
| Fonts self-hosted, no third-party host | CONFIRMED | 3 `@font-face` over `/static/fonts`, 0 references to `fonts.googleapis` or `fonts.gstatic` |

## Floor

No kickers or eyebrows, no hard offset shadows, no gradient text, no coloured
side stripe above 1px, no monospace as costume, no nested cards, no hero-metric
template, no zero-offset halo. The one glyph standing in for an icon is fixed
above.

## Note on evidence hygiene

A verification subagent was given this review and failed after 167 turns,
leaving ~80 minutes and 16.2M tokens spent with no output. Diagnosis: the prompt
asked a reviewer to *hunt* rather than to *score*, so it wrote its own probes,
mutated the live database (three synthetic `demo-cafe-1kg` sales), and exhausted
itself. The review above was performed inline instead, and its claim checks are
reproducible greps rather than a reviewer's recollection.

The three probe sales it created remain in the development database. They are
**not** inert: each wrote a `stock_movements` row, and that table is append-only
by trigger, so deleting them means disabling the custody guard this review
itself keeps ("keep" above). The product rows were untouched. What the three
probes actually cost is the demo: `/sales` and the catalog carry three $4.500
sale documents and one synthetic product that nobody recognizes.

A fourth `demo-cafe-1kg` sale predates the subagent (2026-10-13, 250 × 4600 =
$11.500) and is the owner's own data, not a probe. Any cleanup that keys on the
product name hits it too.
