<script lang="ts">
  /**
   * The list of parties, shared by `/clientes` and `/proveedores`.
   *
   * `v5-customer-account.md` S1: two pages, one component. The failure mode this
   * avoids is the one this repository already has — `Ledger.svelte` and
   * `PurchaseLedger.svelte` are the same table written twice, 2700 lines, drifting.
   *
   * **It reads and does nothing else.** Opening a party's panel is T3; a row is a
   * link to `?c=<id>` so the panel is deep-linkable, and this component's whole job
   * is to render the list and hand the id up.
   */
  import { lang } from '#lib/i18n/index.svelte.js';

  interface Party {
    id: string;
    name: string;
    created_at: string;
  }

  let {
    parties,
    error,
    empty,
    onopen
  }: {
    parties: Party[] | null;
    error: string | null;
    empty: string;
    onopen: (id: string) => void;
  } = $props();
</script>

{#if error !== null}
  <!-- A read that failed is not an empty list, and the two must never look alike:
       "there are no clients" and "we could not read the clients" are different
       facts and the shop acts differently on each. -->
  <p class="state error" role="alert">{error}</p>
{:else if parties === null}
  <p class="state">{lang.t.customers_loading}</p>
{:else if parties.length === 0}
  <p class="state">{empty}</p>
{:else}
  <ul class="rows">
    {#each parties as party (party.id)}
      <li class="row">
        <!-- A button, not a link with an href: the id belongs in the URL as a query
             parameter the page owns (S2), and a plain `<a href="?c=…">` would make
             the browser push a history entry per click, so Back would close the
             panel instead of leaving the page. -->
        <button class="open" type="button" onclick={() => onopen(party.id)}>
          <!-- The seeded walk-in and one-off-supplier are real rows, not noise: they
               are who most sales and purchases belong to, so they are labelled the
               way the rest of the app labels them rather than shown as raw slugs. -->
          <span
            class="name"
            class:seeded={party.name === 'walk-in' || party.name === 'one-off-supplier'}
          >
            {#if party.name === 'walk-in'}
              {lang.t.sales_walkin}
            {:else if party.name === 'one-off-supplier'}
              {lang.t.purchases_one_off_supplier}
            {:else}
              {party.name}
            {/if}
          </span>
          <span class="mark" aria-hidden="true">›</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row + .row {
    border-top: 1px solid var(--rule);
  }
  /* The whole row is the target: a row whose only clickable part is the name makes
     the shop aim at a word. */
  .open {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 14px 26px 14px 28px;
    background: none;
    border: 0;
    text-align: left;
    font: inherit;
    color: var(--ink);
    cursor: pointer;
  }
  .open:hover {
    background: var(--ground);
  }
  .open:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .name {
    font-size: var(--step-1);
  }
  /* The two seeded rows are real counterparties but not customers the shop added.
     Set apart by weight, not colour — the accent is spent on the balance rule. */
  .name.seeded {
    color: var(--ink-soft);
    font-style: italic;
  }
  .mark {
    color: var(--ink-soft);
    font-family: var(--font-figure);
  }
  .state {
    padding: 18px 28px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .state.error {
    color: var(--state-void);
  }
</style>
