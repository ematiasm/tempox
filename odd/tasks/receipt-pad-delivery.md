# Receipt pad — delivery plan

This branch holds three features' worth of work and is far past one review slice.
It ships as a chain of three PRs, each of which leaves the app working and the
suite green on its own.

## The chain

```text
main
 └── PR 1  docs(design): the design system, three directions, and the token fix
      └── PR 2  feat(sales): the receipt pad for /sales
           └── PR 3  feat(purchases): the mirror, and the guards it exposed
```

Each PR targets the previous PR's branch so its diff shows only its own work.
After a parent merges, retarget the next one to `main` — the same flow this
repository used for #74 → #75, and the reason `ci.yml` deliberately has no
`branches:` filter (a PR whose base is another branch runs no CI at all
otherwise).

## Why three, and why each is over the 400-line budget

| PR | Changed lines | What it is |
|---|---|---|
| 1 | ~3.8k | the design system sync, three 1440×900 artboards, the canvas record |
| 2 | ~2.1k | `/sales` rebuilt as a pad and a stack of slips |
| 3 | ~2.2k | `/purchases` mirrored, the shared slip extracted, two guards fixed |

One honest slicing pass produced this split, and no cohesive split gets any of
them under 400. The reason is structural rather than slack: this work **replaces**
markup and styles, so every touched line counts twice — once as a deletion and
once as an addition. `/sales` changing from a table to a slip stack is ~600 lines
of CSS leaving and ~600 arriving; that is one deliverable and it cannot be cut
without producing a PR whose intermediate state does not work.

Splitting further would mean shipping "the draft's markup" in one PR and "the
draft's styles" in the next — a review where nothing renders, which is worse for
a reviewer than a larger coherent one. Each PR therefore carries a
`size:exception` rationale naming what makes it large and confirming it is one
deliverable.

The artboards in PR 1 are the largest single contributor and are worth a note:
six self-contained HTML files, ~2.9k lines of design source, reviewed by looking
at them rather than by reading them line by line. The canvas renders them
side by side.

## What each PR delivers

**PR 1 — the design system and the directions.** `design/system.md` synced from
real sources with a `path:line` per value, `design/system.html` rendering those
tokens, three structurally different `/sales` directions, and the canvas handoff
record. It also carries the token repair the sync exposed: `/accounts` consumed
`--figure`, `--rounded-control` and `--rounded-panel`, none declared, measured in
the browser as `Plex Text` and a `0px` radius where the app meant `Plex
Condensed` and `8px`; and the retired palette's values (`#2b3162`, `#a81b2a`,
`#084a6c`, `#f2f6fa`, `#f2efe6`, `#0a1622`) surviving as literals in six files,
invisible to a test that bans their *names*. Both are now guarded by
`tests/unit/design-tokens.test.ts`.

**PR 2 — `/sales` as the receipt pad.** The draft becomes the slip it prints and
the day becomes a stack of documents, each carrying the balance it inherits. The
selector migration uncovered that nothing asserted the running balance on the
render path any more; that gap is closed with a test proven to bite.

**PR 3 — `/purchases`, and the guards.** The mirror of PR 2, then the shared slip
extracted into `DaySlip.svelte` — and the two defects that surfaced while
deciding whether to merge the drafts instead: a purchase could be recorded twice
by any caller that was not the button (two purchases, stock up twice, till down
twice), and the screen advertised an `F2` shortcut that no handler listened for.

## Out of scope, deliberately

- **Consolidating `SaleDraft` and `PurchaseDraft`.** Their scripts diverge in 308
  semantically meaningful lines and `draftLines.ts` records that the two screens
  have deliberately different validity rules. Deferred with a criterion: extract
  only their shared *presentational frame*, only when the redesign next touches a
  draft, and never the validation rules.
- **Any `Draft.svelte` with a `mode` flag.** That is the shape that reads well in
  a diff and rots in maintenance.
