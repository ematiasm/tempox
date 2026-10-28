import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

/**
 * Two guards for the token layer, both from defects found while syncing the
 * design system (see `design/system.md`, "Deviations found").
 *
 * ## 1. A custom property that is consumed must be declared somewhere
 *
 * `/accounts` asked for `var(--figure)`, `var(--rounded-control)` and
 * `var(--rounded-panel)`. None of the three was ever declared. Nothing failed:
 * an unresolved custom property makes the declaration invalid, the browser drops
 * only that declaration, and the element quietly renders with whatever it
 * inherited. So the v3 screen set its figures in the prose face and squared off
 * its controls and its panel, while every other screen used the condensed face
 * and a 2px/8px radius. It shipped, it was reviewed, and it was invisible —
 * because the failure mode of a missing token is *silence*, not an error.
 *
 * The typo is the point: `--figure` is one character away from `--font-figure`,
 * and the only way to notice is to ask whether every name has a definition.
 *
 * A `var(--x, fallback)` is not checked: the fallback is the author saying "this
 * may be absent". An inline `style="--x: …"` counts as a declaration, because
 * it is one.
 *
 * ## 2. The retired world may not come back as a literal
 *
 * `retired-tokens.test.ts` bans the old *names* (`--paper`, `--indigo`,
 * `--crimson`). It cannot see their *values*, and the values survived: after the
 * alias block was deleted, six components kept painting rows with
 * `rgb(43 49 98 / 0.05)` (indigo) and `rgb(168 27 42 / 0.08)` (crimson) — the
 * exact hex of the retired tokens, now invisible to a grep for their names.
 *
 * A ban on names that cannot see values is a ban with a hole in it that widens
 * every time someone deletes the name and keeps the number. These values come
 * from the pre-redesign `app.css`, read back out of git history at the commit
 * that retired them (`92b48e0`), not from guesswork.
 */

const SRC = join(import.meta.dirname, '..', '..', 'src');

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (/\.(svelte|css|ts)$/.test(entry)) out.push(full);
  }
  return out;
}

/**
 * Blank out comments so prose explaining a rule is not read as the rule, while
 * keeping every newline so the reported line numbers still point at the file.
 *
 * An earlier version replaced each comment with the empty string, which deleted
 * the newlines inside it and shifted every line number after the first block
 * comment — the failure report then pointed at `}` and at the comment lines
 * themselves. A guard whose message misleads is worse than no guard at all.
 */
function code(text: string): string {
  const blank = (match: string) => match.replace(/[^\n]/g, ' ');
  const withoutBlockComments = text
    .replace(/\/\*[\s\S]*?\*\//g, blank)
    .replace(/<!--[\s\S]*?-->/g, blank);

  /**
   * Blank a `//` line comment, but only when it is unambiguously one.
   *
   * The rule is deliberately conservative: outside a quoted string, `//` starts a
   * comment only at the start of a line or when a space precedes it. That is the
   * shape every real comment has, and it is what keeps the three cases that used
   * to destroy code out:
   *
   * - `url(//cdn/x.png)` — a protocol-relative asset. The `//` follows `(`.
   * - `http://example.com//p` — a bare URL. The `//` follows `.` or another path
   *   character, never a space.
   * - `const re = /a\/\//; // note` — a regex literal. Its slashes touch the
   *   backslash or each other, so they are not preceded by a space; the real
   *   trailing comment is.
   *
   * The previous version guessed at a URL by checking only for `:`, and truncated
   * the line at the first `//` it met. That failed in the dangerous direction: a
   * regex swallowed the rest of its own line, so a `var(--missing)` in the real
   * comment after it was never seen — the guard silently stopped guarding. When
   * this rule is wrong it errs the other way, leaving prose in place so the guard
   * reports a token in a comment. A visible false positive is fixable; an
   * invisible false negative is not.
   *
   * Not handled, and stated rather than implied: a `//` with a space before it
   * *inside* a regex literal (`/a // b/`, which is unusual) still reads as a
   * comment. Parsing regex literals properly needs the surrounding expression
   * context, which a line-at-a-time scan does not have.
   */
  return withoutBlockComments
    .split('\n')
    .map((line) => {
      let quote: string | undefined;
      let escaped = false;
      for (let i = 0; i < line.length; i++) {
        const char = line[i];
        if (escaped) {
          escaped = false;
          continue;
        }
        if (quote) {
          if (char === '\\') escaped = true;
          else if (char === quote) quote = undefined;
          continue;
        }
        if (char === "'" || char === '"' || char === '`') {
          quote = char;
          continue;
        }
        if (char === '/' && line[i + 1] === '/') {
          const atLineStart = i === 0;
          const afterSpace = i > 0 && /\s/.test(line[i - 1] as string);
          if (!atLineStart && !afterSpace) continue;
          return line.slice(0, i) + ' '.repeat(line.length - i);
        }
      }
      return line;
    })
    .join('\n');
}

const files = walk(SRC);
const sources = files.map((file) => ({ file, text: code(readFileSync(file, 'utf8')) }));

describe('every consumed custom property is declared', () => {
  it('ignores // comments without swallowing following code', () => {
    const fixture = code('// TODO: revisit var(--nope)\nvar(--nope)');
    const references = [...fixture.matchAll(/var\(\s*(--[\w-]+)\s*\)/g)];
    const reported = references.map((match) => {
      const line = fixture.slice(0, match.index).split('\n').length;
      return `fixture:${line} var(${match[1]})`;
    });

    expect(reported).toEqual(['fixture:2 var(--nope)']);
  });

  /**
   * The line-comment rule must never destroy code, and must never lose a real
   * comment. Those are different failures and only one of them is loud.
   *
   * Measured against the version this replaces: trimming at the first `//` it met
   * truncated `const re = /a\/\//; // var(--nope)` down to `const re = /a\/\` — so
   * the code was gone AND the real comment after it was never seen. A guard that
   * silently stops guarding is worse than one that complains too much, which is
   * why the rule below now only blanks a `//` at the start of a line or after a
   * space, and leaves anything ambiguous in place.
   *
   * Each case pins both halves: the expected surviving text (code intact) and
   * whether the trailing `var(--nope)` was blanked.
   */
  it('never truncates code, and never loses a real comment', () => {
    const cases: Array<{ name: string; input: string; blanked: boolean }> = [
      // Real comments: the reference must disappear.
      { name: 'trailing comment', input: 'a { color: red; } // var(--nope)', blanked: true },
      { name: 'comment at line start', input: '// var(--nope)', blanked: true },
      { name: 'comment after a colon', input: '  key: // var(--nope)', blanked: true },
      { name: 'comment after a regex', input: '.a { grid: /a/; } // var(--nope)', blanked: true },
      { name: 'comment at end of line', input: 'a{} //', blanked: true },
      {
        name: 'code swallowed a regex before',
        input: String.raw`const r = /a\/\//; // var(--nope)`,
        blanked: true
      },
      { name: 'CLI flag in a string', input: 'x = "--flag"; // var(--nope)', blanked: true },
      // Not comments: the text must survive untouched, character for character.
      {
        name: 'URL inside a string',
        input: '.a { background: url("http://x/y"); }',
        blanked: false
      },
      { name: 'bare URL with a path', input: '.a { x: http://example.com//p; }', blanked: false },
      {
        name: 'protocol-relative URL',
        input: '.a { background: url(//cdn/x.png); }',
        blanked: false
      },
      {
        name: 'regex containing slashes',
        input: String.raw`const r = /a\/\//; // var(--nope)`,
        blanked: true
      },
      {
        name: 'escaped quote in a string',
        input: '.a { text: "it\\"s"; } // var(--nope)',
        blanked: true
      }
    ];

    for (const { name, input, blanked } of cases) {
      const out = code(input);
      // Length is preserved on purpose: the report quotes file:line, so a rule
      // that shifted characters would point at the wrong line.
      expect(out.length, `${name}: line length must not change`).toBe(input.length);
      if (blanked) {
        // A real comment was blanked. `lastIndexOf` rather than `indexOf`: a line
        // can carry a `/` or `//` before its comment (`const r = /a\/\//; // …`),
        // and the comment is the last one. Everything before it must be untouched
        // and the tail must be spaces — never a truncation to a shorter string.
        const at = input.lastIndexOf('//');
        expect(out.slice(0, at), `${name}: code before the comment`).toBe(input.slice(0, at));
        expect(out.slice(at).trim(), `${name}: the comment must be blank`).toBe('');
      } else {
        // Not a comment at all: the line must come back character for character.
        expect(out, `${name}: code must survive untouched`).toBe(input);
      }
    }
  });

  it('keeps a regex literal whole when a real comment follows it', () => {
    // The dangerous case, asserted on its own so a future rewrite of the table
    // above cannot quietly drop it: the regex survives AND the comment is gone.
    const out = code(String.raw`const r = /a\/\//; // var(--nope)`);
    expect(out).toContain(String.raw`/a\/\//;`);
    expect(out).not.toContain('var(--nope)');
  });

  it('no var() references an undeclared token', () => {
    // Declarations: `--name:` at the start of a declaration, which covers both
    // `:root { --name: … }` in app.css and an inline `style="--name: …"`.
    const declared = new Set<string>();
    for (const { text } of sources) {
      for (const match of text.matchAll(/(--[\w-]+)\s*:/g)) declared.add(match[1]);
    }

    const offenders: string[] = [];
    for (const { file, text } of sources) {
      // Only bare `var(--name)` — no fallback. `var(--name, x)` is intentional.
      for (const match of text.matchAll(/var\(\s*(--[\w-]+)\s*\)/g)) {
        const name = match[1];
        if (declared.has(name)) continue;
        const line = text.slice(0, match.index).split('\n').length;
        offenders.push(`${file}:${line} var(${name})`);
      }
    }

    expect(
      offenders,
      `These custom properties are consumed but never declared. A missing token ` +
        `does not throw: the declaration is dropped and the element silently ` +
        `renders with an inherited value.\n${offenders.join('\n')}`
    ).toEqual([]);
  });
});

/**
 * The retired palette, by value. Each entry names the token it used to be.
 *
 * `rgb()` spellings are listed separately because that is the form the surviving
 * literals actually took — `rgb(43 49 98 / 0.05)` is `#2b3162` at 5% and a hex
 * ban alone would not have caught one of them.
 */
const RETIRED_VALUES: Array<{ value: string; was: string }> = [
  { value: '#f6f2e9', was: '--paper' },
  { value: '#fbf9f3', was: '--paper-raised' },
  { value: '#efeade', was: '--paper-sunk' },
  { value: '#2b3162', was: '--indigo' },
  { value: '#4b5182', was: '--indigo-soft' },
  { value: '#a81b2a', was: '--crimson' },
  { value: '#7e1420', was: '--crimson-deep' },
  // The paper world's cream, used as light text on a dark popover. It is not a
  // token from the alias block, but it is that world's colour and it is still in
  // `ProductPicker.svelte`.
  { value: '#f2efe6', was: 'the paper world’s cream' },
  // These three are not alias-block tokens; they are surviving values from the
  // previous world, so guard them by value rather than inventing token names.
  { value: '#084a6c', was: 'a second blue used for error labels, replaced with var(--accent)' },
  { value: '#f2f6fa', was: 'a hover/open row fill in the day tables' },
  { value: '#0a1622', was: "the primary button's hover ink" }
];

const RETIRED_RGB = [
  { value: 'rgb(43 49 98', was: '--indigo' },
  { value: 'rgb(168 27 42', was: '--crimson' },
  { value: 'rgb(32 36 66', was: 'the retired hairline' },
  { value: 'rgb(242 239 230', was: 'the paper world’s cream' }
];

describe('the retired palette does not survive as a literal value', () => {
  it('no source file paints with a retired colour', () => {
    const offenders: string[] = [];

    for (const { file, text } of sources) {
      const lines = text.split('\n');
      lines.forEach((line, i) => {
        const lower = line.toLowerCase();
        for (const { value, was } of RETIRED_VALUES) {
          if (lower.includes(value)) offenders.push(`${file}:${i + 1} ${value} (${was})`);
        }
        for (const { value, was } of RETIRED_RGB) {
          // Spellings vary in whitespace; normalise before comparing.
          const flat = lower.replace(/\s+/g, ' ');
          if (flat.includes(value)) offenders.push(`${file}:${i + 1} ${value} (${was})`);
        }
      });
    }

    expect(
      offenders,
      `The retired palette is back, as a value instead of a name. Ask for a token ` +
        `instead; a low-alpha tint is \`color-mix(in srgb, var(--ink) N%, transparent)\`.\n` +
        `${offenders.join('\n')}`
    ).toEqual([]);
  });
});
