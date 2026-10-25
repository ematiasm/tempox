<script lang="ts">
  /**
   * `/proveedores` — who we buy from, and the way into their account.
   *
   * `v5-customer-account.md` S1, S7. Reading only: the panel that shows a supplier.s
   * balance and unpaid sales is T3, and this page does not anticipate its shape
   * beyond handing it the id.
   */
  import { lang } from '#lib/i18n/index.svelte.js';
  import { listSuppliers, type Supplier } from '#lib/api.js';
  import PartyList from '#lib/components/PartyList.svelte';

  let parties = $state<Supplier[] | null>(null);
  let error = $state<string | null>(null);

  // **A failed read and an empty list are different facts**, so the error is kept
  // separate from the data instead of rendering as `[]`. The list component renders
  // each of the three states (loading, failed, empty) differently for the same reason.
  $effect(() => {
    let live = true;
    listSuppliers()
      .then((rows) => {
        if (live) parties = rows;
      })
      .catch(() => {
        if (live) error = lang.t.suppliers_error;
      });
    return () => {
      live = false;
    };
  });

  /**
   * **Opening a party is T3, and this is where it will attach.** Until the panel
   * exists, the honest state is a URL that changes and a page that says so — not a
   * button that looks live and does nothing.
   *
   * The review of this candidate (lineage `review-1817973d81dd8fda`, `R2-002` and
   * `R3-004`) caught the earlier version claiming the click was "wired" when all it
   * did was rewrite the query string: a comment that overstates is worse than none,
   * because it is the reason nobody looks. So the page now **reads** the parameter it
   * writes, and shows the pending state rather than pretending the click landed.
   */
  let opened = $state<string | null>(null);

  function open(id: string) {
    const url = new URL(window.location.href);
    url.searchParams.set('c', id);
    // `replaceState`: Back must leave the page, not close the panel (S2).
    history.replaceState(history.state, '', url);
    opened = id;
  }

  // The URL is the source of truth, so a deep link arrives with the panel "open".
  // T3 replaces the paragraph below with the panel itself.
  $effect(() => {
    const id = new URL(window.location.href).searchParams.get('c');
    if (id !== null) opened = id;
  });
</script>

<div class="page">
  <header class="head">
    <h1>{lang.t.suppliers_title}</h1>
    <span class="tagline">{lang.t.suppliers_tagline}</span>
  </header>
  <section class="panel" aria-labelledby="suppliers-h">
    <h2 id="suppliers-h" class="sr-only">{lang.t.suppliers_title}</h2>
    <PartyList {parties} {error} empty={lang.t.suppliers_empty} onopen={open} />
    {#if opened !== null}
      <!-- T3 replaces this with the panel. Until then it says what is true: the id
           is selected, and nothing reads it yet. A silent no-op is the defect the
           review found. -->
      <p class="state pending" role="status">{lang.t.party_panel_pending}</p>
    {/if}
  </section>
</div>

<style>
  .page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
  }
  .head {
    padding: 22px 26px 16px 28px;
    display: flex;
    align-items: baseline;
    gap: 14px;
    flex-wrap: wrap;
  }
  .head h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .tagline {
    font-size: var(--step-0);
    color: var(--ink-soft);
  }
  /* A work panel: closed surface, visible edge, 8px radius (DESIGN.md). */
  .panel {
    margin: 0 26px 26px 28px;
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: 8px;
    overflow: auto;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
