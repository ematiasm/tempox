<script lang="ts">
  /**
   * The ledger: the day's documents, newest first.
   *
   * The composition puts the day on the page itself rather than behind a
   * separate view, so this reads as the book the sheet stands at the head of.
   *
   * The scope is a local calendar day, and the browser computes its bounds and
   * sends them explicitly (S8). The server stays timezone-agnostic and "today"
   * means the shop's today, not UTC's — a sale recorded at 22:00 in Argentina
   * belongs to that day's page, not to tomorrow's.
   */
  import {
    ApiError,
    createCreditNote,
    getCash,
    listSales,
    dayRange,
    type SaleListItem
  } from '#lib/api.js';
  import { formatMoney, formatQuantity, parseQuantity, sumAmounts } from '#lib/money.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import Stamp from './Stamp.svelte';

  interface Props {
    /** Changes whenever a sale is recorded, so the day reloads. */
    refreshToken?: number;
  }

  let { refreshToken = 0 }: Props = $props();

  const PAGE = 50;

  // `dayStart` is the local midnight the page is scoped to. Paging moves it by
  // whole local days rather than by an offset into a global list: the ledger is
  // a book of days, and "older" means yesterday's page, not row 51.
  let dayStart = $state(new Date());

  let rows = $state<SaleListItem[]>([]);
  let cash = $state<string | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  // `SvelteSet` rather than a plain `Set` in `$state`: it carries its own
  // reactivity, so `has`/`add`/`delete` register as reads and writes directly and
  // the expanded rows stay in step with the set without reassigning it.
  let expanded = new SvelteSet<string>();

  // The refund state, kept per document rather than globally: opening a second
  // document must not inherit the first one's half-filled form, and a confirm
  // that is open on one row must not arm another.
  // The value is the text the cashier typed, exactly as typed; validation
  // happens when they submit, not on every keystroke.
  let refundDraft = new SvelteMap<string, Map<string, string>>();
  let confirming = $state<string | null>(null);
  let refunding = $state<string | null>(null);
  let refundError = $state<string | null>(null);

  const range = $derived(dayRange(dayStart));
  /**
   * Whether the page is showing the shop's current day.
   *
   * Computed from calendar fields rather than by calling `setHours` on
   * `dayStart`: that method mutates the Date in place, so using it inside a
   * derived would silently rewrite the state the derived is reading, and the
   * scope controls would drift a day on every re-evaluation.
   */
  const isToday = $derived.by(() => {
    const now = new Date();
    return (
      dayStart.getFullYear() === now.getFullYear() &&
      dayStart.getMonth() === now.getMonth() &&
      dayStart.getDate() === now.getDate()
    );
  });

  const scopeLabel = $derived(
    dayStart.toLocaleDateString('en-US', { day: 'numeric', month: 'short', year: 'numeric' })
  );

  /** The rows the page actually holds — pagination shows no total count (S10). */
  const shownTotal = $derived(sumAmounts(rows.map((r) => r.total)) ?? '0.00');
  const refundedTotal = $derived(
    sumAmounts(rows.flatMap((row) => row.credit_notes.map((note) => note.total))) ?? '0.00'
  );
  const bonusLines = $derived(
    rows.reduce((n, r) => n + r.items.filter((i) => i.unit_price === '0.00').length, 0)
  );

  $effect(() => {
    // Read the reactive inputs so the effect re-runs when the day or the
    // refresh token changes.
    const { from, to } = range;
    void refreshToken;
    let cancelled = false;

    loading = true;
    error = null;
    // The bounds are sent explicitly; an absent bound would mean an unscoped
    // list, which is not what a day page shows.
    listSales({ from, to, limit: PAGE, offset: 0 })
      .then((r) => {
        if (cancelled) return;
        rows = r;
        expanded.clear();
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof ApiError ? `${e.message} (${e.status})` : String(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });

    // The cash figure is the shop's balance across every confirmed sale, not
    // the day's: it is labelled as such rather than implying a daily total.
    getCash()
      .then((c) => {
        if (!cancelled) cash = c.total;
      })
      .catch(() => {
        if (!cancelled) cash = null;
      });

    return () => {
      cancelled = true;
    };
  });

  function shiftDay(delta: number) {
    dayStart = new Date(dayStart.getFullYear(), dayStart.getMonth(), dayStart.getDate() + delta);
  }

  function toggle(id: string) {
    if (expanded.has(id)) expanded.delete(id);
    else expanded.add(id);
  }

  /** A cancelled sale is struck through, never crimson: it is a correction, not an alarm. */
  function isAnnulled(row: SaleListItem) {
    return row.status === 'cancel' || row.reversed_at !== null;
  }

  /**
   * How much of each line has already come back, keyed by sale-line id.
   *
   * Derived from the notes the server sent rather than tracked locally: the
   * server is the authority on what was refunded, and a local tally would drift
   * the moment a refund lands in another tab.
   */
  function refundedByLine(row: SaleListItem): Map<string, number> {
    const refunded = new SvelteMap<string, number>();
    for (const note of row.credit_notes) {
      for (const item of note.items) {
        refunded.set(item.sale_item_id, (refunded.get(item.sale_item_id) ?? 0) + item.quantity);
      }
    }
    return refunded;
  }

  /** What a line still has to return. Zero means it is done. */
  function refundableOf(itemId: string, sold: number, refunded: Map<string, number>): number {
    return Math.max(0, sold - (refunded.get(itemId) ?? 0));
  }

  /**
   * Whether the sale is fully reversed.
   *
   * A partial refund is not an annulment: the document still stands, wearing its
   * confirmed stamp, with a note underneath it. Only when every unit has come
   * back does the sale read as annulled — and even then it is derived, never a
   * status the server wrote on it.
   */
  function isFullyRefunded(row: SaleListItem): boolean {
    const refunded = refundedByLine(row);
    return (
      row.items.length > 0 &&
      row.items.every((item) => refundableOf(item.id, item.quantity, refunded) === 0)
    );
  }

  /** The draft refund text for one line: what was refunded sits above it as "left". */
  function refundText(rowId: string, itemId: string, remaining: number): string {
    return refundDraft.get(rowId)?.get(itemId) ?? String(remaining);
  }

  function setRefundText(rowId: string, itemId: string, text: string) {
    const draft = new SvelteMap(refundDraft.get(rowId) ?? []);
    draft.set(itemId, text);
    refundDraft.set(rowId, draft);
  }

  function clearRefundDraft(rowId: string) {
    refundDraft.delete(rowId);
  }

  /**
   * The lines this document would return, or `null` when the form is not valid.
   *
   * A line left at its full remaining quantity is the common case and needs no
   * special casing: the server enforces the limit, this only refuses to send a
   * quantity that is not a positive integer.
   */
  function refundPlan(row: SaleListItem): { sale_item_id: string; quantity: number }[] | null {
    const refunded = refundedByLine(row);
    const plan: { sale_item_id: string; quantity: number }[] = [];
    for (const item of row.items) {
      const remaining = refundableOf(item.id, item.quantity, refunded);
      if (remaining === 0) continue;
      const quantity = parseQuantity(refundText(row.id, item.id, remaining));
      if (quantity === null || quantity > remaining) return null;
      plan.push({ sale_item_id: item.id, quantity });
    }
    return plan.length > 0 ? plan : null;
  }

  async function submitRefund(row: SaleListItem) {
    const plan = refundPlan(row);
    if (plan === null) {
      refundError = lang.t.sales_refund_invalid;
      return;
    }
    refunding = row.id;
    refundError = null;
    try {
      await createCreditNote(row.id, plan);
      clearRefundDraft(row.id);
      confirming = null;
      // Reload the day rather than patching the row in place: the note the
      // server wrote is the record, and the derived stamped state has to come
      // from it, not from what this component believes it sent.
      rows = await listSales({ from: range.from, to: range.to, limit: PAGE, offset: 0 });
      const c = await getCash();
      cash = c.total;
    } catch (e) {
      // A 409 is the cumulative guard answering, and it is the one refusal a
      // cashier can act on: the line has less left than the form asked for.
      refundError = e instanceof ApiError ? `${e.message} (${e.status})` : String(e);
    } finally {
      refunding = null;
    }
  }
</script>

<section class="ledger" aria-labelledby="ledger-h">
  <div class="head">
    <div>
      <h2 id="ledger-h">{lang.t.sales_ledger_title}</h2>
      <div class="sub">{lang.t.sales_ledger_sub}</div>
    </div>
    <div class="scope">
      <div class="segmented" role="group" aria-label={lang.t.sales_scope}>
        <button type="button" aria-pressed={isToday} onclick={() => (dayStart = new Date())}>
          {lang.t.sales_today}
        </button>
        <button type="button" aria-pressed={!isToday} onclick={() => shiftDay(-1)}>
          {lang.t.sales_yesterday}
        </button>
      </div>
      <span class="date-chip figures">{scopeLabel}</span>
      <span class="pager">
        <button type="button" onclick={() => shiftDay(-1)}>
          ← {lang.t.sales_older}
        </button>
        <button type="button" onclick={() => shiftDay(1)} disabled={isToday}>
          {lang.t.sales_newer} →
        </button>
      </span>
    </div>
  </div>

  <div class="legend">
    <span class="item"><i class="swatch c"></i>{lang.t.sales_legend_confirmed}</span>
    <span class="item"><i class="swatch a"></i>{lang.t.sales_legend_annulled}</span>
    <span class="right">
      {#if loading}
        {lang.t.sales_loading}
      {:else}
        {rows.length}
        {rows.length === 1 ? lang.t.sales_document_one : lang.t.sales_documents}
      {/if}
    </span>
  </div>

  {#if error}
    <div class="alert" role="alert">
      <strong>{lang.t.sales_ledger_error}</strong>
      <span>{error}</span>
      <button type="button" onclick={() => (refreshToken = refreshToken + 1)}>
        {lang.t.sales_retry}
      </button>
    </div>
  {:else if loading}
    <!-- Bars, not a spinner: the header, the scope controls and the legend stay
         put, so the page does not jump while the day arrives. -->
    <div class="skeleton" aria-hidden="true">
      <div class="bar"></div>
      <div class="bar"></div>
      <div class="bar"></div>
      <div class="bar"></div>
    </div>
  {:else if rows.length === 0}
    <div class="empty">
      <p>{isToday ? lang.t.sales_no_sales_today : lang.t.sales_no_sales_day}</p>
      <div class="pager">
        <button type="button" onclick={() => shiftDay(-1)}>← {lang.t.sales_older}</button>
        <button type="button" onclick={() => shiftDay(1)} disabled={isToday}>
          {lang.t.sales_newer} →
        </button>
      </div>
    </div>
  {:else}
    <div class="table-scroll">
      <table class="docs">
        <thead>
          <tr>
            <th scope="col">{lang.t.sales_time}</th>
            <th scope="col">{lang.t.sales_sale}</th>
            <th scope="col">{lang.t.sales_customer}</th>
            <th scope="col" class="num">{lang.t.sales_items}</th>
            <th scope="col">{lang.t.sales_status}</th>
            <th scope="col" class="num">{lang.t.sales_total}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            {@const open = expanded.has(row.id)}
            {@const annulled = isAnnulled(row) || isFullyRefunded(row)}
            <tr class="doc">
              <td class="figures time">
                {new Date(row.created_at).toLocaleTimeString('en-US', {
                  hour: '2-digit',
                  minute: '2-digit',
                  hour12: false
                })}
              </td>
              <td>
                <button
                  type="button"
                  class="doc-id"
                  aria-expanded={open}
                  aria-controls="lines-{row.id}"
                  onclick={() => toggle(row.id)}
                >
                  <svg
                    class="chev"
                    class:open
                    viewBox="0 0 12 12"
                    aria-hidden="true"
                    focusable="false"
                  >
                    <path
                      d="M4.5 2.5 8 6l-3.5 3.5"
                      fill="none"
                      stroke="currentColor"
                      stroke-width="1.8"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                    />
                  </svg>
                  <span class="figures">#{row.id.slice(0, 8)}</span>
                </button>
              </td>
              <td class="customer" class:walkin={row.customer_name === 'walk-in'}>
                {row.customer_name === 'walk-in' ? lang.t.sales_walkin : row.customer_name}
              </td>
              <td class="num figures items">{row.items.length}</td>
              <td>
                {#if annulled}
                  <Stamp label={lang.t.sales_annulled} tone="annulled" size="cell" />
                {:else}
                  <Stamp label={lang.t.sales_confirmed} size="cell" />
                {/if}
              </td>
              <td class="num">
                <span class="total figures" class:void={annulled}>{formatMoney(row.total)}</span>
              </td>
            </tr>
            {#if open}
              <tr class="lines-row">
                <td colspan="6">
                  <div class="lines" id="lines-{row.id}">
                    <table>
                      <caption>{lang.t.sales_lines_of} #{row.id.slice(0, 8)}</caption>
                      <thead>
                        <tr>
                          <th scope="col">{lang.t.sales_product}</th>
                          <th scope="col" class="num">{lang.t.sales_qty}</th>
                          <th scope="col" class="num">{lang.t.sales_unit_price}</th>
                          <th scope="col" class="num">{lang.t.sales_line}</th>
                          <th scope="col" class="num">{lang.t.sales_refund_qty}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {#each row.items as item (item.id)}
                          {@const refunded = refundedByLine(row)}
                          {@const remaining = refundableOf(item.id, item.quantity, refunded)}
                          <tr>
                            <td>
                              {item.product_name}
                              {#if item.unit_price === '0.00'}
                                <span class="gift">— {lang.t.sales_bonus}</span>
                              {/if}
                            </td>
                            <td class="num figures">{formatQuantity(item.quantity, item.unit)}</td>
                            <td class="num figures">
                              {formatMoney(item.unit_price)}<span class="per">/{item.unit}</span>
                            </td>
                            <td class="num figures">{formatMoney(item.line_total)}</td>
                            <td class="num return-cell">
                              {#if remaining > 0}
                                <input
                                  type="text"
                                  inputmode="numeric"
                                  aria-label="Return {item.product_name}"
                                  value={refundText(row.id, item.id, remaining)}
                                  oninput={(event) =>
                                    setRefundText(row.id, item.id, event.currentTarget.value)}
                                />
                              {:else}
                                <span class="returned">{lang.t.sales_refund_none}</span>
                              {/if}
                              <span class="left"
                                >{lang.t.sales_refund_left(remaining, item.quantity)}</span
                              >
                            </td>
                          </tr>
                        {/each}
                      </tbody>
                    </table>
                    {#each row.credit_notes as note (note.id)}
                      <section
                        class="note-block"
                        aria-label="{lang.t.sales_credit_note} NC-#{note.id.slice(0, 8)}"
                      >
                        <div class="note-head">
                          <span>{lang.t.sales_credit_note} NC-#{note.id.slice(0, 8)}</span>
                          <span class="note-total figures">−{formatMoney(note.total)}</span>
                          <time
                            >{new Date(note.created_at).toLocaleTimeString('en-US', {
                              hour: '2-digit',
                              minute: '2-digit',
                              hour12: false
                            })}</time
                          >
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
                            {#each note.items as item (item.id)}
                              <tr>
                                <td>{item.product_name}</td>
                                <td class="num figures"
                                  >{formatQuantity(item.quantity, item.unit)}</td
                                >
                                <td class="num figures"
                                  >{formatMoney(item.unit_price)}<span class="per"
                                    >/{item.unit}</span
                                  ></td
                                >
                                <td class="num figures">{formatMoney(item.line_total)}</td>
                              </tr>
                            {/each}
                          </tbody>
                        </table>
                      </section>
                    {/each}
                    <div class="refund">
                      {#if isFullyRefunded(row)}
                        <p>{lang.t.sales_refund_none}</p>
                      {:else if confirming === row.id}
                        <p class="refund-warn">{lang.t.sales_refund_confirm}</p>
                        <button
                          type="button"
                          class="danger"
                          disabled={refunding === row.id}
                          onclick={() => submitRefund(row)}
                        >
                          {refunding === row.id ? '…' : lang.t.sales_refund_yes}
                        </button>
                        <button type="button" onclick={() => (confirming = null)}
                          >{lang.t.sales_refund_no}</button
                        >
                      {:else}
                        <button
                          type="button"
                          class="danger"
                          onclick={() => {
                            confirming = row.id;
                            refundError = null;
                          }}
                        >
                          {lang.t.sales_refund_action}
                        </button>
                      {/if}
                      {#if refundError}
                        <p role="alert">{lang.t.sales_refund_error}{refundError}</p>
                      {/if}
                    </div>
                  </div>
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <!-- The cash figures close the page the way the footings close a column. -->
  <div class="footings">
    <div class="fig">
      <span class="label">{lang.t.sales_cash_on_hand}</span>
      <span class="value figures">{cash === null ? '—' : formatMoney(cash)}</span>
      <span class="note">{lang.t.sales_cash_note}</span>
    </div>
    <div class="fig">
      <span class="label">{lang.t.sales_shown_sales}</span>
      <span class="value figures">{formatMoney(shownTotal)}</span>
      <span class="note">{lang.t.sales_shown_note}</span>
    </div>
    <div class="fig">
      <span class="label">{lang.t.sales_bonus_lines}</span>
      <span class="value figures">{bonusLines}</span>
      <span class="note">{lang.t.sales_bonus_lines_note}</span>
    </div>
    <div class="fig">
      <span class="label">{lang.t.sales_refunded}</span>
      <span class="value figures">{formatMoney(refundedTotal)}</span>
      <span class="note">{lang.t.sales_refunded_note}</span>
    </div>
  </div>
</section>

<style>
  .ledger {
    padding: 0 0 30px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 20px;
    padding-bottom: 12px;
  }
  .head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.05;
  }
  .head .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 3px;
  }

  .scope {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .segmented {
    display: inline-flex;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .segmented button {
    background: var(--paper-raised);
    border: 0;
    border-right: 1px solid var(--rule);
    padding: 7px 12px;
    font-size: var(--step-0);
    font-weight: 600;
    color: var(--indigo);
    cursor: pointer;
  }
  .segmented button:last-child {
    border-right: 0;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--indigo);
    color: #f2efe6;
  }
  .date-chip {
    border: 1px solid var(--rule-strong);
    background: var(--paper-raised);
    border-radius: var(--radius);
    padding: 6px 10px;
    font-size: var(--step-2);
    font-weight: 600;
  }
  .pager {
    display: inline-flex;
    gap: 6px;
  }
  .pager button {
    border: 1px solid var(--rule-strong);
    background: var(--paper-raised);
    border-radius: var(--radius);
    padding: 7px 11px;
    font-size: var(--step-0);
    font-weight: 600;
    color: var(--indigo);
    cursor: pointer;
  }
  .pager button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .legend {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 9px 12px;
    border: 1px solid var(--rule);
    border-bottom: 0;
    border-radius: 4px 4px 0 0;
    background: var(--paper-raised);
    font-size: var(--step-0);
    color: var(--ink-soft);
  }
  .legend .item {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .legend .swatch {
    width: 10px;
    height: 10px;
    border-radius: 1px;
  }
  .legend .c {
    background: var(--crimson);
  }
  .legend .a {
    background: var(--ink);
  }
  .legend .right {
    margin-left: auto;
  }

  .table-scroll {
    /* The ledger is a wide document. Below the point where its columns stop
       fitting, it scrolls inside its own frame instead of making the whole page
       scroll sideways — a page that pans horizontally under a thumb is how a
       shop employee loses the column they were reading. */
    overflow-x: auto;
    border-radius: 0 0 4px 4px;
  }
  table.docs {
    width: 100%;
    min-width: 640px;
    border-collapse: collapse;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 0 0 4px 4px;
  }
  .docs th {
    font-family: var(--font-figure);
    font-size: var(--step-1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--indigo-soft);
    text-align: left;
    padding: 10px 12px;
    border-bottom: 1px solid var(--rule-strong);
    white-space: nowrap;
  }
  .docs th:first-child,
  .docs > tbody > tr > td:first-child {
    padding-left: 16px;
    /* A hairline of the same line system: the margin rule of a ruled page,
       never a decorative rail. */
    border-left: 1px solid rgb(168 27 42 / 0.5);
  }
  .docs th:last-child,
  .docs > tbody > tr > td:last-child {
    padding-right: 16px;
  }
  .docs td {
    padding: 10px 12px;
    border-bottom: 1px solid var(--rule);
    vertical-align: middle;
  }
  .doc:hover {
    background: rgb(43 49 98 / 0.035);
  }
  .num {
    text-align: right;
  }
  .time {
    font-size: var(--step-2);
  }

  .doc-id {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: 0;
    padding: 2px 0;
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--indigo);
    cursor: pointer;
  }
  .chev {
    width: 12px;
    height: 12px;
    transition: transform 120ms ease-out;
  }
  .chev.open {
    transform: rotate(90deg);
  }

  .customer {
    color: var(--ink);
  }
  .customer.walkin {
    color: var(--ink-soft);
    font-style: italic;
  }
  .items {
    color: var(--ink-soft);
  }
  .total {
    font-size: var(--step-3);
    font-weight: 700;
    white-space: nowrap;
  }
  .total.void {
    color: var(--ink-soft);
    text-decoration: line-through;
    text-decoration-thickness: 1.5px;
  }

  .lines-row td {
    padding: 0;
    border-bottom: 1px solid var(--rule-strong);
  }
  .lines {
    padding: 4px 16px 10px;
  }
  .lines table {
    width: 100%;
    border-collapse: collapse;
    background: var(--paper);
    border: 1px solid var(--rule);
    border-radius: var(--radius);
  }
  .lines caption {
    caption-side: top;
    text-align: left;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--indigo-soft);
    padding: 0 0 5px;
  }
  .lines th {
    font-size: var(--step--1);
    padding: 6px 10px;
    border-bottom: 1px solid var(--rule);
  }
  .lines td {
    padding: 6px 10px;
    border-bottom: 1px solid var(--rule);
    font-size: var(--step-1);
  }
  .lines tr:last-child td {
    border-bottom: 0;
  }
  .lines .per {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .gift {
    color: var(--crimson);
    font-weight: 600;
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
  /*
   * An annulment is a correction, not an alarm (DESIGN.md, "The Annulled Is Ink
   * Rule"). So the control is ink, like every other control on the page, and
   * crimson arrives only on hover — the same shape the sheet's remove control
   * uses. A crimson fill here would be a third role for the one accent, spent
   * against the stamp it is meant to belong to.
   */
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
  .note-block {
    margin-top: 18px;
    padding-top: 12px;
    border-top: 1px solid var(--rule-strong);
  }
  .note-head {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: baseline;
    gap: 14px;
    margin-bottom: 7px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .note-total {
    text-align: right;
    font-size: var(--step-2);
    font-weight: 700;
    color: var(--ink);
  }
  .note-block table {
    width: 100%;
    border-collapse: collapse;
    background: var(--paper);
    border: 1px solid var(--rule);
  }
  .note-block th {
    font-size: var(--step--1);
    padding: 6px 10px;
    border-bottom: 1px solid var(--rule);
  }
  .note-block td {
    padding: 6px 10px;
    border-bottom: 1px solid var(--rule);
    font-size: var(--step-1);
  }

  .alert {
    display: flex;
    align-items: center;
    gap: 10px;
    background: rgb(168 27 42 / 0.06);
    border: 1px solid rgb(168 27 42 / 0.4);
    border-radius: var(--radius);
    padding: 12px 14px;
    font-size: var(--step-0);
  }
  .alert strong {
    font-family: var(--font-figure);
    color: var(--crimson-deep);
  }
  .alert button {
    margin-left: auto;
    border: 1px solid var(--crimson-deep);
    background: transparent;
    color: var(--crimson-deep);
    font-weight: 600;
    font-size: var(--step-0);
    border-radius: var(--radius);
    padding: 6px 11px;
    cursor: pointer;
  }

  .skeleton {
    display: grid;
    gap: 10px;
    padding: 18px 16px;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 0 0 4px 4px;
  }
  .bar {
    height: 12px;
    border-radius: 2px;
    background: linear-gradient(
      90deg,
      rgb(43 49 98 / 0.09),
      rgb(43 49 98 / 0.17),
      rgb(43 49 98 / 0.09)
    );
    background-size: 200% 100%;
    animation: sweep 1.4s linear infinite;
  }
  .bar:nth-child(1) {
    width: 62%;
  }
  .bar:nth-child(2) {
    width: 84%;
  }
  .bar:nth-child(3) {
    width: 74%;
  }
  .bar:nth-child(4) {
    width: 48%;
  }
  @keyframes sweep {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -100% 0;
    }
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 14px;
    padding: 34px 18px;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 0 0 4px 4px;
    color: var(--ink-soft);
    font-size: var(--step-1);
  }

  .footings {
    display: grid;
    grid-template-columns: 1.2fr 1fr 1fr 1fr;
    gap: 0;
    border-top: 1px solid var(--rule-strong);
    margin-top: 18px;
  }
  .fig {
    display: grid;
    gap: 3px;
    padding: 14px 16px 0 0;
    border-right: 1px solid var(--rule);
  }
  .fig:last-child {
    border-right: 0;
    padding-right: 0;
  }
  .value {
    font-size: var(--step-4);
    font-weight: 700;
    line-height: 1.1;
  }
  .note {
    color: var(--ink-soft);
    font-size: var(--step--1);
    line-height: 1.35;
    max-width: 30ch;
  }

  @media (prefers-reduced-motion: reduce) {
    .bar {
      animation: none;
    }
    .chev {
      transition: none;
    }
  }

  @media (max-width: 1180px) {
    .head {
      flex-direction: column;
      align-items: flex-start;
    }
    .footings {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .fig {
      padding-bottom: 14px;
    }
  }
</style>
