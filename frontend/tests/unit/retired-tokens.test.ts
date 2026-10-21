import { describe, it, expect } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The previous world's palette names do not come back.
 *
 * Why this test exists rather than a comment: for most of the redesign, five
 * components still asked for `--paper`, `--indigo` and `--crimson`, and a
 * transitional alias block in `app.css` translated those names onto the new
 * tokens so the app never sat unstyled. That block was migration scaffolding
 * with a deadline, and the deadline arrived — it is deleted.
 *
 * The risk it leaves behind is quiet. Re-adding `var(--crimson)` in a new
 * component does not fail to build, does not fail any behavioural test and does
 * not throw: the property simply becomes invalid, the declaration is dropped,
 * and the element silently loses its colour. Nothing in this suite would notice;
 * a screenshot might, if someone took one. This test is the cheap guard that
 * makes the regression loud instead.
 *
 * `--accent` is the trap worth naming: under the alias block `--crimson`
 * resolved to it, so a `crimson` that meant "alarm" quietly became "the balance
 * colour". Banning the old names is what stops that mistranslation from being
 * re-introduced by someone copying an old rule.
 */

const RETIRED = [
  '--paper',
  '--paper-raised',
  '--paper-sunk',
  '--indigo',
  '--indigo-soft',
  '--crimson',
  '--crimson-deep',
  '--tab-overhang'
];

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (/\.(svelte|css|ts)$/.test(entry)) out.push(full);
  }
  return out;
}

describe('the previous world’s palette names are retired', () => {
  const files = walk(join(import.meta.dirname, '..', '..', 'src'));

  it('no source file consumes a retired token', () => {
    const offenders: string[] = [];

    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      const lines = text.split('\n');
      for (const name of RETIRED) {
        // Match a real consumption (`var(--paper)`, `var(--indigo, …)`) and not a
        // mention in prose. Comments explaining the migration are legitimate and
        // this test must not force them to be deleted.
        const pattern = new RegExp(`var\\(\\s*${name}\\s*[,)]`);
        lines.forEach((line, i) => {
          const code = line.replace(/\/\*.*?\*\//g, '');
          if (pattern.test(code)) offenders.push(`${file}:${i + 1} ${line.trim()}`);
        });
      }
    }

    expect(offenders, `retired palette names found:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('app.css no longer defines the transitional alias block', () => {
    const css = readFileSync(join(import.meta.dirname, '..', '..', 'src', 'app.css'), 'utf8');
    for (const name of RETIRED) {
      expect(css, `app.css still defines ${name}`).not.toMatch(
        new RegExp(`^\\s*${name}\\s*:`, 'm')
      );
    }
  });
});
