<script lang="ts">
  /**
   * The purchases ledger: the day's documents, newest first — the mirror of the
   * sales ledger, with money leaving instead of arriving.
   *
   * It deliberately does not carry the sales ledger's annul machinery: a purchase
   * has no status and no reversal document yet (that is v2.5), so a purchase row
   * here is a fact with nothing to expand into but its own lines.
   *
   * The scope is an instant the server resolves in the shop's timezone, so a
   * purchase belongs to the shop's day rather than to the browser's.
   */
  import {
    ApiError,
    fetchDay,
    listPurchases,
    listSuppliers,
    type DayRange,
    type PurchaseListItem,
    type Supplier
  } from '#lib/api.js';
  import { formatDay, formatMoney, formatQuantity, formatTime } from '#lib/money.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { SvelteSet } from 'svelte/reactivity';

  interface Props {
    /** Changes whenever a purchase is recorded, so the day reloads. */
    refreshToken?: number;
  }

  let { refreshToken = 0 }: Props = $props();

  let scopeInstant = $state(new Date());
  let range = $state<DayRange | null>(null);
  let rows = $state<PurchaseListItem[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let open = new SvelteSet<string>();
  // The list endpoint carries `supplier_id`, not a name: one purchase does not
  // need a joined row, and the suppliers are a short list the picker already
  // loads. Resolving here keeps the document's shape free of a denormalized
  // field that a rename would silently invalidate.
  let suppliers = $state<Supplier[]>([]);
  const supplierNames = $derived(new Map(suppliers.map((s) => [s.id, s.name])));

  // The shop's zone, as the server names it. Every time on the page renders in
  // it rather than in the machine's, so a shop in Buenos Aires reads its own
  // hours from a PC that may be set anywhere.
  const zone = $derived(range?.zone);

  function describe(e: unknown): string {
    if (e instanceof ApiError) return `HTTP ${e.status}`;
    return e instanceof Error ? e.message : String(e);
  }

  $effect(() => {
    const at = scopeInstant;
    // Reading refreshToken here is what makes a recorded purchase reload the day.
    void refreshToken;
    let cancelled = false;
    loading = true;
    error = null;

    (async () => {
      try {
        const day = await fetchDay(at);
        const [list, supplierList] = await Promise.all([
          listPurchases({ from: day.from, to: day.to, limit: 200 }),
          listSuppliers()
        ]);
        if (cancelled) return;
        range = day;
        rows = list;
        suppliers = supplierList;
      } catch (e) {
        if (!cancelled) error = describe(e);
      } finally {
        if (!cancelled) loading = false;
      }
    })();

    return () => {
      cancelled = true;
    };
  });

  function toggle(id: string) {
    if (open.has(id)) open.delete(id);
    else open.add(id);
  }
</script>

<section class="ledger" aria-labelledby="purchases-ledger-title">
  <header class="ledger-head">
    <div>
      <h2 id="purchases-ledger-title">{lang.t.purchases_ledger_title}</h2>
      <p class="sub">{lang.t.purchases_ledger_sub}</p>
    </div>
    <div class="pager">
      <span class="day figure">{range ? formatDay(new Date(range.from), zone) : '—'}</span>
      <button
        type="button"
        class="ghost"
        disabled={!range}
        onclick={() => {
          if (range) {
            // The server's neighbour, not browser arithmetic: a day is not
            // always 24 hours where a DST rule crosses midnight.
            scopeInstant = new Date(range.prev);
          }
        }}
      >
        {lang.t.sales_older}
      </button>
      <button
        type="button"
        class="ghost"
        disabled={!range}
        onclick={() => {
          if (range) scopeInstant = new Date(range.next);
        }}
      >
        {lang.t.sales_newer}
      </button>
    </div>
  </header>

  {#if error}
    <p class="error" role="alert">
      {lang.t.purchases_ledger_error}{error}
      <button type="button" class="ghost" onclick={() => (scopeInstant = new Date())}>
        {lang.t.sales_retry}
      </button>
    </p>
  {:else if loading}
    <p class="empty">{lang.t.sales_loading}</p>
  {:else if rows.length === 0}
    <p class="empty">
      {range && formatDay(new Date(range.from), zone) === formatDay(new Date(), zone)
        ? lang.t.purchases_no_purchases_today
        : lang.t.purchases_no_purchases_day}
    </p>
  {:else}
    <p class="count">
      {rows.length}
      {rows.length === 1 ? lang.t.purchases_document_one : lang.t.purchases_documents}
    </p>

    <ol class="documents">
      {#each rows as row (row.purchase.id)}
        {@const isOpen = open.has(row.purchase.id)}
        <li class="document">
          <button
            type="button"
            class="head"
            aria-expanded={isOpen}
            onclick={() => toggle(row.purchase.id)}
          >
            <span class="time figure">{formatTime(new Date(row.purchase.created_at), zone)}</span>
            <span class="supplier">
              {#if supplierNames.get(row.purchase.supplier_id) === 'one-off-supplier'}
                {lang.t.purchases_one_off_supplier}
              {:else}
                {supplierNames.get(row.purchase.supplier_id) ?? lang.t.purchases_one_off_supplier}
              {/if}
            </span>
            <span class="amount figure">{formatMoney(row.purchase.total)}</span>
            <span class="chevron" aria-hidden="true">{isOpen ? '−' : '+'}</span>
          </button>

          {#if isOpen}
            <table class="lines">
              <thead>
                <tr>
                  <th scope="col">{lang.t.sales_product}</th>
                  <th scope="col" class="num">{lang.t.sales_qty}</th>
                  <th scope="col" class="num">{lang.t.purchases_price_unit}</th>
                  <th scope="col" class="num">{lang.t.sales_line}</th>
                </tr>
              </thead>
              <tbody>
                {#each row.items as item (item.id)}
                  <tr>
                    <td>{item.product_name}</td>
                    <td class="num figure">
                      {formatQuantity(item.quantity, item.unit)}
                    </td>
                    <td class="num figure">{formatMoney(item.unit_price)}</td>
                    <td class="num figure">{formatMoney(item.line_total)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .ledger {
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: var(--radius);
    padding: 18px 20px 20px;
  }
  .ledger-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 18px;
    flex-wrap: wrap;
  }
  .ledger-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 2px;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .day {
    font-size: var(--step-1);
    font-weight: 600;
    text-transform: lowercase;
  }
  .ghost {
    background: none;
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    color: var(--ink-soft);
    font: inherit;
    font-size: var(--step--1);
    padding: 5px 10px;
    cursor: pointer;
  }
  .ghost:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--ink-soft);
  }
  .ghost:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .empty,
  .count {
    color: var(--ink-soft);
    font-size: var(--step-0);
    padding: 16px 0 4px;
  }
  .error {
    color: var(--crimson);
    padding: 14px 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .documents {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    border-top: 1px solid var(--rule-strong);
  }
  .document {
    border-bottom: 1px solid var(--rule);
  }
  .head {
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr) auto 20px;
    align-items: center;
    gap: 12px;
    width: 100%;
    background: none;
    border: 0;
    padding: 10px 4px;
    font: inherit;
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .head:hover {
    background: var(--paper);
  }
  .time {
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  .supplier {
    font-weight: 500;
  }
  .amount {
    font-weight: 600;
    font-size: var(--step-2);
  }
  .chevron {
    color: var(--ink-soft);
    text-align: center;
  }
  .figure {
    font-variant-numeric: tabular-nums;
    font-family: var(--font-figure);
  }
  .lines {
    width: 100%;
    border-collapse: collapse;
    margin: 0 0 12px;
  }
  .lines th {
    text-align: left;
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-soft);
    padding: 4px 8px;
    font-weight: 600;
  }
  .lines td {
    padding: 4px 8px;
    border-top: 1px solid var(--rule);
    font-size: var(--step-0);
  }
  .lines .num {
    text-align: right;
  }
</style>
