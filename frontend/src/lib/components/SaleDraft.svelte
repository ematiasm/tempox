<script lang="ts">
  /**
   * The draft sheet: the sale being typed, standing at the head of the book.
   *
   * It is deliberately not a modal. Recording a sale is the shop's primary
   * action and the ledger stays visible behind it, because the cashier checks
   * what just landed against what they are typing.
   *
   * The sheet holds no money of its own: every figure shown comes from
   * `lineTotal` and the sum of those, so the total on screen and the total the
   * server computes cannot drift.
   */
  import {
    createSale,
    listCustomers,
    findOrCreateCustomer,
    type Customer,
    type Product
  } from '#lib/api.js';
  import { formatCents, lineTotal, parseAmountToCents, parseQuantity } from '#lib/money.js';
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
  let submitting = $state(false);
  let error = $state<string | null>(null);
  /** Set when a name already existed and was selected instead of created (S7). */
  let notice = $state<string | null>(null);
  let justRecorded = $state<{ id: string; total: number } | null>(null);
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

  const parsed = $derived(
    lines.map((l) => ({
      product: l.product,
      quantity: parseQuantity(l.qtyText),
      unitPrice: parseAmountToCents(l.priceText)
    }))
  );

  /** A line counts only when both of its fields parse; a half-typed line is not money. */
  const complete = $derived(parsed.filter((l) => l.quantity !== null && l.unitPrice !== null));

  const total = $derived(
    complete.reduce((sum, l) => sum + lineTotal(l.quantity as number, l.unitPrice as number), 0)
  );

  const canRecord = $derived(complete.length > 0 && !submitting);

  // Tell the route whether the band's action is live. Without this the button
  // would look clickable while the sheet has nothing to record.
  $effect(() => {
    onready?.(canRecord);
  });

  /** True when a line has text in a field that does not parse. */
  const hasInvalid = $derived(
    parsed.some(
      (l, i) =>
        (l.quantity === null && lines[i].qtyText.trim() !== '') ||
        (l.unitPrice === null && lines[i].priceText.trim() !== '')
    )
  );

  const unitCount = $derived(complete.reduce((sum, l) => sum + (l.quantity as number), 0));
  const bonusCount = $derived(complete.filter((l) => (l.unitPrice as number) === 0).length);

  function addProduct(product: Product) {
    // The price is prefilled from the list price but stays overridable: a
    // negotiated price is a real thing at a counter.
    lines = [
      ...lines,
      {
        id: `l${nextLineId++}`,
        product,
        qtyText: '1',
        priceText: (product.list_price / 100).toFixed(2)
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
    return product.unit === 'unit' ? lang.t.sales_price_unit : `$/${product.unit}`;
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
        items: complete.map((l) => ({
          product_id: l.product.id,
          quantity: l.quantity as number,
          unit_price: l.unitPrice as number
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

<section class="sheet" aria-labelledby="draft-h">
  <header class="head">
    <h2 id="draft-h">{lang.t.sales_draft_title}</h2>
    {#if justRecorded}
      <span class="recorded">
        <Stamp label={lang.t.sales_confirmed} size="cell" land />
        <span class="figures">{formatCents(justRecorded.total)}</span>
      </span>
    {:else}
      <span class="hint">{lang.t.sales_draft_hint}</span>
    {/if}
  </header>

  <div class="entry">
    <div class="field">
      <span class="label" id="picker-label">{lang.t.sales_product}</span>
      <ProductPicker onpick={addProduct} />
    </div>
    <div class="field">
      <label class="label" for="customer">{lang.t.sales_customer}</label>
      <select class="control" id="customer" bind:value={customerId}>
        {#each customers as c (c.id)}
          <option value={c.id}>{c.name === 'walk-in' ? lang.t.sales_walkin : c.name}</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="customer-actions">
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
    <table class="lines">
      <thead>
        <tr>
          <th scope="col">{lang.t.sales_product}</th>
          <th scope="col" class="num">{lang.t.sales_qty}</th>
          <th scope="col" class="num">{lang.t.sales_price}</th>
          <th scope="col" class="num">{lang.t.sales_line}</th>
          <th scope="col"><span class="sr">Remove</span></th>
        </tr>
      </thead>
      <tbody>
        {#each lines as line, i (line.id)}
          {@const qty = parseQuantity(line.qtyText)}
          {@const price = parseAmountToCents(line.priceText)}
          {@const lt = qty !== null && price !== null ? lineTotal(qty, price) : null}
          <tr>
            <td class="pname">
              {line.product.name}
              <span class="unit-note">· {line.product.unit}</span>
            </td>
            <td class="num">
              <input
                class="control tiny"
                class:bad={line.qtyText.trim() !== '' && qty === null}
                bind:this={lastQtyEl}
                bind:value={line.qtyText}
                inputmode="numeric"
                aria-label="{lang.t.sales_qty} — {line.product.name}"
              />
            </td>
            <td class="num">
              <div class="pricewrap">
                <span class="cur">$</span>
                <input
                  class="control tiny"
                  class:bad={line.priceText.trim() !== '' && price === null}
                  bind:value={line.priceText}
                  inputmode="decimal"
                  aria-label="{lang.t.sales_price} {priceHint(line.product)} — {line.product.name}"
                />
                <span class="per">/{line.product.unit}</span>
              </div>
            </td>
            <td class="num figures linetotal">{lt === null ? '—' : formatCents(lt)}</td>
            <td class="num">
              <button
                type="button"
                class="remove"
                onclick={() => removeLine(i)}
                aria-label="{lang.t.sales_remove} — {line.product.name}"
              >
                ×
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if error}
    <p class="error" role="alert">
      <strong>{lang.t.sales_error_title}</strong>
      {error}
    </p>
  {/if}

  <div class="foot">
    <div class="summary">
      <span class="label">{lang.t.sales_live_total}</span>
      <span class="words">
        {complete.length}
        {complete.length === 1 ? lang.t.sales_line_one : lang.t.sales_lines}
        {#if unitCount}· {unitCount} {lang.t.sales_pieces}{/if}
        {#if bonusCount}
          <br />{lang.t.sales_bonus_note}
        {/if}
      </span>
    </div>
    <div class="total figures">{formatCents(total)}</div>
  </div>

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
</section>

<style>
  .sheet {
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 4px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 13px 18px 11px;
    border-bottom: 1px solid var(--rule);
  }
  .head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .hint {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .recorded {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    font-size: var(--step-2);
  }

  .entry {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 260px);
    gap: 12px;
    padding: 15px 18px 0;
    align-items: start;
  }
  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .customer-actions {
    display: flex;
    gap: 8px;
    padding: 10px 18px 14px;
    border-bottom: 1px solid var(--rule);
  }
  .ghost {
    background: none;
    border: 1px dashed var(--rule-strong);
    color: var(--indigo);
    border-radius: var(--radius);
    padding: 9px 12px;
    font-size: var(--step-1);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .ghost:hover:not(:disabled) {
    border-color: var(--indigo);
    background: rgb(43 49 98 / 0.05);
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .notice {
    margin: 0;
    padding: 9px 18px;
    background: rgb(43 49 98 / 0.06);
    border-bottom: 1px solid var(--rule);
    color: var(--ink);
    font-size: var(--step-0);
  }

  .empty {
    padding: 22px 18px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  table.lines {
    width: 100%;
    border-collapse: collapse;
  }
  .lines th {
    font-family: var(--font-figure);
    font-size: var(--step-1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--indigo-soft);
    text-align: left;
    padding: 8px 10px;
    border-bottom: 1px solid var(--rule-strong);
    white-space: nowrap;
  }
  .lines th:first-child {
    padding-left: 18px;
  }
  .lines th:last-child {
    padding-right: 18px;
  }
  .lines td {
    padding: 8px 10px;
    border-bottom: 1px solid var(--rule);
    vertical-align: middle;
  }
  .lines td:first-child {
    padding-left: 18px;
  }
  .lines td:last-child {
    padding-right: 18px;
  }
  .num {
    text-align: right;
  }
  .pname {
    font-size: var(--step-2);
  }
  .unit-note {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .linetotal {
    font-size: var(--step-3);
    font-weight: 600;
    white-space: nowrap;
  }

  .control.tiny {
    width: 92px;
    padding: 6px 8px;
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  /* A field holding text that is not an amount says so on the field itself,
     not in a message somewhere else on the page. */
  .control.bad {
    border-color: var(--crimson);
    background: rgb(168 27 42 / 0.05);
  }

  .pricewrap {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
  }
  .cur,
  .per {
    font-family: var(--font-figure);
    color: var(--ink-soft);
    font-size: var(--step-1);
  }

  .remove {
    background: none;
    border: 0;
    color: var(--ink-soft);
    font-size: var(--step-3);
    line-height: 1;
    padding: 2px 6px;
    cursor: pointer;
    border-radius: 2px;
  }
  .remove:hover {
    color: var(--crimson);
    background: rgb(168 27 42 / 0.08);
  }

  .error {
    margin: 0;
    padding: 10px 18px;
    background: rgb(168 27 42 / 0.07);
    border-top: 1px solid rgb(168 27 42 / 0.35);
    color: var(--ink);
    font-size: var(--step-0);
  }
  .error strong {
    font-family: var(--font-figure);
    color: var(--crimson-deep);
    margin-right: 6px;
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 18px;
    background: var(--paper-sunk);
    border-top: 1px solid var(--rule-strong);
  }
  .summary {
    display: grid;
    gap: 3px;
  }
  .words {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .total {
    font-size: var(--step-5);
    font-weight: 700;
    line-height: 1;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 18px;
    border-top: 1px solid var(--rule);
  }
  .warn {
    color: var(--crimson-deep);
    font-size: var(--step-0);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  @media (max-width: 860px) {
    .entry {
      grid-template-columns: minmax(0, 1fr);
    }
    .foot {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
