import { describe, expect, it } from 'vitest';
import { flushSync, mount } from 'svelte';
import Harness from './expanded-survives-refresh.svelte';

/**
 * A re-read must not collapse the row the reader has open.
 *
 * **This is the mechanism behind the two failing sales e2e**, isolated. `Ledger.svelte`'s effect
 * calls `expanded.clear()` on every read. That was right while the effect only ran when the scope
 * or day changed; once a recorded credit note began refreshing the page through the same effect,
 * every write collapsed the open row — and the credit-note block lives inside it, so the note
 * vanished from the screen while existing on the server.
 *
 * Written as a component test because the question is about what Svelte does with state across an
 * effect re-run, and that cannot be answered from a pure function.
 */
describe('a refresh after a write', () => {
  function setup(clearOnRead: boolean) {
    const target = document.createElement('div');
    document.body.append(target);
    const harness = mount(Harness, { target, props: { clearOnRead } });
    flushSync();
    return { harness, target };
  }

  it('keeps the open row, which is what the reader expects', async () => {
    const { harness } = setup(false);
    await harness.read();
    flushSync();
    harness.open('a');
    flushSync();
    expect(harness.isOpen('a')).toBe(true);

    // A credit note lands and the page is refreshed — the same row must stay open.
    harness.refresh();
    await harness.read();
    flushSync();

    expect(harness.isOpen('a')).toBe(true);
  });

  it('collapses it when the effect clears on every read, which is the defect', async () => {
    const { harness } = setup(true);
    await harness.read();
    flushSync();
    harness.open('a');
    flushSync();
    expect(harness.isOpen('a')).toBe(true);

    harness.refresh();
    await harness.read();
    flushSync();

    // Pinned as the *wrong* behaviour, so the fix has something to fail against: if someone
    // re-adds an unconditional clear, this test documents what breaks.
    expect(harness.isOpen('a')).toBe(false);
  });
});
