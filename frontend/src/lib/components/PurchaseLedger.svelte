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
    getPurchasesSummary,
    listPurchases,
    listSuppliers,
    type DayRange,
    type PurchasesSummary,
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
  import {
    canStepPage,
    clampOffset,
    pageCount,
    pageIsPartial,
    pageNumber
  } from '#lib/ledgerPaging.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { isOnAccount, methodLabel } from '#lib/methodLabel.js';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import Stamp from './Stamp.svelte';
  import DaySlip from './DaySlip.svelte';
  import { overflowsX } from '#lib/overflowsX.js';

  interface Props {
    /** Changes whenever a purchase is recorded, so the day reloads. */
    refreshToken?: number;
    /** Reports the period's document count to the page header. */
    oncount?: (count: number | null) => void;
  }

  let { refreshToken = 0, oncount }: Props = $props();

  let scopeInstant = $state(new Date());
  let range = $state<DayRange | null>(null);
  let rows = $state<PurchaseListItem[]>([]);
  // The period's totals, from the database. Null until the first read lands, so
  // the footer shows an em dash rather than a zero it has not earned.
  let summary = $state<PurchasesSummary | null>(null);

  // How many columns the reader cannot see, measured by the `overflowsX` action.
  let hiddenColumns = $state(0);

  /**
   * The size of one page of this ledger, and which page is on screen.
   *
   * **This ledger used to ask for 200 rows and let the rest of the day go in silence.** No
   * notice, no pager, no way to learn that a day holding 300 purchases rendered 200 of them —
   * the same class as `R2-001` and `R3-004` on the expenses side, in its worst form: silent.
   * `ledgerPaging.ts` owns the arithmetic now, shared with the other two ledgers.
   */
  const PAGE = 200;
  let pageOffset = $state(0);

  // Tell the page how many documents the period holds, whenever that changes.
  const periodDocuments = $derived(summary?.documents ?? null);
  /** The period's exact document count, so the last page is a known address. */
  const total = $derived(periodDocuments ?? 0);
  $effect(() => {
    oncount?.(periodDocuments);
  });
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
   * Moves to the neighbouring page of the period, clamped by `clampOffset`.
   *
   * The clamp is the rule rather than a guard: it is what makes "the last page is an address"
   * true instead of hopeful, and it is what stops the pager disabling a button that would
   * have moved — the defect the expenses pager shipped with.
   */
  function stepPage(delta: number) {
    const next = clampOffset(pageOffset + delta * PAGE, PAGE, total);
    if (next === pageOffset) return;
    pageOffset = next;
    // An expanded row from the previous page is not on screen any more; leaving it open
    // would reopen a stranger's form on return.
    open.clear();
    // **Through `reload`, and that is the fix for `R3-001` of `review-58ab3200d6f59c90`.**
    // This called `loadDay(scopeInstant, () => false)`, and a constant-false predicate is not
    // "the counter owns staleness" — it is "nothing is ever stale". `loadDay` gates every
    // write with `if (isStale()) return false`, so two page steps in flight both wrote: the
    // slower response overwrote `rows` and `summary` after the newer one had landed, leaving
    // the table on a different page from the one the pager names. `reload` takes the counter
    // and bumps it, so the older response is discarded, and it carries the same guard a day
    // change has.
    void reload(scopeInstant);
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
    // The totals come from the database, not from the rows this page happens to
    // hold: `listPurchases` caps at its own page size, so anything summed here
    // omits the rest of the period. One request covers the whole day.
    const [list, supplierList, totals] = await Promise.all([
      listPurchases({ from: day.from, to: day.to, limit: PAGE, offset: pageOffset }),
      listSuppliers(),
      getPurchasesSummary(day.from, day.to)
    ]);
    if (isStale()) return false;
    range = day;
    // **A remembered offset can outlive the rows it addressed.** A day left open while a
    // purchase is returned elsewhere shrinks, and the page a reader is on can end up past the
    // end — an empty table under a foot that reports documents, which is the contradiction
    // this whole class of fix exists to remove. Re-reading once is safe because the clamp is
    // idempotent, so the second pass falls through.
    const clamped = clampOffset(pageOffset, PAGE, totals.documents);
    if (clamped !== pageOffset) {
      pageOffset = clamped;
      // The caller's staleness check still applies: a reload triggered by the clamp is not
      // allowed to overwrite a newer day.
      if (isStale()) return false;
      return loadDay(at, isStale);
    }
    rows = list;
    suppliers = supplierList;
    summary = totals;
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
    // **The page the request was made for is part of staleness.** Without it, stepping twice
    // quickly leaves the slower response writing rows for a page the reader has left — the
    // same defect as the constant predicate above, reachable through the write path too.
    const requestedPage = pageOffset;
    await loadDay(at, () => seq !== loadSeq || scopeInstant !== at || pageOffset !== requestedPage);
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
      {#if pageIsPartial(rows.length, total)}
        <!-- **The count used to be the page's and read as the period's.** A day holding 300
             purchases rendered "200 documentos" over footings that covered all 300, which is
             the silent truncation this fix removes. -->
        {lang.t.ledger_shown_partial(rows.length, total)}
      {:else}
        {rows.length}
        {rows.length === 1 ? lang.t.purchases_document_one : lang.t.purchases_documents}
      {/if}
    </p>

    <div class="table-frame">
      <div class="table-scroll" use:overflowsX={(n) => (hiddenColumns = n)}>
        {#each rows as row (row.purchase.id)}
          {@const isOpen = open.has(row.purchase.id)}
          {@const returned = isFullyReturned(row.items, row.supplier_returns)}
          {@const refunded = returnedByLine(row.supplier_returns)}
          <!-- The slip itself lives in `DaySlip.svelte`: the same head, chevron,
               body and keyboard contract as the sale's, with the fields this
               surface prints passed in. Purchases deliberately pass no `balance`
               — a purchase has no running balance in this design. -->
          <DaySlip
            id={row.purchase.id}
            createdAt={row.purchase.created_at}
            {zone}
            idPrefix="purchase-lines-"
            total={row.purchase.total}
            open={isOpen}
            void={returned}
            ontoggle={() => toggle(row.purchase.id)}
          >
            {#snippet who()}
              <span class="who-name">
                {#if supplierNames.get(row.purchase.supplier_id) === 'one-off-supplier'}
                  {lang.t.purchases_one_off_supplier}
                {:else}
                  {supplierNames.get(row.purchase.supplier_id) ?? lang.t.purchases_one_off_supplier}
                {/if}
              </span>
            {/snippet}

            {#snippet meta()}
              <span class="method" class:on-account={isOnAccount(row.purchase.payment_method_name)}
                >{methodLabel(row.purchase.payment_method_name, {
                  onAccount: lang.t.method_on_account,
                  unknown: lang.t.method_unknown
                })}</span
              >
              ·
              {#if returned}
                <Stamp label={lang.t.purchases_returned_state} tone="annulled" />
              {:else}
                <Stamp label={lang.t.purchases_confirmed_state} />
              {/if}
            {/snippet}

            {#snippet detail()}
              <table>
                <caption>{lang.t.purchases_lines_of} #{row.purchase.id.slice(0, 8)}</caption>
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
                    {@const remaining = returnableOf(item.id, item.quantity, refunded)}
                    <tr>
                      <td>{item.product_name}</td>
                      <td class="num figure">{formatQuantity(item.quantity, item.unit)}</td>
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

              <!-- The returns this purchase produced, oldest first. They sit
                   under the purchase and never replace it: the purchase keeps
                   every figure it was recorded with. -->
              {#each row.supplier_returns as supplierReturn (supplierReturn.return.id)}
                <section
                  class="return-block"
                  aria-label="{lang.t
                    .purchases_supplier_return} DR-#{supplierReturn.return.id.slice(0, 8)}"
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
                  <p role="status">{reloadNotice.get(row.purchase.id)}</p>
                {/if}
              </div>
            {/snippet}
          </DaySlip>
        {/each}
      </div>
      {#if hiddenColumns > 0}
        <!-- The count, not only the fade: a gradient says "there is more" without
             saying whether it is one column or five, and a reader cannot judge
             whether the swipe is worth it without the number. -->
        <p class="hint-more">
          {hiddenColumns}
          {hiddenColumns === 1 ? lang.t.more_column : lang.t.more_columns}
        </p>
      {/if}
    </div>
  {/if}

  <!-- The period's totals, from the server. Purchases and returns are separate
       facts: what the shop spent, and what suppliers gave back. A footer derived
       from the loaded page would stop at its own page size and call itself the
       period — the defect the sales ledger shipped once. -->
  <div class="footings">
    <div class="fig">
      <span class="label">{lang.t.purchases_footing_gross}</span>
      <span class="value figures">{summary === null ? '—' : formatMoney(summary.gross)}</span>
      <span class="note">{lang.t.purchases_footing_gross_note}</span>
    </div>
    <div class="fig">
      <span class="label">{lang.t.purchases_footing_returned}</span>
      <span class="value figures">{summary === null ? '—' : formatMoney(summary.returned)}</span>
      <span class="note">{lang.t.purchases_footing_returned_note}</span>
    </div>
    <div class="pager">
      <button
        type="button"
        onclick={() => stepPage(1)}
        disabled={!canStepPage(pageOffset, PAGE, total, 1)}
      >
        ← {lang.t.sales_older}
      </button>
      <span class="page-of figures">
        {lang.t.ledger_page_of(pageNumber(pageOffset, PAGE), pageCount(total, PAGE))}
      </span>
      <button
        type="button"
        onclick={() => stepPage(-1)}
        disabled={!canStepPage(pageOffset, PAGE, total, -1)}
      >
        {lang.t.sales_newer} →
      </button>
    </div>

    <div class="fig">
      <span class="label">{lang.t.purchases_footing_net}</span>
      <span class="value figures">{summary === null ? '—' : formatMoney(summary.net)}</span>
      <span class="note">{lang.t.purchases_footing_net_note}</span>
    </div>
  </div>
</section>

<style>
  /* A column, like the sales ledger: the header, the count and the footings are
     fixed, the table scrolls between them. Without the column and its
     `min-height: 0`, the table grew to its content's height and pushed the
     footings 3,291px down the page — measured, not guessed: the totals existed
     and no one could see them. */
  .ledger {
    padding: 18px 20px 20px;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
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
  .pager {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
  }
  .pager button {
    background: none;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    padding: 6px 12px;
    font: inherit;
    font-size: var(--step--1);
    color: var(--ink);
    cursor: pointer;
  }
  .pager button:disabled {
    color: var(--ink-soft);
    border-color: var(--rule);
    cursor: default;
  }
  .page-of {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }

  .count {
    color: var(--ink-soft);
    font-size: var(--step-0);
    padding: 16px 0 4px;
  }
  .error {
    color: var(--state-void);
    padding: 14px 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  /* The table frame scrolls its own rows and draws the affordance itself, the
     same mechanism the sales ledger uses: a fade at the cut edge and a count of
     what is below it, both of which survive a screenshot. */
  .table-frame {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-height: 0;
    margin-top: 10px;
  }
  .table-scroll {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: auto;
    min-width: 0;
    flex: 1 1 auto;
    min-height: 0;
    scrollbar-gutter: stable;
    scrollbar-width: auto;
    scrollbar-color: var(--ink-soft) var(--rule);
  }
  /* The sideways cut, made visible without a gesture — the same rule and the same
     reason as the catalog's: measured at 390px this frame reports
     `scrollWidth 438 > clientWidth 294` while drawing no bar, and the capture
     showed `ESTADO` cut to `EST`. */
  /* The mirror of the right fade: once the reader has scrolled, columns have
     left the view on the left side and nothing said so. It is keyed to
     `overflows-left`, not to `can-scroll`, so it appears only when something is
     actually out of view in that direction. */
  .table-scroll:global(.overflows-left)::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    bottom: 0;
    width: 28px;
    pointer-events: none;
    background: linear-gradient(to left, transparent, var(--surface) 72%);
  }
  .table-scroll:global(.overflows-x)::after {
    content: '';
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 28px;
    pointer-events: none;
    background: linear-gradient(to right, transparent, var(--surface) 72%);
  }
  .hint-more {
    margin: 6px 0 0;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-soft);
    text-align: right;
  }
  .who-name {
    display: block;
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .method {
    color: inherit;
    font-size: inherit;
    white-space: nowrap;
  }
  .method.on-account {
    font-style: italic;
  }
  .footings {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    flex-shrink: 0;
    margin-top: auto;
    gap: 0 20px;
    border-top: 1px solid var(--rule-strong);
    padding: 14px 18px 0 0;
  }
  .fig {
    display: grid;
    gap: 3px;
    align-content: start;
    padding-left: 20px;
    border-left: 1px solid var(--rule);
    min-width: 0;
  }
  .fig:first-child {
    padding-left: 0;
    border-left: 0;
  }
  .footings .label {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }
  .footings .value {
    font-size: var(--step-4);
    font-weight: 700;
    line-height: 1.1;
    white-space: nowrap;
  }
  .footings .note {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .figure {
    font-variant-numeric: tabular-nums;
    font-family: var(--font-figure);
  }
  :global(.lines) {
    padding: 4px 16px 10px;
  }
  :global(.lines) table {
    width: 100%;
    border-collapse: collapse;
    background: var(--ground);
    border: 1px solid var(--rule);
    border-radius: var(--radius);
  }
  :global(.lines) caption {
    caption-side: top;
    text-align: left;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-soft);
    padding: 0 0 5px;
  }
  :global(.lines) th {
    text-align: left;
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-soft);
    padding: 4px 8px;
    font-weight: 600;
  }
  :global(.lines) td {
    padding: 4px 8px;
    border-top: 1px solid var(--rule);
    font-size: var(--step-0);
  }
  :global(.lines) .num {
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
    background: var(--surface);
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
    background: var(--ground);
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
    background: var(--surface);
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
    border-color: var(--state-void);
    color: var(--state-void);
    background: color-mix(in srgb, var(--state-void) 8%, var(--surface));
  }
  .refund button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .refund [role='alert'] {
    flex-basis: 100%;
    color: var(--state-void);
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

  @media (max-width: 620px) {
    .footings {
      grid-template-columns: minmax(0, 1fr);
      gap: 12px;
      padding-right: 0;
    }
    .fig,
    .fig:first-child {
      padding-left: 12px;
      border-left: 1px solid var(--rule);
    }
  }
</style>
