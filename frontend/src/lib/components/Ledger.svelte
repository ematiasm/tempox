<script lang="ts">
  /**
   * The ledger: the day's documents, newest first.
   *
   * The composition puts the day on the page itself rather than behind a
   * separate view, so this reads as the book the sheet stands at the head of.
   *
   * The scope is an instant interpreted by the server in the shop's timezone
   * (S8), so a sale belongs to the shop-local day rather than the browser's.
   */
  import {
    ApiError,
    createCreditNote,
    getCash,
    listSales,
    fetchDay,
    type DayRange,
    type SaleListItem
  } from '#lib/api.js';
  import { formatDay, formatMoney, formatQuantity, formatTime, sumAmounts } from '#lib/money.js';
  import {
    isFullyRefunded as saleIsFullyRefunded,
    parseRefundQuantity,
    planRefund,
    refundableOf as saleRefundableOf,
    refundedByLine as saleRefundedByLine
  } from '#lib/salesRefund.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { createStaleNotice } from '#lib/screenTrust.js';
  import { runningBalances } from '#lib/runningBalance.js';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import Stamp from './Stamp.svelte';

  interface Props {
    /** Changes whenever a sale is recorded, so the day reloads. */
    refreshToken?: number;
  }

  let { refreshToken = 0 }: Props = $props();

  const PAGE = 50;

  // Keep the instant the page is scoped to; the server resolves its local day.
  let scopeInstant = $state(new Date());
  let range = $state<DayRange | null>(null);
  let todayFrom = $state<string | null>(null);
  // The shop's own zone, as the server names it. Every time on the page is
  // rendered in it: the day is the shop's (S8), so its hours must be too (S19).
  let shopZone = $state<string | undefined>(undefined);

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
  // Which lines the cashier chose to include. Stored as the choice rather than
  // as the default, so a line the form has never seen reads as selected: a
  // refund starts with every line that still has something left, and unchecking
  // is the deliberate act. Without this, a line left untouched was submitted at
  // its full remaining amount -- stock and money moving on a line nobody chose.
  let refundSelected = new SvelteMap<string, Map<string, boolean>>();
  let confirming = $state<string | null>(null);
  let refunding = $state<string | null>(null);
  let refundError = $state<string | null>(null);
  // Set when the credit note was written and only the day's reload failed. Kept
  // apart from `refundError` on purpose: the two carry opposite meanings, and the
  // cashier acts differently on each. The lifetime rule lives in
  // `screenTrust.ts`: a successful reload retires every notice, because the
  // figures are current and a warning saying otherwise is a lie. Backed by a
  // `SvelteMap` so retiring it re-renders.
  const refundStale = createStaleNotice(new SvelteMap<string, string>());

  // The server's current-day boundary is the sole authority for this state.
  const isToday = $derived(range !== null && todayFrom !== null && range.from === todayFrom);

  // The chip and the times carry the active language (S6). They read from the
  // same formatters the money does, so a language switch cannot leave the dates
  // behind in the previous one.
  // The chip names the day the server resolved, not the one the browser thinks it
  // is. `range.from` is that day's first instant and `shopZone` says which clock
  // to read it on; using `scopeInstant` here labels the page with the machine's
  // date, which is a day late on a UTC browser (S19).
  const scopeLabel = $derived(
    range ? formatDay(new Date(range.from), shopZone) : formatDay(scopeInstant, shopZone)
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
    const at = scopeInstant;
    void refreshToken;
    let cancelled = false;

    loading = true;
    error = null;
    // Both reads have to succeed before this screen counts as current, and they
    // are independent requests that run in parallel: retiring the notice on the
    // sales read alone cleared the warning while the cash figure was still stale.
    // A retry where `listSales` succeeds and `getCash` fails is exactly that case.
    let salesFresh = false;
    let cashFresh = false;
    const retireIfCurrent = () => {
      if (cancelled || !salesFresh || !cashFresh) return;
      refundStale.retireAll();
    };
    // Resolve both scoped and current-day boundaries through the server before
    // requesting sales; a failed day lookup must never invent a client range.
    Promise.all([fetchDay(at), fetchDay(new Date())])
      .then(async ([day, today]) => {
        if (cancelled) return;
        range = day;
        todayFrom = today.from;
        shopZone = day.zone;
        const sales = await listSales({ from: day.from, to: day.to, limit: PAGE, offset: 0 });
        if (cancelled) return;
        rows = sales;
        expanded.clear();
        // The figures are current again, so nothing on this screen is stale. A
        // warning here would contradict the data beside it, and an operator who
        // learns to ignore it will not read it on the day it is true. This is the
        // rule the first implementation was missing: it only ever retired a
        // notice on a click that happened to touch that same row.
        salesFresh = true;
        retireIfCurrent();
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
        if (cancelled) return;
        cash = c.total;
        cashFresh = true;
        retireIfCurrent();
      })
      .catch(() => {
        // A cash read that fails leaves the balance visually unknown but does not
        // discredit the day's rows, so it is not turned into a page error. It does
        // keep the stale notice alive, because the figures are not all current.
        if (!cancelled) cash = null;
      });

    return () => {
      cancelled = true;
    };
  });

  /**
   * Pages to the adjacent day the *server* named.
   *
   * The old version shifted the instant by one local calendar day on the
   * browser's clock, which is not one shop day: in a zone whose DST change
   * crosses midnight (Santiago, Havana) a single click could skip a day or land
   * two back. `range.prev` and `range.next` are already the neighbouring days'
   * first instants, computed by the same calendar that scoped the page.
   */
  function shiftDay(delta: number) {
    if (!range) return;
    const target = delta < 0 ? range.prev : range.next;
    scopeInstant = new Date(target);
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
   * the moment a refund lands in another tab. The arithmetic itself lives in
   * `salesRefund.ts`, where a unit test can reach it -- see that module's header
   * for what keeping it here cost.
   */
  function refundedByLine(row: SaleListItem): Map<string, number> {
    return saleRefundedByLine(row.credit_notes);
  }

  /** What a line still has to return. Zero means it is done. */
  function refundableOf(itemId: string, sold: number, refunded: Map<string, number>): number {
    return saleRefundableOf(itemId, sold, refunded);
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
    return saleIsFullyRefunded(row.items, row.credit_notes);
  }

  /**
   * The running balance each row carries, keyed by sale id.
   *   * The ledger lists newest first; a statement reads down from the oldest, so
   * the column is computed over the reversed rows and handed back keyed by id.
   * The arithmetic itself lives in `runningBalance.ts`, where a unit test can
   * reach it — a column that merely repeats each row's own total looks correct
   * in a screenshot and is wrong on paper.
   *
   * A cancelled sale contributes nothing but still inherits the balance above
   * it, which is why this is keyed rather than positional: the row stays
   * visible and struck through, and its cell reads the same number as the row
   * before it.
   */
  const balances = $derived.by(() => {
    const oldestFirst = [...rows].reverse();
    const computed = runningBalances(
      oldestFirst.map((row) => ({
        total: row.total,
        counts: !(isAnnulled(row) || isFullyRefunded(row))
      }))
    );
    // `SvelteMap`, not `Map`: the column must recompute when the day reloads.
    // A plain Map inside `$derived.by` is rebuilt on every recompute anyway, but
    // the lint rule is right about the class — a mutable Map in Svelte state is
    // the shape that silently stops updating when it is hoisted out of a
    // derived, and the next person to touch this will not know which one it is.
    const byId = new SvelteMap<string, string>();
    oldestFirst.forEach((row, index) => byId.set(row.id, computed[index]));
    return byId;
  });

  /** The draft refund text for one line: what was refunded sits above it as "left". */
  function refundText(rowId: string, itemId: string, remaining: number): string {
    return refundDraft.get(rowId)?.get(itemId) ?? String(remaining);
  }

  function setRefundText(rowId: string, itemId: string, text: string) {
    const draft = new SvelteMap(refundDraft.get(rowId) ?? []);
    draft.set(itemId, text);
    refundDraft.set(rowId, draft);
  }

  /**
   * Whether a line is included in the refund. A line the form has not stored a
   * choice for reads as selected: the form starts with every line that still has
   * something left, and unchecking is the deliberate act.
   */
  function isRefundSelected(rowId: string, itemId: string): boolean {
    return refundSelected.get(rowId)?.get(itemId) ?? true;
  }

  function setRefundSelected(rowId: string, itemId: string, selected: boolean) {
    const choice = new SvelteMap(refundSelected.get(rowId) ?? []);
    choice.set(itemId, selected);
    refundSelected.set(rowId, choice);
  }

  function clearRefundDraft(rowId: string) {
    refundDraft.delete(rowId);
    refundSelected.delete(rowId);
  }

  /**
   * The lines this document would return, or `null` when the form is not valid.
   *
   * A line left at its full remaining quantity is the common case and needs no
   * special casing: the server enforces the limit, this only refuses to send a
   * quantity that is not a positive integer. What it does refuse to do is send a
   * line the cashier unchecked -- that omission is the whole point of the
   * checkbox, and `planRefund` owns the rule so a unit test can pin it.
   */
  function refundPlan(row: SaleListItem): { sale_item_id: string; quantity: number }[] | null {
    const refunded = refundedByLine(row);
    const selected = new SvelteMap<string, boolean>();
    const requested = new SvelteMap<string, number | null>();
    for (const item of row.items) {
      selected.set(item.id, isRefundSelected(row.id, item.id));
      const remaining = refundableOf(item.id, item.quantity, refunded);
      if (remaining === 0) continue;
      requested.set(item.id, parseRefundQuantity(refundText(row.id, item.id, remaining)));
    }
    return planRefund(row.items, refunded, selected, requested);
  }

  async function submitRefund(row: SaleListItem) {
    const plan = refundPlan(row);
    if (plan === null) {
      refundError = lang.t.sales_refund_invalid;
      return;
    }
    if (!range) return;
    refunding = row.id;
    refundError = null;
    refundStale.forget(row.id);
    // The note and the reload are separate failures. Once `createCreditNote`
    // answers 201 the stock and the money have moved; a reload that fails after
    // that is a stale screen, not a failed annulment. Reporting both through the
    // same "Not annulled" alert told the cashier a note did not exist when it did,
    // and the retry it invited wrote a second partial note on top of the first.
    try {
      await createCreditNote(row.id, plan);
    } catch (e) {
      // A 409 is the cumulative guard answering, and it is the one refusal a
      // cashier can act on: the line has less left than the form asked for.
      refundError = e instanceof ApiError ? `${e.message} (${e.status})` : String(e);
      refunding = null;
      return;
    }
    clearRefundDraft(row.id);
    confirming = null;
    // Reload the day rather than patching the row in place: the note the server
    // wrote is the record, and the derived stamped state has to come from it, not
    // from what this component believes it sent.
    try {
      rows = await listSales({ from: range.from, to: range.to, limit: PAGE, offset: 0 });
      const c = await getCash();
      cash = c.total;
      // This is a successful reload of the whole day, so the same rule applies as
      // in the effect above: nothing on this screen is stale any more. The rule
      // lives in one place (`screenTrust.ts`) precisely so both success paths
      // honour it -- the first version only retired notices on the effect path,
      // which would have left a warning standing over figures that had just been
      // re-read.
      refundStale.retireAll();
    } catch (e) {
      // The note stands; only this screen is behind. It is said as that, and the
      // confirm is closed, so the cashier does not read it as a refund to redo.
      refundStale.mark(row.id, e instanceof ApiError ? `${e.message} (${e.status})` : String(e));
    }
    refunding = null;
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
        <button type="button" aria-pressed={isToday} onclick={() => (scopeInstant = new Date())}>
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

  <!-- One retry for the whole screen, for the state the alert below cannot reach.
       A failed reload after a written credit note sets the stale notice and never
       `error`, so the alert's own retry was never rendered in that state: the
       warning told the cashier the figures were stale with no way to refresh
       them. One control here serves every row, because re-reading the day is the
       same action regardless of which document exposed it. -->
  {#if !error && !refundStale.empty}
    <div class="alert" role="status">
      <strong>{lang.t.sales_stale_title}</strong>
      <button type="button" onclick={() => (refreshToken = refreshToken + 1)}>
        {lang.t.sales_retry}
      </button>
    </div>
  {/if}

  <!-- Mutually exclusive with the loading, empty and table states, as it was
       before: a failed read must replace the ledger region, never sit beside an
       empty-state paragraph claiming there are no sales, or beside stale rows. -->
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
            <th scope="col" class="balance-col">{lang.t.sales_running_balance}</th>
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
              <!-- The running balance, in the margin where a printed statement
                   puts it. It is the design's signature: a column that reads as
                   one continuous number down the page. A cancelled document
                   shows the balance it inherits, struck through with the rest
                   of its row, because money that never moved cannot appear in
                   a running total. -->
              <td class="figures balance" class:void={annulled}>
                {formatMoney(balances.get(row.id) ?? row.total)}
              </td>
              <td class="figures time">
                {formatTime(new Date(row.created_at), shopZone)}
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
                <td colspan="7">
                  <div class="lines" id="lines-{row.id}">
                    <table>
                      <caption>{lang.t.sales_lines_of} #{row.id.slice(0, 8)}</caption>
                      <thead>
                        <tr>
                          <th scope="col" class="pick">
                            <span class="sr-only">{lang.t.sales_refund_include_col}</span>
                          </th>
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
                            <td class="pick">
                              {#if remaining > 0}
                                <input
                                  type="checkbox"
                                  aria-label={lang.t.sales_refund_include(item.product_name)}
                                  checked={isRefundSelected(row.id, item.id)}
                                  onchange={(event) =>
                                    setRefundSelected(row.id, item.id, event.currentTarget.checked)}
                                />
                              {/if}
                            </td>
                            <td>
                              {item.product_name}
                              {#if item.unit_price === '0.00'}
                                <span class="gift">— {lang.t.sales_bonus}</span>
                              {/if}
                            </td>
                            <td class="num figures">{formatQuantity(item.quantity, item.unit)}</td>
                            <td class="num figures">
                              {formatMoney(item.unit_price)}<span class="per"
                                >/{lang.fmt.unit(item.unit)}</span
                              >
                            </td>
                            <td class="num figures">{formatMoney(item.line_total)}</td>
                            <td class="num return-cell">
                              {#if remaining > 0}
                                {@const picked = isRefundSelected(row.id, item.id)}
                                <input
                                  type="text"
                                  inputmode="numeric"
                                  aria-label={lang.t.sales_return_line(item.product_name)}
                                  value={refundText(row.id, item.id, remaining)}
                                  disabled={!picked}
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
                          <time>{formatTime(new Date(note.created_at), shopZone)}</time>
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
                                    >/{lang.fmt.unit(item.unit)}</span
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
                            // Deliberately NOT forgetting the stale notice here. Arming
                            // the confirm is not "the cashier dealt with this
                            // document": they may still cancel, and dropping the
                            // notice would leave the figures stale with neither a
                            // warning nor the retry that refreshes them. Only a
                            // reload that actually succeeds retires it.
                          }}
                        >
                          {lang.t.sales_refund_action}
                        </button>
                      {/if}
                      {#if refundError}
                        <p role="alert">{lang.t.sales_refund_error}{refundError}</p>
                      {/if}
                      {#if refundStale.get(row.id)}
                        <p class="refund-stale" role="status">
                          {lang.t.sales_refund_stale}{refundStale.get(row.id)}
                        </p>
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
      <span class="note">{lang.t.sales_bonus_lines_note(formatMoney('0.00'))}</span>
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
  /* The running-balance column: the design's signature move, set in the margin
     the way a printed statement puts it. It is right-aligned like every other
     figure so the digits line up down the page and the column reads as one
     continuous number rather than six unrelated ones. */
  .balance-col,
  .balance {
    text-align: right;
    white-space: nowrap;
    font-weight: 600;
    color: var(--accent);
    padding-right: 16px !important;
    border-right: 1px solid var(--rule);
  }
  .balance.void {
    /* A cancelled document never moved money. Its balance still shows what it
       inherits, struck through with the rest of the row. */
    color: var(--ink-soft);
    text-decoration: line-through;
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
  .pick {
    width: 1.5em;
    padding-right: 0;
  }
  .pick input {
    width: 1.05em;
    height: 1.05em;
    accent-color: var(--indigo, var(--ink));
    cursor: pointer;
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
  /* An excluded line is not gone, it is set aside: the cashier can change their
     mind, so the field stays readable while it stops being an input. */
  .return-cell input:disabled {
    background: var(--paper);
    border-style: dashed;
    color: var(--ink-soft);
    cursor: not-allowed;
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
  .refund-stale {
    flex-basis: 100%;
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
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
