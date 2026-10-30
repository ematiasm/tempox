<script lang="ts">
  /**
   * Does a re-read keep the row the reader had open?
   *
   * **The bug this reproduces.** `Ledger.svelte`'s effect clears `expanded` on every read, which
   * was correct while the effect only ran on a scope or day change. After the page owner was
   * introduced, a recorded credit note refreshes the page — the effect runs again — and the row
   * the cashier was looking at collapses. The credit-note block lives *inside* that row, so it
   * disappears from the screen even though the note exists. Two e2e caught it as "the note-block
   * is not there"; this pins the mechanism directly.
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { LedgerPage } from '#lib/ledgerPage.svelte.js';

  interface Props {
    /** The first version's behaviour: clear on every read. */
    clearOnRead?: boolean;
  }

  let { clearOnRead = true }: Props = $props();

  type Extra = { revision: number };
  const page = new LedgerPage<string, Extra>(2);
  let expanded = new SvelteSet<string>();
  let rows = $state<string[]>([]);
  let reads = 0;

  const apply = (list: string[], extra: Extra) => {
    reads += 1;
    rows = list;
    if (clearOnRead) expanded.clear();
    void extra;
  };

  const fetcher = async (offset: number) => ({
    page: { rows: ['a', 'b', 'c'].slice(offset, offset + 2), total: 3 },
    extra: { revision: reads }
  });

  export function read() {
    return page.read('day', fetcher, apply);
  }
  export function refresh() {
    page.refresh();
  }
  export function open(id: string) {
    expanded.add(id);
  }
  export function isOpen(id: string) {
    return expanded.has(id);
  }
  export function snapshot() {
    return { reads, rows: [...rows], open: [...expanded], offset: page.offset };
  }
</script>

<span data-open={isOpen('a') ? 'yes' : 'no'}>{rows.join(',')}</span>
