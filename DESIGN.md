---
name: Tempox
description: A computed account statement for the shop's working day.
colors:
  ground: "#fbfcfd"
  surface: "#ffffff"
  rule: "#e8ecf1"
  rule-strong: "#c3ccd6"
  ink: "#12212e"
  ink-soft: "#5f6f7d"
  accent: "#0b5f8a"
  state-void: "#a33227"
  state-pending: "#8a6410"
typography:
  figure:
    fontFamily: "Plex Condensed, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.45
    fontFeature: "'tnum' 1"
  step--1:
    fontSize: "0.75rem"
  step-0:
    fontSize: "0.8125rem"
  step-1:
    fontSize: "0.875rem"
  step-2:
    fontSize: "1rem"
  step-3:
    fontSize: "1.25rem"
  step-4:
    fontSize: "1.75rem"
  step-5:
    fontSize: "2.375rem"
  body:
    fontFamily: "Plex Text, ui-sans-serif, system-ui, -apple-system, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.45
rounded:
  control: "2px"
  panel: "8px"
spacing:
  panel-gap: "24px"
  rail-width: "88px"
components:
  primary-action:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.surface}"
    typography: "{typography.figure}"
    rounded: "{rounded.control}"
    padding: "12px 18px"
  text-field:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "10px 11px"
  nav-active:
    textColor: "{colors.accent}"
    rounded: "{rounded.control}"
  ledger-balance:
    textColor: "{colors.accent}"
    typography: "{typography.figure}"
    padding: "10px 16px"
  work-panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.panel}"
---

# Design System: Tempox

## Overview

**Creative North Star: "The Computed Account Statement"**

The printed account statement is the money document the shop already trusts. The day reads as one: every row is a computed figure and every balance is recomputed. This refuses the category default of a CRUD screen where state is a coloured pill nobody reads.

The signature is a running-balance column in the ledger's left margin, recomputed per row so it reads as one continuous number down the page. Work areas are contained in closed, bordered panels: their own edges establish separation, rather than whitespace alone. The sales surface and shell are documented here; `/products` and `/purchases` have not yet been redesigned and remain outside this system's implemented scope.

**Key Characteristics:**
- A cold near-white ground, dark ink, quiet grey and a restrained blue accent.
- Computed money figures, with a continuous running balance as the ledger's signature.
- Closed, white work panels separated by their own border and a 24px gap.
- State is readable text with a small colour tick, not a filled badge.

## Colors

Near-white surfaces and cool rules let ink carry the reading; blue is reserved for the running balance and active navigation, while red and amber communicate exceptional document states.

### Primary
- **Statement Blue** (`#0b5f8a`): the running-balance figures and active navigation marker; it is a signal, not a general decorative fill.

### Neutral
- **Cold Ground** (`#fbfcfd`): the page background.
- **White Surface** (`#ffffff`): the work panels and control surfaces.
- **Fine Rule** (`#e8ecf1`): the quiet border and separator for contained work areas and calculated rows.
- **Strong Rule** (`#c3ccd6`): the stronger edge for controls and structural boundaries.
- **Ledger Ink** (`#12212e`): primary text, confirmed state and the primary action.
- **Soft Ink** (`#5f6f7d`): secondary copy and supporting information.

### Named Rules
**The Balance Owns the Accent Rule.** Keep the blue accent for the running-balance column and active destination. Focus and caret affordances also use the accent in the shared stylesheet; do not turn it into a general-purpose section colour or filled action colour.

**The State Is Readable Rule.** State is text plus a 2px tick. Confirmed stays in ink; annulled or refunded uses the void state colour; pending uses the pending state colour. Never encode a state only by colour.

## Typography

**Display Font:** Plex Condensed (IBM Plex Sans Condensed; `ui-sans-serif, system-ui, sans-serif` fallback)
**Body Font:** Plex Text (IBM Plex Sans; `ui-sans-serif, system-ui, -apple-system, sans-serif` fallback)

**Character:** Condensed Plex carries figures, labels and headings; Plex Text carries prose. Amounts use tabular numerals so their columns stay aligned.

### Hierarchy
- **Figures** (600, 0.75rem–2.375rem across the implemented scale, line-height 1.45 at the base): amounts and other values that need to align; use tabular numerals.
- **Headings** (600, 1.25rem–1.75rem, line-height 1.1–1.15): page and work-panel titles, set in Plex Condensed.
- **Body** (400, 0.875rem, line-height 1.45): prose, hints and table content.
- **Labels** (600, 0.75rem–0.8125rem, tracked and uppercase): compact captions and document-state text.

**The Tabular Figures Rule.** Set money figures in Plex Condensed with tabular numerals; their aligned columns are part of how the statement is read.

## Layout

The shell uses an 88px navigation rail as the page's margin, with the work area beside it. On the sales surface, a heading and document count sit above two work panels: the draft gets the wider column, and the day's ledger is a bounded reading column. The panels have a 24px gap and each has its own closed edge. Desktop work fits the viewport; at narrower widths the panels stack and the page scrolls. At 860px and below, the navigation becomes a horizontal bar while remaining visible.

Within the panels, fixed headings and controls frame the content that needs to scroll. The ledger scrolls within its own frame; its running balance sits at the left of each row. The primary action remains at the foot of the draft panel, outside its scrolling middle.

## Elevation & Depth

The system is flat: no shadow vocabulary is used to lift work areas. Depth and separation come from white surfaces against the near-white ground and from the 1px edge around each closed panel. The border is the separation, not empty air.

## Shapes

Controls use restrained 2px corners. Work panels use an 8px radius and clip their contents with `overflow: hidden`. Rules stay thin and functional: they mark panel edges, calculated rows and boundaries rather than decorating the page.

## Components

### Primary Action
The primary action is a solid ink bar at the foot of the draft it closes, not a page-wide band.
- **Shape:** restrained corners (2px).
- **Color:** ink fill with white text; never the blue accent.
- **Size:** full available width in the sales draft, with 12px 18px padding.
- **Disabled:** white surface, soft-ink text and a dashed strong-rule outline, with an adjacent hint explaining why it is not ready.

### Work Panels
Each work area is a closed surface with a visible edge.
- **Shape:** 8px radius, clipped contents.
- **Background:** white surface.
- **Border:** 1px fine rule.
- **Separation:** 24px panel gap in the sales workspace.

### Navigation
The rail is the page's margin, not a filled slab.
- **Desktop:** 88px wide, transparent against the ground, separated from content by a 1px fine rule.
- **Active:** blue text and a blue left rule; it is not a filled block. At narrow widths, the rail becomes a horizontal bar and the active marker moves to its bottom edge.

### Ledger Row and Balance
Rows are document figures, not generic CRUD records.
- **Signature:** a recomputed, right-aligned running balance in the ledger's left margin, set in blue and tabular figures.
- **Rows:** fine-rule separators; the balance column has a right boundary to distinguish its continuous reading column.
- **Annulled or fully refunded:** the row's amount and inherited balance are struck through; a cancellation does not add money to the running balance.

### Document State
- **Shape:** uppercase condensed text beside a 2px tick; no filled background or stamp silhouette.
- **Confirmed:** ink.
- **Annulled or refunded:** void-state red.
- **Pending:** pending-state amber.

### Fields
- **Style:** white surface, 1px strong-rule border, 2px radius, 10px 11px padding.
- **Focus:** visible 2px accent outline with a 2px offset; the caret also uses the accent.
- **Figure fields:** condensed, tabular and right-aligned.

## Do's and Don'ts

### Do:
- **Do** recompute and show a running balance beside each ledger row.
- **Do** use the blue accent for the balance and active destination, and preserve its observed use for focus and caret affordances.
- **Do** keep work areas closed by their own white surface, fine-rule border and 8px radius, with the observed 24px gap between sales panels.
- **Do** render document state as legible text plus a 2px tick; keep confirmed state quiet in ink.
- **Do** use the solid ink bar for the primary action.
- **Do** strike through a cancelled or fully refunded row's amount and inherited balance.

### Don't:
- **Don't** use a filled state pill or a rotated rubber stamp.
- **Don't** make the primary action a full-bleed accent band or an accent-filled button.
- **Don't** replace the panel's separating border with whitespace alone.
- **Don't** use a name from the transitional token alias block for a new component. It is temporary migration debt kept so the two remaining legacy consumers render; remove each alias when its last legacy consumer is rewritten.
