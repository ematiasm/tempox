<script lang="ts">
  /**
   * The purchase sheet: the purchase being typed, standing at the head of the
   * book — the mirror of `SaleDraft`, with the direction reversed.
   *
   * The differences from the sale sheet are deliberate and small:
   *
   * - The supplier is the counterpart of the customer, and it defaults to the
   *   seeded one-off supplier. The sale sheet can absorb a duplicate customer
   *   name because creating one is an inline convenience; here a duplicate is
   *   reported, because the picker already lists the suppliers and the answer is
   *   to choose the existing one.
   * - The stock goes UP. Nothing on this screen shows the resulting balance: a
   *   balance is derived, and the ledger is where that story is told.
   */
  import {
    ApiError,
    createPurchase,
    createSupplier,
    listSuppliers,
    type Product,
    type Supplier
  } from '#lib/api.js';
  import { formatMoney, lineTotalAmount } from '#lib/money.js';
  import { draftTotal, everyLineComplete, parseDraftLine } from '#lib/draftLines.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import ProductPicker from './ProductPicker.svelte';

  interface Props {
    /** Called after a purchase is recorded, so the ledger can refresh. */
    onrecorded?: (purchaseId: string) => void;
    /** Reports whether the sheet holds something recordable; the band owns the action. */
    onready?: (ready: boolean) => void;
  }

  let { onrecorded, onready }: Props = $props();

  type Line = {
    id: string;
    product: Product;
    qtyText: string;
    priceText: string;
  };

  let nextLineId = 0;
  let lines = $state<Line[]>([]);
  let suppliers = $state<Supplier[]>([]);
  let supplierId = $state<string>('');
  let submitting = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let creatingSupplier = $state(false);
  let newSupplierName = $state('');
  let supplierError = $state<string | null>(null);
  let justRecorded = $state<{ id: string; total: string } | null>(null);

  // The seeded supplier is the default, and it is what the server would use
  // anyway if the field were left alone. Selecting it explicitly keeps the
  // screen honest about who the purchase is from.
  $effect(() => {
    listSuppliers()
      .then((list) => {
        suppliers = list;
        if (!supplierId) {
          const seeded = list.find((s) => s.name === 'one-off-supplier');
          supplierId = seeded?.id ?? list[0]?.id ?? '';
        }
      })
      .catch((e: unknown) => {
        supplierError = lang.t.purchases_load_suppliers_failed + describe(e);
      });
  });

  function describe(e: unknown): string {
    if (e instanceof ApiError) return `HTTP ${e.status}`;
    return e instanceof Error ? e.message : String(e);
  }

  // The lines as parsed numbers, or null when any field is unreadable. The text
  // is the source of truth for each field's validity; a parsed value only exists
  // once it parses. Positional on purpose: the markup and `record` both index
  // this by the line's index in `lines`.
  const parsed = $derived(lines.map((line) => parseDraftLine(line)));

  // Deliberately stricter than the sales sheet: a purchase is a supplier's
  // document, so a single unreadable line blocks the whole thing rather than
  // being dropped from it.
  const parsedOk = $derived(everyLineComplete(lines));

  // `null` rather than `0.00` when the form is not recordable: the preview shows
  // a dash, because a partial purchase has no total worth showing.
  const total = $derived.by(() => {
    if (!parsedOk) return null;
    return draftTotal(parsed.filter((p) => p !== null));
  });

  // The library owns the rule; this screen does not restate half of it. Purchases
  // are stricter than sales about *which* lines count, which is why the strict
  // check is `everyLineComplete` and not `canRecord` — but "not while in flight"
  // is one fact, and it lives in one place on the other drafts.
  const ready = $derived(parsedOk && !submitting);

  $effect(() => {
    onready?.(ready);
  });

  function addLine(product: Product) {
    lines.push({
      id: `pl-${nextLineId++}`,
      product,
      // A purchase usually arrives in units, and the catalogue price is the
      // best guess; both fields stay editable because an invoice rarely matches.
      //
      // `amountInput` is not decoration: `list_price` is the WIRE form (`10.00`)
      // and the price field is read back by a locale-aware parser, which correctly
      // refuses `10.00` under es-AR. Pre-filling the wire form is the exact defect
      // v1.5 recorded — the field looked filled and the purchase was silently
      // unrecordable.
      qtyText: '1',
      priceText: lang.fmt.amountInput(product.list_price)
    });
    justRecorded = null;
  }

  function removeLine(id: string) {
    lines = lines.filter((l) => l.id !== id);
  }

  function clear() {
    lines = [];
    error = null;
    justRecorded = null;
  }

  /**
   * Ctrl/Cmd+Enter records, so the keyboard flow never needs the mouse.
   *
   * This lives here rather than on the page because `record()` does: the page's
   * F2 handler and this one are the same action reached two ways, and the sheet
   * owning its own shortcut is what keeps them from drifting. It was missing —
   * the page advertised `Control+Enter` in `aria-keyshortcuts` and nothing
   * listened, so the shortcut the sales screen teaches silently did nothing
   * here. The review caught it; the button had been promising it since the page
   * was written.
   */
  function onsheetkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      record();
    }
  }

  async function record() {
    // The guard against a second submission landing while the first is in flight.
    // It used to be missing here, and the disabled button hid that: the button is
    // only one caller, and a queued keyboard event or a form submit reaches this
    // handler regardless. A purchase raises stock and lowers cash, so recording it
    // twice moves the till twice — measured before this line existed, two
    // dispatched Ctrl+Enter presses produced two purchases.
    //
    // `SaleDraft` guards the same way through `canRecord(lines, submitting)`, and
    // `Catalog.svelte` names the symmetry in its own comment.
    if (submitting) return;
    if (!supplierId) {
      error = lang.t.purchases_supplier_required;
      return;
    }
    if (!parsedOk) {
      error = lang.t.purchases_invalid;
      return;
    }
    submitting = true;
    error = null;
    try {
      const items = lines.map((line, i) => {
        const p = parsed[i];
        if (p === null) throw new Error('unreachable: parsed checked above');
        return {
          product_id: line.product.id,
          quantity: p.quantity,
          unit_price: p.unitPrice
        };
      });
      const created = await createPurchase({ supplier_id: supplierId, items });
      justRecorded = { id: created.purchase.id, total: created.purchase.total };
      lines = [];
      error = null;
      onrecorded?.(created.purchase.id);
    } catch (e) {
      error = describe(e);
    } finally {
      submitting = false;
    }
  }

  async function addSupplier() {
    const name = newSupplierName.trim();
    if (!name) return;
    supplierError = null;
    try {
      const created = await createSupplier(name);
      suppliers = [...suppliers, created].sort((a, b) => a.name.localeCompare(b.name));
      supplierId = created.id;
      newSupplierName = '';
      creatingSupplier = false;
    } catch (e) {
      // A 409 is the useful case: the name is already in the list, so the answer
      // is to pick it rather than to create it twice.
      if (e instanceof ApiError && e.status === 409) {
        notice = lang.t.purchases_supplier_existing(name);
      } else {
        supplierError = lang.t.purchases_create_supplier_error + describe(e);
      }
    }
  }

  // Exposed to the band's button and to Ctrl+Enter, so the two paths cannot
  // drift apart.
  export function triggerRecord() {
    void record();
  }
</script>

<svelte:window onkeydown={onsheetkeydown} />

<section class="sheet" aria-labelledby="purchase-draft-title">
  <header class="sheet-head">
    <h2 id="purchase-draft-title">{lang.t.purchases_draft_title}</h2>
    {#if justRecorded}
      <!-- The identity an e2e test needs; see the same note in SaleDraft. -->
      <p class="done" role="status" data-doc-id={justRecorded.id}>
        {lang.t.purchases_legend} · {formatMoney(justRecorded.total)}
      </p>
    {/if}
  </header>

  <div class="supplier-row">
    <label for="purchase-supplier">{lang.t.purchases_supplier}</label>
    <select id="purchase-supplier" bind:value={supplierId}>
      {#each suppliers as s (s.id)}
        <option value={s.id}>
          {s.name === 'one-off-supplier' ? lang.t.purchases_one_off_supplier : s.name}
        </option>
      {/each}
    </select>
    <button type="button" class="ghost" onclick={() => (creatingSupplier = !creatingSupplier)}>
      {lang.t.purchases_new_supplier}
    </button>
  </div>

  {#if creatingSupplier}
    <div class="supplier-new">
      <input
        type="text"
        bind:value={newSupplierName}
        placeholder={lang.t.purchases_new_supplier_placeholder}
        aria-label={lang.t.purchases_new_supplier_placeholder}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            void addSupplier();
          }
        }}
      />
      <button type="button" class="ghost" onclick={() => void addSupplier()}>
        {lang.t.purchases_new_supplier}
      </button>
    </div>
  {/if}

  {#if notice}
    <p class="notice" role="status">{notice}</p>
  {/if}
  {#if supplierError}
    <p class="error" role="alert">{supplierError}</p>
  {/if}

  <div class="picker-row">
    <ProductPicker onpick={addLine} />
  </div>

  {#if lines.length === 0}
    <p class="empty">{lang.t.purchases_no_lines}</p>
  {:else}
    <!-- The printed body: each line is a row of the document, not a record in a
         grid. The name stands on its own and its arithmetic sits beneath it, the
         way a printed slip itemises. This is the mirror of the sales draft, and
         it moved for the same reason: a purchase is the same document in the
         other direction. -->
    <div class="lines-scroll">
      <div class="printed">
        {#each lines as line, i (line.id)}
          <div class="prow" data-draft-line>
            <div class="pname">
              <span class="pname-v">{line.product.name}</span>
              <span class="pname-sub unit">· {lang.fmt.unit(line.product.unit)}</span>
            </div>

            <div class="pfields">
              <input
                type="text"
                inputmode="numeric"
                class="qty"
                bind:value={line.qtyText}
                aria-label="{lang.t.sales_qty} {line.product.name}"
              />
              <span class="times" aria-hidden="true">×</span>
              <span class="cur" aria-hidden="true">$</span>
              <input
                type="text"
                inputmode="decimal"
                class="price"
                bind:value={line.priceText}
                aria-label="{lang.t.sales_price} {line.product.name}"
              />
              <span class="per">/{lang.fmt.unit(line.product.unit)}</span>
            </div>

            <div class="pline figure">
              {#if parsed[i]}
                {formatMoney(lineTotalAmount(parsed[i]!.quantity, parsed[i]!.unitPrice) ?? '0.00')}
              {:else}
                —
              {/if}
            </div>

            <button
              type="button"
              class="ghost remove"
              onclick={() => removeLine(line.id)}
              aria-label="{lang.t.sales_remove} {line.product.name}"
            >
              ×
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if error}
    <p class="error" role="alert">{lang.t.purchases_error_title}{error}</p>
  {/if}

  <footer class="sheet-foot">
    {#if lines.length > 0}
      <button type="button" class="ghost" onclick={clear}>{lang.t.sales_clear}</button>
    {/if}
    <span class="total-label">{lang.t.purchases_live_total}</span>
    <span class="total figure">{total === null ? '—' : formatMoney(total)}</span>
    {#if !ready}
      <span class="state-hint">{lang.t.purchases_action_hint}</span>
    {/if}
    <span class="hint">{lang.t.purchases_draft_hint}</span>
  </footer>
</section>

<style>
  /* The slip is the same artifact as the sale's, in the other direction. The page
     draws the closed surface around it and pins the action at its foot, so this
     element is the sheet's interior rather than its frame. */
  .sheet {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    background: var(--surface);
    padding: 14px 18px 0;
  }
  .sheet + :global(.action-foot) {
    padding-top: 0;
  }
  .sheet + :global(.action-foot .why) {
    display: none;
  }
  .sheet-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
  }
  .sheet-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .done {
    color: var(--ink);
    font-size: var(--step--1);
  }
  .supplier-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
    /* Three items share this row: the label, the select and the "Proveedor nuevo"
       button. At 390px they add up wider than the panel, and the button was
       overflowing it to x=397 against a container ending at 342 — a control
       clipped by its own frame, which no caption explains. The row wraps instead. */
    flex-wrap: wrap;
    min-width: 0;
  }
  .supplier-row label {
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-soft);
  }
  .supplier-row select {
    min-width: 0;
    max-width: 100%;
  }
  .supplier-row button.ghost {
    flex-shrink: 0;
  }
  .supplier-row select,
  .supplier-new input {
    font: inherit;
    padding: 7px 9px;
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    background: var(--ground);
    color: var(--ink);
  }
  .supplier-new {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .supplier-new input {
    flex: 1;
  }
  .picker-row {
    margin-top: 14px;
  }
  .empty {
    color: var(--ink-soft);
    font-size: var(--step-0);
    padding: 18px 0;
  }
  /* The one region that grows without limit, exactly as on the sales slip: it
     takes whatever height is left and scrolls inside it, so a forty-line purchase
     never pushes the total or the action off the screen. */
  .lines-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    margin-top: 12px;
    padding-right: 2px;
    margin-right: -2px;
  }
  .printed {
    padding: 0 0 2px;
  }

  /* A printed line: the product stands on its own, its arithmetic sits beneath
     it, and the line total sits against the slip's right edge. */
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
  .pfields input {
    font: inherit;
    text-align: right;
    padding: 5px 7px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--surface);
    font-variant-numeric: tabular-nums;
  }
  .pfields .qty {
    width: 62px;
  }
  .pfields .price {
    width: 96px;
  }
  .cur,
  .per {
    color: var(--ink-soft);
    font-size: var(--step--1);
    white-space: nowrap;
  }
  .pline {
    font-size: var(--step-2);
    font-weight: 600;
    text-align: right;
    min-width: 92px;
    white-space: nowrap;
  }
  .remove {
    align-self: center;
  }
  .figure {
    font-variant-numeric: tabular-nums;
    font-family: var(--font-figure);
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
  .ghost:hover {
    color: var(--ink);
    border-color: var(--ink-soft);
  }
  .notice {
    color: var(--ink);
    font-size: var(--step--1);
    margin-top: 8px;
  }
  .error {
    color: var(--state-void);
    font-size: var(--step-0);
    padding: 10px 0;
  }
  .sheet-foot {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 8px 14px;
    padding: 12px 0 6px;
    margin-top: auto;
    border-top: 1px dashed var(--rule-strong);
  }
  .hint,
  .state-hint {
    flex-basis: 100%;
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .hint {
    padding-bottom: 8px;
  }
  .total-label {
    margin-left: auto;
    font-family: var(--font-figure);
    font-size: var(--step-0);
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--ink-soft);
  }
  .total {
    font-size: var(--step-4);
    font-weight: 700;
    line-height: 1;
    letter-spacing: -0.01em;
  }
</style>
