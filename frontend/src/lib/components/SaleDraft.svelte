<script lang="ts">
  /**
   * The draft sheet: the sale being typed, standing at the head of the book.
   *
   * It is deliberately not a modal. Recording a sale is the shop's primary
   * action and the ledger stays visible behind it, because the cashier checks
   * what just landed against what they are typing.
   *
   * The live draft preview is summed as integer cents internally; the recorded
   * total always comes from the server response.
   */
  import {
    createSale,
    listCustomers,
    listPaymentMethods,
    findOrCreateCustomer,
    type Customer,
    type PaymentMethod,
    type Product
  } from '#lib/api.js';
  import { formatMoney, lineTotalAmount, parseAmount, parseQuantity } from '#lib/money.js';
  import {
    bonusCountOf,
    canRecord as canRecordNow,
    completeLines,
    draftTotal,
    hasInvalidLine,
    unitCountOf
  } from '#lib/draftLines.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import ProductPicker from './ProductPicker.svelte';
  import Stamp from './Stamp.svelte';

  interface Props {
    /** Called after a sale is recorded, so the ledger can refresh. */
    onrecorded?: (saleId: string) => void;
    /**
     * Reports whether the sheet currently holds something recordable.
     *
     * The band owns the page's single next action, and the band lives in the
     * route, so the sheet has to say when that action is available rather than
     * keeping the button to itself.
     */
    onready?: (ready: boolean) => void;
  }

  let { onrecorded, onready }: Props = $props();

  // One line being composed: the product, plus the two fields as typed text.
  // The text is the source of truth for the field's own validity; the parsed
  // numbers only exist once they parse.
  type Line = {
    id: string;
    product: Product;
    qtyText: string;
    priceText: string;
  };

  let nextLineId = 0;

  let lines = $state<Line[]>([]);
  let customers = $state<Customer[]>([]);
  let customerId = $state<string>('');
  /**
   * The methods the counter can offer, and the one chosen.
   *
   * Empty `paymentMethodId` means "the server's default", which is the seeded cash
   * method — the same thing an older client sends by omitting the field. Only
   * **active** methods are offered: `is_active` is a picker concern by design, and
   * the API would accept an inactive one, which is exactly why the screen is where
   * a retired method must disappear.
   */
  let methods = $state<PaymentMethod[]>([]);
  let paymentMethodId = $state<string>('');
  /** A failed method read: the picker falls back to the server's default and says so. */
  let methodsFailed = $state(false);
  let submitting = $state(false);
  let error = $state<string | null>(null);
  /** Set when a name already existed and was selected instead of created (S7). */
  let notice = $state<string | null>(null);
  let justRecorded = $state<{ id: string; total: string } | null>(null);
  let creatingCustomer = $state(false);
  let newCustomerName = $state('');

  let lastQtyEl = $state<HTMLInputElement | undefined>(undefined);

  // The seeded walk-in is the default customer: most sales in a shop counter
  // are anonymous, and making the cashier pick one every time is the kind of
  // friction that ends with the system abandoned for paper.
  $effect(() => {
    listCustomers()
      .then((list) => {
        customers = list;
        if (!customerId) {
          const walkIn = list.find((c) => c.name === 'walk-in');
          customerId = walkIn?.id ?? '';
        }
      })
      .catch(() => {
        /* The form still works without the list: the server defaults to walk-in. */
      });
  });

  $effect(() => {
    listPaymentMethods()
      .then((list) => {
        // Active only, and the seeded cash first: it is the default and the common
        // case at a counter, so it must not sit wherever the alphabet puts it.
        methods = list
          .filter((m) => m.is_active)
          .sort((a, b) => (a.name === 'cash' ? -1 : b.name === 'cash' ? 1 : 0));
      })
      .catch(() => {
        // The form still works: the server defaults to cash, and an empty picker
        // records exactly what a cashier did before v3. But a silent `catch` here
        // would leave the control showing only its "default" entry with no hint why,
        // so a failed read is said out loud rather than swallowed. It is not a page
        // error: the sale can still be recorded.
        methodsFailed = true;
      });
  });

  const complete = $derived(completeLines(lines));

  const total = $derived(draftTotal(complete));

  const canRecord = $derived(canRecordNow(lines, submitting));

  // Tell the route whether the band's action is live. Without this the button
  // would look clickable while the sheet has nothing to record.
  $effect(() => {
    onready?.(canRecord);
  });

  /** True when a line has text in a field that does not parse. */
  const hasInvalid = $derived(hasInvalidLine(lines));

  const unitCount = $derived(unitCountOf(complete));
  const bonusCount = $derived(bonusCountOf(complete));

  function addProduct(product: Product) {
    // The price is prefilled from the list price but stays overridable: a
    // negotiated price is a real thing at a counter.
    lines = [
      ...lines,
      {
        id: `l${nextLineId++}`,
        product,
        qtyText: '1',
        // The field is read back in the active language, so it is *filled* in
        // the active language too. Pre-filling the wire form (`25.00`) into an
        // es-AR field would make the line refuse to parse on arrival, and the
        // sale would be silently unrecordable (S5).
        priceText: lang.fmt.amountInput(product.list_price)
      }
    ];
    justRecorded = null;
    error = null;
    // Focus moves to the quantity of the line just added, so the cashier keeps
    // typing instead of hunting for where the row landed. New lines are always
    // appended, so the last bound input is the one just created.
    queueMicrotask(() => lastQtyEl?.select());
  }

  function removeLine(index: number) {
    lines = lines.filter((_, i) => i !== index);
  }

  function priceHint(product: Product) {
    return lang.fmt.priceUnit(product.unit);
  }

  async function addCustomer() {
    const name = newCustomerName.trim();
    if (!name) return;
    creatingCustomer = true;
    notice = null;
    error = null;
    try {
      const before = customers.length;
      const customer = await findOrCreateCustomer(name);
      customers = await listCustomers();
      customerId = customer.id;
      newCustomerName = '';
      // Decision 7: a duplicate name is not an error. Say what happened, so the
      // cashier knows they did not create a second row by accident.
      notice = customers.length === before ? lang.t.sales_customer_existing(name) : null;
    } catch (e) {
      error = (e as Error).message;
    } finally {
      creatingCustomer = false;
    }
  }

  /**
   * Records the sale.
   *
   * Exported so the band's action — the page's single next action — drives the
   * same code path as the keyboard shortcut, instead of a second implementation
   * that could drift from this one.
   */
  export async function record() {
    if (!canRecord) return;
    submitting = true;
    error = null;
    notice = null;
    try {
      const sale = await createSale({
        customer_id: customerId || undefined,
        payment_method_id: paymentMethodId || undefined,
        items: complete.map((l) => ({
          product_id: l.product.id,
          quantity: l.quantity,
          unit_price: l.unitPrice
        }))
      });
      lines = [];
      justRecorded = { id: sale.id, total: sale.total };
      onrecorded?.(sale.id);
    } catch (e) {
      // The typed lines stay on screen: losing a half-entered sale to a
      // dropped connection is the kind of thing that sends someone back to paper.
      error = (e as Error).message;
    } finally {
      submitting = false;
    }
  }

  function onsheetkeydown(e: KeyboardEvent) {
    // Ctrl/Cmd+Enter records, so the keyboard flow never needs the mouse.
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      record();
    }
  }
</script>

<!-- The recording shortcut listens on the window rather than on a container:
     a non-interactive container must not take keyboard focus, and
     Ctrl/Cmd+Enter is unambiguous wherever the caret happens to be. -->
<svelte:window onkeydown={onsheetkeydown} />

<section class="sheet slip" aria-labelledby="draft-h">
  <!-- The slip's accessible name is a heading that is always present. It used to
       be the visible `<h2>`; the pad draws its state word in that slot instead, so
       the name has to survive both states or `aria-labelledby` would dangle the
       moment a sale is recorded and the word is replaced by the stamp. -->
  <h2 id="draft-h" class="sr-only">{lang.t.sales_draft_title}</h2>
  <!-- The slip's own head. It carries the document's state word and the two
       questions a sale asks about context rather than about goods — who it was
       for and where the money went — and nothing else.

       The number slot deliberately holds a word and not a number. The artboard
       prints "0005" there because it drew a document that already existed; the
       real draft has no number until the server assigns one, and inventing the
       next in a sequence would print a number that is wrong the moment another
       till records a sale. So the slot says what the slip actually is: still
       being written, or recorded. -->
  <header class="head slip-top">
    <div class="slip-no">
      {#if justRecorded}
        <!-- `data-doc-id` is the identity an e2e test needs. A ledger slip shows
             the time, the customer, the method, the state and the total, and the
             product name only lives in the lines behind it — so a spec has no
             other field to select its own document by. A total is not unique
             either: 100.00 x 7 and 175.00 x 4 both come to 700.00. The id is. -->
        <span class="recorded" data-doc-id={justRecorded.id}>
          <Stamp label={lang.t.sales_confirmed} />
          <span class="figures">{formatMoney(justRecorded.total)}</span>
        </span>
      {:else}
        <span class="label draft-state">{lang.t.sales_draft_title}</span>
        <span class="hint">{lang.t.sales_draft_hint}</span>
      {/if}
    </div>

    <div class="slip-meta">
      <div class="field meta-item">
        <label class="label" for="customer">{lang.t.sales_customer}</label>
        <select class="control compact" id="customer" bind:value={customerId}>
          {#each customers as c (c.id)}
            <option value={c.id}>{c.name === 'walk-in' ? lang.t.sales_walkin : c.name}</option>
          {/each}
        </select>
      </div>
      <div class="field meta-item">
        <!-- Where the money went. It sits beside the customer because they are the
             two questions a sale asks about context rather than about goods, and
             omitting it means the seeded cash method — the default the server
             already applies, so a cashier who never touches this control records
             exactly what they did before v3 did. Only active methods are listed:
             a retired one must not be offered. -->
        <label class="label" for="method">{lang.t.sales_method}</label>
        <select class="control compact" id="method" bind:value={paymentMethodId}>
          <option value="">{lang.t.sales_method_default}</option>
          {#each methods as m (m.id)}
            <option value={m.id}>{m.name}</option>
          {/each}
        </select>
        {#if methodsFailed}
          <p class="method-hint" role="status">{lang.t.sales_method_failed}</p>
        {/if}
      </div>
    </div>
  </header>

  <!-- The entry line: where a line is added. It sits between the slip's head and
       its printed body, the way a counter pad has a blank line to write on. -->
  <div class="entry">
    <div class="field">
      <span class="label" id="picker-label">{lang.t.sales_product}</span>
      <div class="picker-wrap">
        <ProductPicker onpick={addProduct} />
        <kbd class="picker-key" aria-hidden="true">F2</kbd>
      </div>
    </div>
  </div>

  <div class="customer-actions">
    <!-- The new-customer field belongs to the customer field above it, and read
         as a separate unlabelled input it looked like a second question. The
         inline create is part of choosing a customer: name one that is not in
         the list and it is created. The button says the action, so the field
         only needs to say what to type. -->
    <input
      class="control"
      bind:value={newCustomerName}
      placeholder={lang.t.sales_new_customer_placeholder}
      aria-label={lang.t.sales_new_customer}
      onkeydown={(e) => e.key === 'Enter' && addCustomer()}
    />
    <button type="button" class="ghost" onclick={addCustomer} disabled={creatingCustomer}>
      {creatingCustomer ? '…' : lang.t.sales_new_customer}
    </button>
  </div>

  {#if notice}
    <p class="notice" role="status">{notice}</p>
  {/if}

  {#if lines.length === 0}
    <p class="empty">{lang.t.sales_no_lines}</p>
  {:else}
    <!-- The printed body. Each line is a row of the document, not a record in a
         grid: the name stands on its own and its arithmetic sits beneath it, the
         way a printed slip itemises. It is the one part of the draft that grows
         without limit, so it scrolls and everything around it does not. -->
    <div class="lines-scroll">
      <div class="printed">
        {#each lines as line, i (line.id)}
          {@const qty = parseQuantity(line.qtyText)}
          {@const price = parseAmount(line.priceText)}
          {@const lt = qty !== null && price !== null ? lineTotalAmount(qty, price) : null}
          <!-- `data-draft-line` is the contract an e2e test reads. A line used to
               be a `<tr>` and twenty-three assertions pointed at
               `table.lines tbody tr`; the selector now names the thing a test
               means instead of the element the design happened to use. -->
          <div class="prow" data-draft-line>
            <div class="pname">
              <span class="pname-v">{line.product.name}</span>
              <span class="pname-sub">
                <span class="unit">· {lang.fmt.unit(line.product.unit)}</span>
              </span>
            </div>

            <div class="pfields">
              <input
                class="control tiny"
                class:bad={line.qtyText.trim() !== '' && qty === null}
                bind:this={lastQtyEl}
                bind:value={line.qtyText}
                inputmode="numeric"
                aria-label="{lang.t.sales_qty} — {line.product.name}"
              />
              <span class="times" aria-hidden="true">×</span>
              <span class="cur" aria-hidden="true">$</span>
              <input
                class="control tiny"
                class:bad={line.priceText.trim() !== '' && price === null}
                bind:value={line.priceText}
                inputmode="decimal"
                aria-label="{lang.t.sales_price} {priceHint(line.product)} — {line.product.name}"
              />
              <span class="per">/{lang.fmt.unit(line.product.unit)}</span>
            </div>

            <div class="pline figures">{lt === null ? '—' : formatMoney(lt)}</div>

            <button
              type="button"
              class="remove"
              onclick={() => removeLine(i)}
              aria-label="{lang.t.sales_remove} — {line.product.name}"
            >
              <svg viewBox="0 0 12 12" aria-hidden="true" focusable="false">
                <path
                  d="M3 3 9 9M9 3 3 9"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">
      <strong>{lang.t.sales_error_title}</strong>
      {error}
    </p>
  {/if}

  {#if lines.length || hasInvalid}
    <div class="actions">
      {#if lines.length}
        <button type="button" class="ghost" onclick={() => (lines = [])} disabled={submitting}>
          {lang.t.sales_clear}
        </button>
      {/if}
      {#if hasInvalid}
        <span class="warn">{lang.t.sales_invalid}</span>
      {/if}
    </div>
  {/if}

  <!-- The slip's foot: the arithmetic, then the total on its own rule. The action
       is the page's, and lands directly beneath this — see the note in
       `routes/sales/+page.svelte`, which owns it because the button must reach the
       sheet's `record()` without the sheet keeping a second copy of it. -->
  <div class="foot slip-foot">
    <div class="sums">
      <div class="summary">
        <span class="words">
          {complete.length}
          {complete.length === 1 ? lang.t.sales_line_one : lang.t.sales_lines}
          {#if unitCount}· {unitCount} {lang.t.sales_pieces}{/if}
          {#if bonusCount}
            <br />{lang.t.sales_bonus_note(formatMoney('0.00'))}
          {/if}
        </span>
      </div>
      <div class="sum big">
        <span class="label">{lang.t.sales_live_total}</span>
        <span class="total figures">{total === null ? '—' : formatMoney(total)}</span>
      </div>
    </div>
  </div>
</section>

<style>
  /* The slip: the sale being typed, drawn in the shape it will be printed in.
     A closed sheet with a head, a printed body and a foot. It is still not a
     modal — the day stays visible beside it — but it is now the document itself
     rather than a form that resembles one. */
  .sheet {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    background: var(--surface);
  }

  /* The head: the state word and the two context fields. The dashed lower edge
     marks where the provisional part begins, the same way a pad separates what
     was written from what is still blank. */
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 18px 12px;
    border-bottom: 1px dashed var(--rule-strong);
    flex-wrap: wrap;
  }
  .slip-no {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .draft-state {
    font-size: var(--step-0);
    letter-spacing: 0.12em;
  }
  .hint {
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  .recorded {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-size: var(--step-2);
  }

  .slip-meta {
    display: flex;
    gap: 14px;
    flex-wrap: wrap;
  }
  .meta-item {
    display: grid;
    gap: 5px;
    min-width: 132px;
  }

  /* The entry line: where a line is added. Between the slip's head and its
     printed body, the way a counter pad has a blank line to write on. */
  .entry {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
    padding: 13px 18px 0;
    align-items: start;
  }
  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .method-hint {
    margin: 0;
    color: var(--state-pending);
    font-size: var(--step--1);
  }

  /* Visually hidden and still announced. `position: fixed` with no offset keeps
     it 1px wide; an absolutely positioned one anchored to no ancestor measured
     538px inside a 390px viewport once and stretched the whole document. */
  .sr-only {
    position: fixed;
    top: 0;
    left: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .picker-wrap {
    position: relative;
  }
  .picker-wrap :global(#product-search) {
    padding-right: 48px;
  }
  .picker-key {
    position: absolute;
    top: 50%;
    right: 10px;
    transform: translateY(-50%);
    font-family: var(--font-text);
    font-size: var(--step--1);
    font-weight: 600;
    line-height: 1;
    color: var(--ink-soft);
    border: 1px solid currentColor;
    border-radius: 2px;
    padding: 2px 5px;
    pointer-events: none;
  }

  .customer-actions {
    display: flex;
    gap: 8px;
    padding: 10px 18px 13px;
    border-bottom: 1px solid var(--rule);
  }
  .ghost {
    background: none;
    border: 1px dashed var(--rule-strong);
    color: var(--ink);
    border-radius: var(--radius);
    padding: 9px 12px;
    font-size: var(--step-1);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .ghost:hover:not(:disabled) {
    border-color: var(--ink);
    /* The tint is derived from the ink token rather than written as the retired
       indigo literal: `rgb(43 49 98 / 0.05)` was the previous world's colour
       surviving as a raw value, which is exactly what DESIGN.md forbids. */
    background: color-mix(in srgb, var(--ink) 5%, transparent);
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .notice {
    margin: 0;
    padding: 9px 18px;
    background: color-mix(in srgb, var(--ink) 6%, transparent);
    border-bottom: 1px solid var(--rule);
    font-size: var(--step-0);
  }
  .empty {
    margin: 0;
    padding: 16px 18px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  /* The one region that grows without limit. It takes whatever height is left
     after the fixed parts and scrolls inside it, so a forty-line sale never
     pushes the total or the action off the screen. `min-height: 0` is what lets
     a flex child actually scroll instead of stretching to its content. */
  .lines-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding-right: 2px;
    margin-right: -2px;
  }

  .printed {
    padding: 2px 18px 0;
  }

  /* A printed line: the product stands on its own, its arithmetic sits beneath
     it, and the line total is right-aligned against the slip's edge. No columns
     are shared across lines, which is what lets a long name wrap without
     pushing every other row out of alignment. */
  .prow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto auto;
    gap: 12px;
    align-items: baseline;
    padding: 10px 0;
    border-bottom: 1px solid var(--rule);
  }
  .prow:last-child {
    border-bottom: 0;
  }
  .pname {
    min-width: 0;
  }
  .pname-v {
    display: block;
    overflow-wrap: anywhere;
  }
  .pname-sub {
    display: block;
    margin-top: 2px;
  }
  .unit {
    color: var(--ink-soft);
    font-size: var(--step--1);
  }

  .pfields {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .times {
    color: var(--ink-soft);
    font-size: var(--step--1);
  }
  .control.tiny {
    width: 62px;
    text-align: right;
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    padding: 5px 7px;
  }
  .control.bad {
    border-color: var(--state-void);
    background: color-mix(in srgb, var(--state-void) 5%, var(--surface));
  }
  .cur,
  .per {
    color: var(--ink-soft);
    font-size: var(--step--1);
    white-space: nowrap;
  }
  .pline {
    font-family: var(--font-figure);
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    font-size: var(--step-2);
    text-align: right;
    min-width: 92px;
    white-space: nowrap;
  }

  .remove {
    background: none;
    border: 0;
    color: var(--ink-soft);
    cursor: pointer;
    padding: 4px;
    line-height: 0;
    border-radius: var(--radius);
  }
  .remove svg {
    width: 12px;
    height: 12px;
  }
  .remove:hover {
    color: var(--state-void);
  }

  .error {
    margin: 0;
    padding: 10px 18px;
    border-top: 1px solid var(--rule);
    color: var(--state-void);
    font-size: var(--step-0);
  }
  .error strong {
    font-weight: 600;
  }

  /* The foot: the arithmetic, then the total on its own rule. The action lands
     directly beneath this and belongs to the page — it has to reach `record()`
     without the sheet keeping a second copy of the primary button. */
  .foot {
    flex-shrink: 0;
    padding: 12px 18px 6px;
    border-top: 1px dashed var(--rule-strong);
  }
  .sums {
    display: grid;
    gap: 8px;
  }
  .summary {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .words {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .sum.big {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding-top: 8px;
    border-top: 1px solid var(--rule);
  }
  .total {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.01em;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 18px;
    border-top: 1px solid var(--rule);
  }
  .warn {
    color: var(--state-void);
    font-size: var(--step-0);
  }

  @media (max-width: 860px) {
    .head {
      flex-direction: column;
      align-items: stretch;
    }
    .slip-meta {
      flex-direction: column;
    }
    /* A narrow slip has no room for fields beside the arithmetic: the line
       becomes a name above a row of fields, which is what a receipt does on a
       till's own printer. */
    .prow {
      grid-template-columns: minmax(0, 1fr) auto;
      row-gap: 6px;
    }
    .pfields {
      grid-column: 1 / -1;
    }
    .pline {
      grid-column: 2;
      grid-row: 1;
    }
  }
</style>
