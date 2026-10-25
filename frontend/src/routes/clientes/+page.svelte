<script lang="ts">
  /**
   * `/clientes` — who buys from the shop, and the way into their account.
   *
   * `v5-customer-account.md` S1, S7. Reading only: the panel that shows a client's
   * balance and unpaid sales is T3, and this page does not anticipate its shape
   * beyond handing it the id.
   */
  import { lang } from '#lib/i18n/index.svelte.js';
  import { listCustomers, type Customer } from '#lib/api.js';
  import PartyList from '#lib/components/PartyList.svelte';

  let parties = $state<Customer[] | null>(null);
  let error = $state<string | null>(null);

  // **A failed read and an empty list are different facts**, so the error is kept
  // separate from the data instead of rendering as `[]`. The list component renders
  // each of the three states (loading, failed, empty) differently for the same reason.
  $effect(() => {
    let live = true;
    listCustomers()
      .then((rows) => {
        if (live) parties = rows;
      })
      .catch(() => {
        if (live) error = lang.t.customers_error;
      });
    return () => {
      live = false;
    };
  });

  /**
   * Opening a party is T3. It is wired here as a no-op that names itself rather
   * than left unwired, so the button is not a dead control that silently does
   * nothing — the same rule the sales screen applies to a disabled action.
   */
  function open(id: string) {
    const url = new URL(window.location.href);
    url.searchParams.set('c', id);
    // `replaceState`: Back must leave the page, not close the panel (S2).
    history.replaceState(history.state, '', url);
  }
</script>

<div class="page">
  <header class="head">
    <h1>{lang.t.customers_title}</h1>
    <span class="tagline">{lang.t.customers_tagline}</span>
  </header>
  <section class="panel" aria-labelledby="customers-h">
    <h2 id="customers-h" class="sr-only">{lang.t.customers_title}</h2>
    <PartyList {parties} {error} empty={lang.t.customers_empty} onopen={open} />
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
