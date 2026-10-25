import { describe, expect, it } from 'vitest';
import { partiesNav } from '#lib/partiesNav.js';

/**
 * The two new destinations, and the rule the rail already follows.
 *
 * `v5-customer-account.md` S1 and S7. Two pages, not tabs: they are two jobs in the
 * shop's day and the rail marks one active. `/clientes` and `/proveedores` sort into
 * the rail next to the four that exist.
 *
 * The reason this is a pure function and not four lines of markup: the rail is where
 * a destination can silently disappear. A tab whose `href` and label drift apart, or
 * whose `startsWith` check matches a sibling, is a nav that lies about where you are
 * — and the existing `+layout.svelte` derives both from one list precisely to make
 * that impossible. A second list written by hand in another file is how it becomes
 * possible again.
 */

describe('the parties rail entries', () => {
  it('offers exactly the two destinations, in the rail order the shop reads', () => {
    expect(partiesNav.es.map((t) => t.href)).toEqual(['/clientes', '/proveedores']);
    expect(partiesNav.en.map((t) => t.href)).toEqual(['/clientes', '/proveedores']);
  });

  it('names each destination in both shipped languages', () => {
    expect(partiesNav.es.map((t) => t.label)).toEqual(['Clientes', 'Proveedores']);
    expect(partiesNav.en.map((t) => t.label)).toEqual(['Customers', 'Suppliers']);
  });

  it('keeps href and label paired, so a rename cannot leave the wrong word on the right link', () => {
    for (const lang of [partiesNav.es, partiesNav.en]) {
      for (const tab of lang) {
        expect(tab.href.startsWith('/')).toBe(true);
        expect(tab.label.trim()).not.toBe('');
      }
    }
  });

  it('needs no word of its own for `es` beyond the two names', () => {
    // `/clientes` and `/proveedores` are the same in both languages: the *word* the
    // shop sees is translated, the *route* is not, because a URL is a contract and a
    // localized path would break every bookmark and every link a cashier pastes.
    expect(partiesNav.es.map((t) => t.href)).toEqual(partiesNav.en.map((t) => t.href));
  });
});
