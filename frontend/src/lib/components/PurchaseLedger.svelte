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
    createSupplierReturn,
    fetchDay,
    listPurchases,
    listSuppliers,
    type DayRange,
    type PurchaseListItem,
    type Supplier
  } from '#lib/api.js';
  import { formatDay, formatMoney, formatQuantity, formatTime } from '#lib/money.js';
  import { createStaleNotice } from '#lib/screenTrust.js';
  import {
    isFullyReturned,
    parseReturnQuantity,
    planReturn,
    returnableOf,
    returnedByLine
  } from '#lib/supplierReturns.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';

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

  // ── Returns ──────────────────────────────────────────────────────────────
  //
  // The same shape the sales ledger uses for a refund, with the direction
  // reversed: a draft per line, a two-step confirm, and the day reloaded from
  // the server afterwards rather than patched locally. The server is the
  // authority on what went back, so a local tally would be a second opinion that
  // could disagree with the record.
  let returnDraft = new SvelteMap<string, Map<string, string>>();
  // Per row, not per component. With two documents expanded, a single `error`
  // string rendered inside every block showed one row's refusal under both, and
  // a single `confirming` put two rows into the confirm step at once.
  let confirming = new SvelteSet<string>();
  let returning = new SvelteSet<string>();
  let returnError = new SvelteMap<string, string>();
  // Kept apart from `returnError` on purpose: "the return did not happen" and
  // "the return happened but the day could not be re-read" are opposite facts,
  // and one message for both told the operator the opposite of the truth.
  // The lifetime rule lives in `screenTrust.ts`, shared with the sales ledger:
  // a successful reload retires every notice, because a warning that the screen
  // is behind is a lie once the screen has caught up.
  const reloadNotice = createStaleNotice(new SvelteMap<string, string>());

  // The shop's zone, as the server names it. Every time on the page renders in
  // it rather than in the machine's, so a shop in Buenos Aires reads its own
  // hours from a PC that may be set anywhere.
  const zone = $derived(range?.zone);

  function describe(e: unknown): string {
    if (e instanceof ApiError) return `HTTP ${e.status}`;
    return e instanceof Error ? e.message : String(e);
  }

  /**
   * A generation counter for day loads.
   *
   * Every load takes a number and every load checks it before writing. Without
   * this, a reload still in flight when the user pages would write the old day
   * over the new one -- measured: paging to the previous day while a slow reload
   * ran flipped the label back to today with today's purchase on screen.
   */
  let loadSeq = 0;

  /**
   * Reads one shop day and applies it, unless the caller says it went stale.
   *
   * The one place a day is read. The `$effect` and a recorded return both call
   * it, so the two cannot drift apart -- they used to be the same three lines
   * written twice, and only the effect had a cancellation guard.
   *
   * Returns false when the read was discarded, so a caller can tell "reloaded"
   * from "superseded": a return must not claim the day is current when the user
   * has already paged away from it.
   */
  async function loadDay(at: Date, isStale: () => boolean): Promise<boolean> {
    const day = await fetchDay(at);
    const [list, supplierList] = await Promise.all([
      listPurchases({ from: day.from, to: day.to, limit: 200 }),
      listSuppliers()
    ]);
    if (isStale()) return false;
    range = day;
    rows = list;
    suppliers = supplierList;
    // The figures are current again, so nothing on this screen is stale. The
    // first version only ever retired a notice on a click that happened to touch
    // that same row, so a warning survived every successful reload.
    reloadNotice.retireAll();
    return true;
  }

  /**
   * Re-reads the day already on screen, keeping the scope the user chose.
   *
   * Throws if the read fails, so a return can tell its own operator what
   * happened. Discards its own result silently if a newer load started, or if the
   * user paged while it was in flight -- that load is the one that owns the
   * screen now.
   */
  async function reload(at: Date): Promise<void> {
    const seq = ++loadSeq;
    await loadDay(at, () => seq !== loadSeq || scopeInstant !== at);
  }

  $effect(() => {
    const at = scopeInstant;
    // Reading refreshToken here is what makes a recorded purchase reload the day.
    void refreshToken;
    let cancelled = false;
    const seq = ++loadSeq;
    const isStale = () => cancelled || seq !== loadSeq;
    loading = true;
    error = null;

    (async () => {
      try {
        await loadDay(at, isStale);
      } catch (e) {
        if (!isStale()) error = describe(e);
      } finally {
        if (!isStale()) loading = false;
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

  /**
   * The draft text for one line: what was already returned sits under it as
   * "left", so a partial return reads as what is left rather than as a total.
   */
  function returnText(rowId: string, itemId: string, remaining: number): string {
    return returnDraft.get(rowId)?.get(itemId) ?? String(remaining);
  }

  function setReturnText(rowId: string, itemId: string, text: string) {
    const draft = new SvelteMap(returnDraft.get(rowId) ?? []);
    draft.set(itemId, text);
    returnDraft.set(rowId, draft);
  }

  function clearReturnDraft(rowId: string) {
    returnDraft.delete(rowId);
  }

  /**
   * The plan this document would send, or `null` when the form is not valid.
   *
   * The arithmetic lives in `supplierReturns.ts` so it can be tested without a
   * browser: the credit note measured that a component-only version of this is
   * where a line gets overdrawn.
   */
  function planFor(row: PurchaseListItem): { purchase_item_id: string; quantity: number }[] | null {
    const returned = returnedByLine(row.supplier_returns);
    const requested = new SvelteMap<string, number | null>();
    for (const item of row.items) {
      const remaining = returnableOf(item.id, item.quantity, returned);
      if (remaining === 0) continue;
      // `parseReturnQuantity`, not `parseQuantity`: on this form a zero (or a
      // blank) means "nothing from this line", while `parseQuantity` reads both
      // as `null` — which turned the whole plan invalid.
      requested.set(item.id, parseReturnQuantity(returnText(row.purchase.id, item.id, remaining)));
    }
    return planReturn(row.items, returned, requested);
  }

  async function submitReturn(row: PurchaseListItem) {
    const rowId = row.purchase.id;
    returnError.delete(rowId);
    reloadNotice.forget(rowId);

    const plan = planFor(row);
    if (plan === null) {
      returnError.set(rowId, lang.t.purchases_return_invalid);
      return;
    }
    if (returning.has(rowId)) return;

    returning.add(rowId);
    try {
      await createSupplierReturn(rowId, plan);
    } catch (e) {
      // The return did NOT happen. A 409 is the cumulative guard answering: the
      // line has less left than the form asked for.
      returnError.set(rowId, e instanceof ApiError ? `${e.message} (${e.status})` : String(e));
      return;
    } finally {
      returning.delete(rowId);
    }

    // From here the return is a fact on the server. Anything that fails below is
    // a failure to re-read, never a failure to return -- and saying so is the
    // difference between an operator retrying and an operator over-returning.
    clearReturnDraft(rowId);
    confirming.delete(rowId);
    try {
      await reload(scopeInstant);
    } catch (e) {
      reloadNotice.mark(
        rowId,
        `${lang.t.purchases_return_recorded}${e instanceof ApiError ? ` (${e.status})` : ''}`
      );
    }
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

  <!-- The retry the alert below cannot offer. A failed re-read after a written
       return sets the stale notice and never `error`, so this state had no way to
       refresh the day at all. It re-reads the day already on screen rather than
       jumping to today, because the notice is about that day. -->
  {#if !error && !reloadNotice.empty}
    <p class="error" role="status">
      {lang.t.purchases_stale_title}
      <button type="button" class="ghost" onclick={() => (refreshToken = refreshToken + 1)}>
        {lang.t.sales_retry}
      </button>
    </p>
  {/if}

  <!-- Mutually exclusive with the loading, empty and table states, as it was
       before: a failed read must replace the ledger region, never sit beside an
       empty-state paragraph claiming there are no purchases, or beside stale rows. -->
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
            {@const returned = returnedByLine(row.supplier_returns)}
            <table class="lines">
              <thead>
                <tr>
                  <th scope="col">{lang.t.sales_product}</th>
                  <th scope="col" class="num">{lang.t.sales_qty}</th>
                  <th scope="col" class="num">{lang.t.purchases_price_unit}</th>
                  <th scope="col" class="num">{lang.t.sales_line}</th>
                  <th scope="col" class="num">{lang.t.purchases_return_qty}</th>
                </tr>
              </thead>
              <tbody>
                {#each row.items as item (item.id)}
                  {@const remaining = returnableOf(item.id, item.quantity, returned)}
                  <tr>
                    <td>{item.product_name}</td>
                    <td class="num figure">
                      {formatQuantity(item.quantity, item.unit)}
                    </td>
                    <td class="num figure">{formatMoney(item.unit_price)}</td>
                    <td class="num figure">{formatMoney(item.line_total)}</td>
                    <td class="num return-cell">
                      {#if remaining > 0}
                        <input
                          type="text"
                          inputmode="numeric"
                          aria-label={lang.t.purchases_return_line(item.product_name)}
                          value={returnText(row.purchase.id, item.id, remaining)}
                          oninput={(event) =>
                            setReturnText(row.purchase.id, item.id, event.currentTarget.value)}
                        />
                      {:else}
                        <span class="returned">{lang.t.purchases_return_none}</span>
                      {/if}
                      <span class="left">
                        {lang.t.purchases_return_left(remaining, item.quantity)}
                      </span>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>

            <!-- The returns this purchase produced, oldest first, each with its
                 own total and lines. They sit UNDER the purchase and never
                 replace it: the purchase keeps every figure it was recorded
                 with, and whether it is fully returned is derived from these. -->
            {#each row.supplier_returns as supplierReturn (supplierReturn.return.id)}
              <section
                class="return-block"
                aria-label="{lang.t.purchases_supplier_return} DR-#{supplierReturn.return.id.slice(
                  0,
                  8
                )}"
              >
                <div class="return-head">
                  <span>
                    {lang.t.purchases_supplier_return} DR-#{supplierReturn.return.id.slice(0, 8)}
                  </span>
                  <span class="return-total figure"
                    >+{formatMoney(supplierReturn.return.total)}</span
                  >
                  <time>{formatTime(new Date(supplierReturn.return.created_at), zone)}</time>
                </div>
                <table>
                  <thead>
                    <tr>
                      <th scope="col">{lang.t.sales_product}</th>
                      <th scope="col" class="num">{lang.t.sales_qty}</th>
                      <th scope="col" class="num">{lang.t.sales_unit_price}</th>
                      <th scope="col" class="num">{lang.t.sales_line}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each supplierReturn.items as item (item.id)}
                      <tr>
                        <td>{item.product_name}</td>
                        <td class="num figure">{formatQuantity(item.quantity, item.unit)}</td>
                        <td class="num figure">{formatMoney(item.unit_price)}</td>
                        <td class="num figure">{formatMoney(item.line_total)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </section>
            {/each}

            <div class="refund">
              {#if isFullyReturned(row.items, row.supplier_returns)}
                <p>{lang.t.purchases_return_none}</p>
              {:else if confirming.has(row.purchase.id)}
                <p class="refund-warn">{lang.t.purchases_return_confirm}</p>
                <button
                  type="button"
                  class="danger"
                  disabled={returning.has(row.purchase.id)}
                  onclick={() => submitReturn(row)}
                >
                  {returning.has(row.purchase.id) ? '…' : lang.t.purchases_return_yes}
                </button>
                <button type="button" onclick={() => confirming.delete(row.purchase.id)}>
                  {lang.t.purchases_return_no}
                </button>
              {:else}
                <button
                  type="button"
                  class="danger"
                  onclick={() => {
                    confirming.add(row.purchase.id);
                    returnError.delete(row.purchase.id);
                    // Not "the operator dealt with this document": they may cancel.
                    // Dropping the stale notice here left the figures stale with no
                    // warning and no retry. Only a reload that succeeds retires it.
                  }}
                >
                  {lang.t.purchases_return_action}
                </button>
              {/if}
              {#if returnError.get(row.purchase.id)}
                <p role="alert">
                  {lang.t.purchases_return_error}{returnError.get(row.purchase.id)}
                </p>
              {/if}
              {#if reloadNotice.get(row.purchase.id)}
                <!-- Not a failure of the return: the server has it. The screen just
                     could not re-read the day, so what is left below may be stale. -->
                <p role="status">
                  {reloadNotice.get(row.purchase.id)}
                </p>
              {/if}
            </div>
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
  .return-cell {
    white-space: nowrap;
  }
  .return-cell input {
    width: 5.5em;
    padding: 5px 7px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: #fff;
    color: var(--ink);
    font-family: var(--font-figure);
    font-size: var(--step-1);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .left {
    display: block;
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  .returned {
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  /*
   * A return is a correction, not an alarm (DESIGN.md, "The Annulled Is Ink
   * Rule"), so its block is ink on paper and its amount is signed with a plus:
   * money comes back. Crimson would be a third role for the one accent, spent
   * against the stamp it belongs to.
   */
  .return-block {
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--paper);
    padding: 10px 12px;
    margin: 0 0 10px;
  }
  .return-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    font-family: var(--font-figure);
    font-size: var(--step-0);
    font-weight: 600;
    color: var(--ink-soft);
  }
  .return-head time {
    margin-left: auto;
    font-family: var(--font-text);
    font-weight: 400;
    font-size: var(--step--1);
  }
  .return-total {
    color: var(--ink);
    font-size: var(--step-1);
  }
  .return-block table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 6px;
  }
  .return-block th {
    text-align: left;
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-soft);
    padding: 4px 8px;
    font-weight: 600;
  }
  .return-block td {
    padding: 4px 8px;
    border-top: 1px solid var(--rule);
    font-size: var(--step-0);
  }
  .return-block .num {
    text-align: right;
  }
  .refund {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 22px;
    padding: 16px 0 4px;
  }
  .refund p {
    margin: 0;
  }
  .refund-warn {
    flex-basis: 100%;
    color: var(--ink-soft);
  }
  .refund button {
    border: 1px solid var(--rule-strong);
    background: var(--paper-raised);
    color: var(--ink);
    border-radius: var(--radius);
    padding: 7px 11px;
    font-size: var(--step-0);
    font-weight: 600;
    cursor: pointer;
  }
  .refund button.danger {
    border-color: var(--ink);
    background: transparent;
    color: var(--ink);
  }
  .refund button.danger:hover:not(:disabled) {
    border-color: var(--crimson);
    color: var(--crimson);
    background: rgb(168 27 42 / 0.08);
  }
  .refund button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .refund [role='alert'] {
    flex-basis: 100%;
    color: var(--crimson-deep);
    font-size: var(--step-0);
  }
  /*
   * A failed re-read is not an error, so it is not crimson: nothing went wrong
   * with the return. It is spare prose in soft ink, the same voice the day's
   * hints use.
   */
  .refund [role='status'] {
    flex-basis: 100%;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
</style>
