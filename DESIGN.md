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
  joined: "4px"
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

The signature is a running-balance column in the ledger's left margin, recomputed per row so it reads as one continuous number down the page. Work areas are contained in closed, bordered panels: their own edges establish separation, rather than whitespace alone. The shell and `/sales` are documented here in detail; `/purchases` and `/products` are built in the same world, and the structure, content and states each of them resolves are recorded in its own surface brief.

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

The shell uses an 88px navigation rail as the page's margin, with the work area beside it. On the sales surface the page is divided by a single rule into a **pad** and a **day**: the slip being written is the left column at a fixed 620px, and the day's stack of documents takes the rest, because a stack reads better wide than a form does. Desktop work fits the viewport; at 1120px and below the columns stack and the page scrolls. At 860px and below, the navigation becomes a horizontal bar while remaining visible.

The pad is one closed surface holding the slip and, at its foot, the page's single next action — outside the slip's scrolling middle, so the action is never the thing you scroll to find. The day scrolls its own stack of documents within the column.

**The Slip Composition Rule.** The day is not a table. Each document is a slip: a head carrying its number, time and the balance it inherits, and a body carrying the customer, the method, its state word and its total. The draft is the same artifact, drawn before it is printed. Nothing in this composition shares a column with anything else, which is what lets a long product name wrap without pushing every other document out of alignment. The unit price and quantity still align, because they are figures on one line, but that alignment stops at the slip's edge.

**The Balance Lives in the Document Rule.** The running balance is still recomputed oldest-first and still the statement's signature, but it is now printed in each document's own head rather than in a column that spans the page. A document that contributes nothing still shows the balance it inherits, struck through, so the number never breaks.

## Elevation & Depth

The system is flat: no shadow vocabulary is used to lift work areas. Depth and separation come from white surfaces against the near-white ground and from the 1px edge around each closed panel. The border is the separation, not empty air.

## Shapes

Controls use restrained 2px corners. Work panels use an 8px radius and clip their contents with `overflow: hidden`. Rules stay thin and functional: they mark panel edges, calculated rows and boundaries rather than decorating the page.

**The Joined Edge Rule.** A 4px radius belongs to an element that meets another edge rather than standing alone: the table frame whose bottom corners continue a header strip, and the loading and empty states that sit inside the ledger's frame. It is not a third control size — a control is still 2px and a panel is still 8px. When an element joins a neighbour on one side and closes on the other, it takes 4px on the closed side only, never a full 4px box.

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

Rows are documents, not generic CRUD records, and each is drawn as a slip rather than as a table row.

- **Signature:** a recomputed, right-aligned running balance in each slip's head, set in blue and tabular figures.
- **Slip:** a closed surface with an 8px radius; its head sits on the ground with a fine-rule lower edge, and the body stays on white.
- **Opens in place:** the whole slip is the control — a keyboard-reachable element with `aria-expanded` and `aria-controls` — and its lines, credit notes and refund form open beneath it.
- **Annulled or fully refunded:** the slip's total and inherited balance are struck through; a cancellation does not add money to the running balance.
- **Never a table:** a column that spans the page would force every document into one row's width, and the document is the thing being read.

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
- **Don't** reintroduce a name from the previous world's palette. The same rule stands, now with nothing left to migrate: `--paper`, `--indigo`, `--crimson` and `--tab-overhang` resolved through a transitional alias block for as long as the last five components still named them, and that block is deleted — every surface now asks for `--ground`, `--surface`, `--ink`, `--ink-soft`, `--accent` or a state tone by name. If you are looking at a diff that adds one of the old names, it is a regression, not a shortcut.
