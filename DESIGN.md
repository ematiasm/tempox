---
name: Tempox
description: A shop's sales ledger — every event wears its state on its face.
colors:
  crimson: "#a81b2a"
  crimson-deep: "#7e1420"
  indigo: "#2b3162"
  indigo-soft: "#4b5182"
  paper: "#f6f2e9"
  paper-raised: "#fbf9f3"
  paper-sunk: "#efeade"
  ink: "#15161c"
  ink-soft: "#5b5e6e"
typography:
  figure:
    fontFamily: "Fira Sans Condensed, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.45
    fontFeature: "'tnum' 1"
  display:
    fontFamily: "Fira Sans Condensed, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.75rem"
    fontWeight: 600
    lineHeight: 1.1
  body:
    fontFamily: "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.45
  label:
    fontFamily: "Fira Sans Condensed, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 600
    letterSpacing: "0.1em"
rounded:
  sm: "2px"
  md: "3px"
  lg: "4px"
spacing:
  xs: "6px"
  sm: "10px"
  md: "16px"
  lg: "22px"
  xl: "26px"
components:
  band:
    backgroundColor: "{colors.crimson}"
    textColor: "#ffffff"
    padding: "14px 26px 14px 28px"
  band-action:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: "12px 18px"
  record:
    backgroundColor: "{colors.crimson}"
    textColor: "#ffffff"
    rounded: "{rounded.md}"
    padding: "11px 18px"
  input:
    backgroundColor: "#ffffff"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    padding: "10px 11px"
    height: "40px"
  nav-tab-active:
    backgroundColor: "{colors.crimson}"
    textColor: "#ffffff"
    padding: "20px 0 26px"
  ledger-row:
    backgroundColor: "{colors.paper-raised}"
    textColor: "{colors.ink}"
    padding: "10px 12px"
---

# Design System: Tempox

## Overview

**Creative North Star: "The Stamped Ledger"**

Tempox is a shop's sales book, and the world is the one a bookkeeper already
knows: paper, hairline rules, and ink. The organising idea is that **every event
wears its state on its face** — a sale is a document that has been stamped, not
a row carrying a status badge. That is the whole thesis, and it refuses the
category default it was built against: the generic admin CRUD where state is a
coloured pill nobody reads.

The world lends exactly four things and refuses to lend a fifth: type, palette,
density, and one signature move. It never supplies the layout, the navigation
model, or the controls. Every control here is the ordinary thing a web
application already uses — a text field, a select, a table, a button — because
the person behind the counter needs to know where everything is *before* they
notice the world. A screen that reads as a costume of the ledger would be a
failure of this system, not a success of it.

Density is high and quiet. This is a working surface seen all day under office
light at a fixed PC in a shop, so the ground is paper, not a dark theme chosen
because the category likes dark. Expression is spent in one place: the stamp.
Everything else is clean, aligned, and deliberately unremarkable, because a
screen with one strong idea needs the rest of itself to stay out of the way.

**Key Characteristics:**
- Paper ground, hairline rules, ink — no second layer, no texture behind content
- One saturated accent (crimson) spent on one action band and the stamp
- Condensed humanist for every figure and heading; tabular numerals everywhere
- One authored motion: the stamp landing
- Standard web controls; the world never re-skins a familiar affordance

## Colors

A restrained strategy with one committed accent: tinted neutrals carry the
whole surface, and crimson is spent where the shop's attention actually matters.

### Primary
- **Stamp Crimson** (`#a81b2a`): the system's only accent, carrying two related
  jobs. **Commitment**: the confirmed stamp, the action band, the record and
  create buttons, the active destination, and the focus ring and caret.
  **Attention**: an error's heading, an invalid field, a shelf running low, the
  regalo marker on a bonus line, and the destructive hover on a remove control.
  Both jobs mean "look here"; the colour is never decoration, never a
  background wash, and never a section marker.
- **Crimson Deep** (`#7e1420`): the shadow under the record button, and error
  text that needs to sit darker than the accent itself.

### Secondary
- **Ledger Indigo** (`#2b3162`): the navigation rail, the cash-on-hand ground,
  the active date-scope segment. Structure, not attention.
- **Indigo Soft** (`#4b5182`): field labels, scrollbar thumb, and the
  secondary text that sits on paper.

### Neutral
- **Notebook Paper** (`#f6f2e9`): the page ground, and the ground of every
  nested document. The base of the world.
- **Raised Paper** (`#fbf9f3`): sheets and cards that sit *on* the paper — the
  draft sale sheet, the catalog, a ledger row's own lines.
- **Sunk Paper** (`#efeade`): the live-total foot, the cash footing, disabled
  fields. Used where a surface should read as pressed into the page.
- **Ink** (`#15161c`): body text, the annulled stamp, the strikethrough rule.
- **Soft Ink** (`#5b5e6e`): secondary text, hints, placeholders.

### Named Rules
**The One Line System Rule.** `--rule` and `--rule-strong` are the only line
values in the system. The sheet's rules and the table's borders consume the same
two tokens, because the paper's rules *are* the table's grid — never a texture
painted behind a table. Two line systems would turn the world into a costume.

**The Crimson Is Spent Rule.** Crimson is the system's only accent and it does
one kind of work: it marks what the shop should look at. Commitment — the
stamp, the action band, the record button, the active destination, the focus
ring. Attention — an error, an invalid field, a low shelf, a giveaway. Nothing
else: it is never a background wash, never a decorative rule, never a section
marker. A page that adds crimson in a third role has spent it.

**The Annulled Is Ink Rule.** A cancelled document is indigo-black and struck
through, never crimson. An annulment is a correction, not an alarm; colouring it
crimson would make a cancelled sale look like the worst thing on the page and
would spend the accent that belongs to the confirmed stamp.

The rule reaches the control that causes one. **Annul sale** is an ink control
like every other: crimson is what the *hover* answers with, exactly as the draft
sheet's remove control behaves. A filled crimson button would be a third role
for the system's one accent — a commitment role spent on a correction.

**The Reversal Is a Second Document Rule.** An annulled sale is not a state the
sale carries. Reversing one emits a credit note: a document of its own, listed
under the sale it corrects, with its own total and lines. The sale keeps its
`confirmed` stamp; it wears `Anulado` only when every line has come back, and
that is derived from its notes rather than written onto it. A partial refund
therefore reads as what it is — a sale that still stands, with something
returned.

## Typography

**Display Font:** Fira Sans Condensed (with `ui-sans-serif, system-ui, sans-serif`)
**Body Font:** the system sans (`ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, sans-serif`)

**Character:** a condensed humanist carries every figure, heading and label —
compressed, workmanlike, slightly narrow, the voice of a printed form. The
system sans carries prose and does it invisibly. The pairing reads as "a
document with someone's own handwriting on the numbers", not as an editorial
statement.

Display and figure faces are self-hosted from the project's own files
(`/fonts/fira-sans-condensed-{400,600,700}.woff2`, latin subset). A shop PC on a
bad connection should not wait on a third-party host, and a hosted font
stylesheet is an undeclared dependency.

### Hierarchy
- **Display** (600, 1.75rem / 28px, line-height 1.1): page headings inside the
  action band and the ledger's own heading. Never larger.
- **Title** (600, 1.25rem / 20px): sheet headings, catalog heading, line totals
  in the draft table.
- **Body** (400, 0.875rem / 14px): prose, table cells, hints. Measure stays
  inside 65–75ch where a paragraph exists.
- **Figure** (600, 1rem–2.375rem, `font-variant-numeric: tabular-nums`): every
  amount, every quantity, every date. Tabular is not optional — a total that
  reflows while the cashier types a digit is a total nobody can trust.
- **Label** (600, 0.8125–0.875rem, letter-spacing 0.1em, uppercase): field
  labels and table headers. Uppercase and tracked, so they read as a form's
  printed captions rather than as content.

### Named Rules
**The Tabular Rule.** Every number a user reads or types — money, quantities,
times, dates, counts — is set in the figure face with tabular numerals. No
exceptions, including inside inputs.

**The Label Is Uppercase Rule.** A field label is uppercase and tracked, and
therefore its accessible name is compared case-insensitively; a test that
demands one case is asserting the stylesheet, not the design.

## Layout

A two-column grid: a fixed 92px navigation rail and a fluid work area. The rail
carries the product name, the three destinations as vertical tabs — sell, buy,
look after the catalogue — and the language toggle at its foot. The active tab is *extended* — it is 104px wide and
overhangs 12px into the work area, which is what makes it read as a pulled-up
index tab rather than as a second column.

Below the rail, every page opens with a full-bleed crimson band: the page's
heading and its single next action. The band is the one saturated region, and it
is always the same shape, so the eye learns it once. **This rule is asserted, not
just declared**: `shell.spec.ts` compares the computed background of
`header.band` across every page and fails if one of them chooses another colour.
It exists because it happened — the purchases screen shipped with an indigo band,
on the reasoning that buying mirrors selling, and the behavioural tests could not
see it.

Content below the band is a single fluid column at 22–28px inset. Regions are
separated by generous whitespace rather than by boxes where the content does not
need a container; where a container is real (the draft sheet, the catalog), it
gets a raised-paper fill and a hairline border.

**Breakpoints.** Two, both about staying usable rather than being designed for:
at **1180px** multi-column work areas stack (the ledger's footings go to two
columns, the catalog's create form to two); at **860px** the rail becomes a
horizontal bar. The navigation never hides behind a gesture — a shop employee
should never have to discover a hamburger.

**Density.** Tight inside a group, generous between groups, and always more
space above a heading than below it. Table rows sit at 10px vertical padding;
sheet internals at 15–18px; regions are separated by 22–24px.

## Elevation & Depth

Tonal layering first, shadows second — and only two shadows exist in the whole
system.

Depth is carried by the paper stack: `--paper` is the page, `--paper-raised`
sits on it, `--paper-sunk` is pressed into it. That is how a reader tells the
draft sheet from the page it lies on, and how the live-total foot reads as the
bottom of a form.

### Shadow Vocabulary
- **Action lift** (`box-shadow: 0 2px 0 rgb(0 0 0 / 0.18), 0 8px 18px -10px rgb(0 0 0 / 0.55)`):
  the band's action button only. A real offset plus a soft blur, so the button
  reads as a physical key on a bright field.
- **Floating list** (`box-shadow: 0 14px 30px -14px rgb(21 22 28 / 0.45)`): the
  product picker's match list, and nothing else. It floats over content, so it
  is the one thing allowed to cast a long shadow.
- **Record key** (`box-shadow: 0 2px 0 #7e1420`): the record button's own
  crimson shadow, which disappears when the button is disabled.

### Named Rules
**The Flat-By-Default Rule.** Surfaces are flat at rest. Nothing in this system
carries a decorative shadow; the three above are the complete vocabulary, and
each one encodes a physical relationship (a key, a floating sheet, a pressed
button).

## Shapes

A 3px radius (`--radius`) on controls and containers, 2px on small chips and
keyboard hints, 4px on sheets. Corners are slightly rounded, never pill-shaped
and never square — the radius of a printed form's ruled box, not a consumer
app's button.

Borders are hairlines of the single line system. There is no thick border
anywhere, no coloured side stripe on a card or a row, and no zero-offset glow.
The one exception in value (not in kind) is the ledger table's left margin rule,
which carries a 1px crimson hairline at 50% alpha — the printed margin of a
ruled page, and still a hairline of the same system.

The stamp is the system's one deliberate irregularity: a 2px rectangular border,
rotated −6.5° off the baseline, with `mix-blend-mode: multiply` so its ink bites
into the paper instead of floating above it. It is the only element in the
system permitted to sit off-axis.

## Components

### The Stamp (signature)
- **Shape:** 2px border, `border-radius: 2px`, rotated −6.5°, `mix-blend-mode: multiply`
- **Confirmed:** crimson text and border, uppercase, letter-spacing 0.13em. Three
  sizes: `cell` (13px, in a ledger row), `inline` (14px), `hero` (28px, after a
  sale is recorded).
- **Annulled:** ink, struck through by a `::before` rule rotated −1.5°. Never crimson.
- **Registration:** a duplicate impression of the same word, offset a hair and
  at 28% opacity, so the stamp reads as rubber on paper.
- **Motion:** `stamp-land` — 420ms `cubic-bezier(0.16, 1, 0.3, 1)`, from
  `scale(1.42)` and `opacity: 0` to settled. It runs **only** on the document
  that was just recorded; a ledger of forty documents must not perform forty
  presses at once. Disabled under `prefers-reduced-motion`.

### Buttons
- **Shape:** 3px radius
- **Band action:** raised paper on the crimson band, 20px condensed 600,
  `padding: 12px 18px`, with the lift shadow above. It carries its keyboard
  shortcut in a `<kbd>`, and that shortcut is wired.
- **Record:** crimson on paper, white text, `padding: 11px 18px`, sat on a
  crimson-deep shadow. Disabled state drops to sunk paper with no shadow.
- **Ghost:** 1px dashed rule, indigo text, transparent fill. For secondary
  actions that must not compete — *Clear*, *New customer*, *Create the first
  product*.
- **Hover:** background shifts, never a scale or a translate, except the
  band action which presses down 1px.
- **Focus:** a 2px crimson ring at 2px offset, from `:focus-visible`. The same
  ring everywhere in the system.

### Inputs / Fields
- **Style:** white fill, 1px `--rule-strong` border, 3px radius, 16px text.
- **Focus:** the crimson focus ring; the caret is crimson too.
- **Invalid:** the field itself turns crimson-bordered on a subtle crimson wash.
  A field holding something that is not a number says so *on the field*, not in
  a message somewhere else on the page.
- **Figure inputs:** quantity and price fields are set in the figure face,
  right-aligned, tabular. What the cashier types looks like what they will read.

### Navigation
- **Style:** a 92px indigo rail. Destinations are vertical (writing-mode:
  vertical-rl) 16px condensed uppercase tabs.
- **Default / hover / active:** muted warm white at 80%, full white on hover,
  and the active tab is a crimson block that extends past the rail.
- **Mobile (≤860px):** the rail becomes a horizontal bar; the tabs become
  horizontal and the extension is dropped.

### Cards / Containers
- **Corner style:** 4px on sheets, 3px on inner elements, and the ledger table
  is rounded only at its foot so the legend above it reads as its head.
- **Background:** `--paper-raised` on the page's `--paper`.
- **Border:** 1px hairline. Never a shadow, never a thick edge, never a stripe.
- **Padding:** 14–18px inside a region; 10–12px inside a table row.

### Tables
- **Headers:** uppercase 14px condensed labels at 0.1em tracking, over a
  `--rule-strong` bottom border.
- **Rows:** hairline `--rule` separators, 10px vertical padding. Hover is a 3.5%
  indigo wash — enough to track the row with a mouse, too little to shout.
- **Nested lines:** a document's own lines sit in a `--paper` table inside the
  raised row, so the document reads as a thing containing things.

## Do's and Don'ts

### Do:
- **Do** set every number in the figure face with tabular numerals.
- **Do** reserve crimson for attention: commitment (stamp, action, focus) and
  alert (error, invalid field, low stock, regalo). Never a wash, a rule, or a
  section marker.
- **Do** render a `g` or `ml` product's unit verbatim: `1,500 g` at `$0.01/g`,
  never converted to kilograms or litres.
- **Do** strike through an annulled document in ink, and its total with it.
- **Do** show honest empty states that name the next action (*No products yet —
  create the first one*), and never invent sample rows.
- **Do** keep the single line system: the paper's rules and the table's grid are
  the same two tokens.
- **Do** wire every shortcut you print on a control.
- **Do** theme the browser's own surfaces (selection, caret, scrollbar, focus,
  underline offset) from the palette.

### Don't:
- **Don't** put a texture, gradient, or second rule layer behind content.
  The paper's rules are the table's grid, not a background image.
- **Don't** use a coloured `border-left` or `border-right` above 1px as a card or
  row accent.
- **Don't** use a hard offset shadow (`box-shadow: 4px 4px 0`). The three
  shadows documented above are the entire catalogue.
- **Don't** use glyphs or emoji as icons, and don't set the system sans as a
  display voice — that is what the condensed face is for.
- **Don't** use gradient text, glass, blur, or a coloured halo for emphasis.
  Emphasis comes from weight, size, and the stamp.
- **Don't** add a kicker or eyebrow above a heading.
- **Don't** animate anything but the stamp landing. One authored moment.
- **Don't** let a destination the shop does not sell appear in the rail.
