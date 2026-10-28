# Design system — Tempox

Synced from the project's real sources on 2026-10-26. Every value below names the
file it was read from; nothing here is a proposal. Where the built code already
disagrees with itself, the disagreement is recorded as a deviation instead of
being smoothed over — see "Deviations found" at the end.

Source files read:

- `frontend/src/app.css` — the token layer (`:root`), `@font-face` declarations, shared primitives (`.figures`, `.label`, `.control`, `.panel`)
- `frontend/src/routes/+layout.svelte` — the shell (rail, tabs, language toggle)
- `frontend/src/routes/sales/+page.svelte` — the composition of `/sales`
- `frontend/src/lib/components/Stamp.svelte` — document state
- `frontend/src/lib/components/{SaleDraft,Ledger,PurchaseLedger,Catalog,ProductPicker,CollectionForm,AccountPanel}.svelte` — component-level values
- `frontend/src/routes/accounts/+page.svelte` — the v3 screen
- `DESIGN.md`, `PRODUCT.md` — the committed visual world and the product constraints

## Colors

| Token | Value | Usage | Source |
| --- | --- | --- | --- |
| `--ground` | `#fbfcfd` | page background | `app.css:85` |
| `--surface` | `#ffffff` | work panels, control surfaces | `app.css:86` |
| `--rule` | `#e8ecf1` | panel borders, row separators, table rules | `app.css:87` |
| `--rule-strong` | `#c3ccd6` | control borders, scrollbar thumb | `app.css:88` |
| `--ink` | `#12212e` | primary text, confirmed state, primary action fill | `app.css:89` |
| `--ink-soft` | `#5f6f7d` | secondary text, labels, placeholders (5.04:1 on `--ground`) | `app.css:90` |
| `--accent` | `#0b5f8a` | running balance, active destination, focus ring, caret | `app.css:91` |
| `--state-void` | `#a33227` | annulled / refunded documents | `app.css:96` |
| `--state-pending` | `#8a6410` | pending documents | `app.css:97` |

Derived (not tokens, but the only values found):

- Primary action hover: `#0a1622` — `SaleDraft.svelte`, `CollectionForm.svelte`, `Catalog.svelte`
- `::selection` background: `accent` at 16% — `app.css`
- `.control:disabled` background: `--ground` — `app.css`

## Typography

Faces are served from the project's own files via `@fontsource` (local woff2, latin
subset); no third-party font host.

| Face | Family stack | Weights used | Source |
| --- | --- | --- | --- |
| Figures / labels / headings | `'Plex Condensed', ui-sans-serif, system-ui, sans-serif` | 400, 500, 600, 700 | `app.css:101` |
| Prose | `'Plex Text', ui-sans-serif, system-ui, -apple-system, sans-serif` | 400, 500, 600 | `app.css:102` |

Scale (`app.css:104-110`, declared against a 16px root):

| Token | Size |
| --- | --- |
| `--step--1` | `0.75rem` |
| `--step-0` | `0.8125rem` |
| `--step-1` | `0.875rem` (body) |
| `--step-2` | `1rem` |
| `--step-3` | `1.25rem` |
| `--step-4` | `1.75rem` (page title) |
| `--step-5` | `2.375rem` |

Rules observed in the code:

- Money and quantities are set in the figure face with `font-variant-numeric: tabular-nums` and `font-feature-settings: 'tnum' 1` — the `.figures` primitive, `app.css`.
- Labels are condensed, 600, uppercase, tracked. The observed tracking values cluster at `0.12em` (9 uses) and `0.1em` (10 uses); `app.css` `.label` uses `0.12em`.
- Body line-height is `1.45` (`app.css`); page headings use `1.1` (`sales/+page.svelte`).

## Spacing

| Value | Usage | Source |
| --- | --- | --- |
| `24px` (`--panel-gap`) | gap between the two work panels of `/sales` | `app.css:116` |
| `88px` (`--rail-w`) | navigation rail width | `app.css:117` |
| `22px 26px 16px 28px` | `/sales` page header | `sales/+page.svelte` |
| `0 26px 26px 28px` | `/sales` workspace insets | `sales/+page.svelte` |
| `16px 20px 20px` | action footer inside the draft panel | `sales/+page.svelte` |
| `10px 11px` | `.control` padding | `app.css` |
| `12px 18px` | primary action padding | `sales/+page.svelte` |

## Radius

| Value | Usage | Source |
| --- | --- | --- |
| `2px` (`--radius`) | every control, primary action, focus outline | `app.css:112` |
| `8px` (`--panel-radius`) | work panels, clipped contents | `app.css:115` |
| `4px`, one side only | an element joining a neighbour (table foot under a header strip) | `Ledger.svelte` (`0 0 4px 4px`, `4px 4px 0 0`) |
| `99px` | scrollbar thumb only | `app.css` |

## Shadows

The system is flat for work areas: panels carry a 1px border, never a shadow. Two
shadows exist, both on floating layers only:

| Value | Usage | Source |
| --- | --- | --- |
| `0 14px 30px -14px rgb(21 22 28 / 0.45)` | product picker dropdown | `ProductPicker.svelte` |
| `-8px 0 24px rgb(18 33 46 / 8%)` | account panel drawer | `AccountPanel.svelte` |

## Component rules

### Work panel

`background: var(--surface)`, `border: 1px solid var(--rule)`,
`border-radius: var(--panel-radius)`, `overflow: hidden`,
`min-width: 0` — the `.panel` primitive, `app.css`. Separation between two panels
is the border plus `--panel-gap`, never whitespace alone (`sales/+page.svelte`).

### Field

`background: var(--surface)`, `border: 1px solid var(--rule-strong)`,
`border-radius: var(--radius)`, `padding: 10px 11px`,
`font-size: var(--step-2)` — `.control`, `app.css`.
Placeholder color is `--ink-soft`, not a lighter grey. Disabled state is
`--ground` background with `--ink-soft` text.

### Focus

`outline: 2px solid var(--accent)`, `outline-offset: 2px`, `border-radius: 2px`
on `:focus-visible`; caret color is `--accent` — `app.css`.

### Primary action

Ink fill, white text, `2px` radius, `12px 18px` padding, condensed 600 uppercase
at `--step-2`, full width of the draft it closes. Disabled: white surface,
`--ink-soft` text, `1px dashed var(--rule-strong)`, with an adjacent hint
explaining why. Never the accent as fill — `sales/+page.svelte`.

### Navigation

Transparent rail against the ground, separated from content by a `1px` `--rule`
right border. Active destination: `--accent` text plus a `3px` `--accent` left
rule; at ≤860px the rail becomes a horizontal bar and the marker moves to a
`2px` bottom rule. Not a filled block — `+layout.svelte`.

### Document state

Condensed, 600, uppercase, `0.1em` tracking, preceded by a `2px × 12px` tick in
`currentColor`. Confirmed is `--ink`; annulled is `--state-void`; pending is
`--state-pending`. Never a filled pill — `Stamp.svelte`.

### Ledger row

Right-aligned running balance in `--accent` with tabular figures, in the row's
first column, with a right boundary separating the continuous reading column.
Annulled or fully refunded rows strike through the amount and the inherited
balance. Row separators are `1px` `--rule` — `Ledger.svelte`, `DESIGN.md`.

### Scroll region

Scrollbars are always visible, `thin`, thumb `--rule-strong` on `--rule` track,
`10px` wide, with a `2px` track-colored border and `99px` radius — `app.css`.
A scroll region the user cannot see is a scroll region they do not know exists.

## Deviations found

These are real and worth deciding on; the sync records them, it does not fix them.

1. **Undefined custom properties on `/accounts`.** `accounts/+page.svelte` asks for
   `var(--figure)` (4 uses), `var(--rounded-control)` (4 uses) and
   `var(--rounded-panel)` (1 use). None of the three is declared anywhere in
   `app.css`. The result is that the v3 screen's figures render in the prose face
   and its controls and panel have square corners, while every other screen uses
   condensed figures and a 2px/8px radius. The correct tokens are
   `--font-figure`, `--radius` and `--panel-radius`.
2. **Old world values still live as raw `rgb()` literals.** `DESIGN.md` forbids
   reintroducing `--paper`, `--indigo`, `--crimson`. The names are gone, but their
   values survive un-tokenized: `rgb(43 49 98 / …)` (`#2b3162`, indigo) in
   `Catalog.svelte`, `SaleDraft.svelte` and `Ledger.svelte`;
   `rgb(168 27 42 / …)` (`#a81b2a`, crimson) in the same three plus
   `ProductPicker.svelte`; `#f2efe6` light-on-dark text in `ProductPicker.svelte`;
   `#084a6c` in `Catalog.svelte`; `#f2f6fa` row fills in `Ledger.svelte` and
   `PurchaseLedger.svelte`. That is the drift the DESIGN.md rule was written to
   prevent, one layer down.
3. **Row tints are not expressible as tokens today.** The low-alpha indigo and
   crimson tints above have no declared equivalent, so any surface that wants a
   tinted row cannot ask for one by name.
