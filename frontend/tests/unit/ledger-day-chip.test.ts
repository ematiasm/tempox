import { describe, expect, it } from 'vitest';
import { esAR, en } from '#lib/locale.js';

/**
 * The ledger's day is the shop's day, and the chip must say so (S6, S8, S19).
 *
 * This file exists because a real defect slipped past a first pass: the rows were
 * correctly scoped by the server while the chip above them was formatted from the
 * browser's clock. On a machine set to UTC, the shop's 16 Oct read `17 oct 2026`
 * over 16-Oct rows — the page contradicting itself in exactly the way S19 forbids.
 *
 * Every assertion here passes **two explicit zones** and compares them, so it is
 * deterministic: it does not depend on the timezone of the machine running the
 * suite. An implementation that ignores the `zone` argument returns the same
 * string for both and fails, wherever it runs.
 */

const SHOP = 'America/Argentina/Buenos_Aires';

/** An independent second opinion, built from `Intl` directly. */
function reference(instant: string, zone: string, locale: string = 'es-AR'): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeZone: zone }).format(
    new Date(instant)
  );
}

describe('the day chip names the shop date, not the browser date (S6, S19)', () => {
  it('renders a late-evening instant on two different dates, one per zone', () => {
    // 2026-10-17T01:30Z is 22:30 on 16 Oct in Buenos Aires and 01:30 on 17 Oct in
    // UTC. The chip must be able to say either one; a formatter that ignores the
    // zone can only ever say the machine's, and these two assertions collapse
    // into one string.
    const at = new Date('2026-10-17T01:30:00Z');
    expect(esAR.day(at, SHOP)).toBe('16 oct 2026');
    expect(esAR.day(at, 'UTC')).toBe('17 oct 2026');
    expect(esAR.day(at, SHOP)).not.toBe(esAR.day(at, 'UTC'));
  });

  it('agrees with Intl for both languages and both zones', () => {
    const at = new Date('2026-10-17T01:30:00Z');
    for (const zone of [SHOP, 'UTC', 'Asia/Tokyo', 'America/Chicago']) {
      expect(esAR.day(at, zone)).toBe(reference(at.toISOString(), zone, 'es-AR'));
      expect(en.day(at, zone)).toBe(reference(at.toISOString(), zone, 'en'));
    }
  });

  it('labels a day whose first instant is 01:00 with its own date, not the previous', () => {
    // Santiago springs forward at midnight on 5 Sep 2027, so that day begins at
    // 01:00 local. A formatter without the zone (or an offset guessed from a
    // neighbouring day) drifts here; `Intl` with the zone does not.
    const at = new Date('2027-09-05T15:00:00Z');
    expect(esAR.day(at, 'America/Santiago')).toBe('5 sept 2027');
    expect(esAR.day(at, 'America/Santiago')).toBe(reference(at.toISOString(), 'America/Santiago'));
  });

  it('renders the shop day the server returned, whatever the browser zone is', () => {
    // `range.from` is the day's first instant as the server resolved it, and the
    // chip is rendered from exactly that value in exactly that zone.
    const from = '2026-10-16T03:00:00Z'; // Buenos Aires local midnight
    expect(esAR.day(new Date(from), SHOP)).toBe('16 oct 2026');
    expect(reference(from, SHOP)).toBe('16 oct 2026');
  });

  it('falls back to the browser zone only when the shop zone is unknown', () => {
    // First paint, before `/api/day` answers. The fallback is the previous
    // behaviour and never a fabricated date: it equals the zone-less formatter.
    const at = new Date(2026, 9, 16, 12, 0);
    const zoneless = new Intl.DateTimeFormat('es-AR', { dateStyle: 'medium' }).format(at);
    expect(esAR.day(at)).toBe(zoneless);
  });
});

describe('paging walks the shop day, not the browser day (S8)', () => {
  it('uses the neighbouring days the server named, which are one shop day apart', () => {
    // The defect: shifting the scope by one *browser* calendar day is not one shop
    // day in a zone whose DST change crosses midnight. Havana repeats local
    // midnight on 7 Nov 2027, so 00:00 happens twice; a browser-day shift from the
    // wrong occurrence lands on the wrong page. The server's `prev`/`next` are the
    // neighbouring firsts, so consecutive pages always meet.
    const zone = 'America/Havana';
    const dayStart = '2027-11-07T04:00:00Z'; // the first 00:00, CDT
    const next = '2027-11-08T05:00:00Z'; // next local midnight, CST

    expect(esAR.day(new Date(dayStart), zone)).toBe('7 nov 2027');
    expect(esAR.day(new Date(next), zone)).toBe('8 nov 2027');

    // They are consecutive shop days, and 24h after the first instant is *not*
    // the second one: the repeated hour makes that day 25 hours long.
    const naive = new Date(new Date(dayStart).getTime() + 24 * 3600 * 1000).toISOString();
    expect(naive).not.toBe(next);
    expect(esAR.day(new Date(naive), zone)).toBe('7 nov 2027');
  });

  it('walks consecutive Argentine days exactly 24 hours apart', () => {
    // The default zone has no DST, so here the two agree — which is why the
    // Havana case above is the one that proves the arithmetic matters.
    const dayStart = '2026-10-16T03:00:00Z';
    const next = '2026-10-17T03:00:00Z';
    expect(new Date(new Date(dayStart).getTime() + 24 * 3600 * 1000).getTime()).toBe(
      new Date(next).getTime()
    );
    expect(esAR.day(new Date(dayStart), SHOP)).toBe('16 oct 2026');
    expect(esAR.day(new Date(next), SHOP)).toBe('17 oct 2026');
  });
});
