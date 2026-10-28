# Design-token guard — deferred findings

The design-token guard (`frontend/tests/unit/design-tokens.test.ts`) has been
through four review rounds. Each one approved the work and left **non-blocking**
findings; this file is where they live so they stop existing only in a
conversation.

None of these blocked a merge. All are informational. Nothing here reopens a
review that already stands.

## Open debt

### Scanner heuristics — `frontend/tests/unit/design-tokens.test.ts:119`

The line-comment rule blanks a `//` only at the start of a line or after a space.
It is deliberately conservative, and it still has edges:

- A `//` preceded by a space *inside* a regex literal (`/a // b/`) reads as a
  comment and swallows the rest of the line. Fixing this needs the surrounding
  expression context, which a line-at-a-time scan does not have.
- Escaped quotes are handled, but template-literal interpolation is not parsed,
  so a `//` inside `${ … }` is judged as if it were plain text.

### Fixture coverage — `frontend/tests/unit/design-tokens.test.ts:184`

The `regex containing slashes` fixture pins the case that used to be destroyed,
but the table does not cover a regex with a space before an internal `//`
(the limitation above), so the guard's tolerance there is unmeasured rather than
proven.

### Earlier rounds, still open

- `design/system.md:154-163` — readability of the deviations section
  (R2-001, round 1)
- `odd/tasks/receipt-pad-delivery.md:56-59` — readability of the PR-1 paragraph
  (R2-002, round 1)

## The judgement behind leaving this open

Four rounds of findings, all on the same function, is a signal about the
approach rather than about any single line. A hand-rolled scanner approximating
JavaScript comment syntax will keep producing one more edge case per review.

If this becomes worth doing properly, the two honest options are:

1. **Parse instead of guess.** Use a real tokenizer for the file types the guard
   reads, so comment detection stops being a heuristic. Cost: a dependency, or a
   small parser maintained here.
2. **Stop distinguishing, start tolerating.** Drop `//`-comment handling entirely
   and make the guards ignore a `var(--name)` that appears only in prose. Simpler
   and provably safe in the direction that matters (the guard can then only
   over-report, never silently under-report).

Both are better than a fifth special case. Neither is urgent: the guard currently
fails toward over-reporting, which is the visible, fixable side of the trade.
