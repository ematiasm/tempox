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
  import { formatMoney, lineTotalAmount, parseAmount, parseQuantity } from '#lib/money.js';
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
  // once it parses.
  const parsed = $derived(
    lines.map((line) => {
      const quantity = parseQuantity(line.qtyText);
      const price = parseAmount(line.priceText);
      return quantity !== null && price !== null ? { quantity, price } : null;
    })
  );

  const parsedOk = $derived(parsed.length > 0 && parsed.every((p) => p !== null));

  const total = $derived.by(() => {
    if (!parsedOk) return null;
    const amounts: string[] = [];
    for (const p of parsed) {
      if (p === null) return null;
      const lineTotal = lineTotalAmount(p.quantity, p.price);
      if (lineTotal === null) return null;
      amounts.push(lineTotal);
    }
    // Summed with the same helper the recorded total is checked against, so the
    // live figure and the server's answer cannot drift in different directions.
    let cents = 0n;
    for (const a of amounts) {
      const [whole, frac] = a.split('.');
      cents += BigInt(whole) * 100n + BigInt(frac ?? '0');
    }
    const sign = cents < 0n ? '-' : '';
    const abs = cents < 0n ? -cents : cents;
    return `${sign}${abs / 100n}.${String(abs % 100n).padStart(2, '0')}`;
  });

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

  async function record() {
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
          unit_price: p.price
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

<section class="sheet" aria-labelledby="purchase-draft-title">
  <header class="sheet-head">
    <h2 id="purchase-draft-title">{lang.t.purchases_draft_title}</h2>
    {#if justRecorded}
      <p class="done" role="status">
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
    <table class="lines">
      <thead>
        <tr>
          <th scope="col">{lang.t.sales_product}</th>
          <th scope="col" class="num">{lang.t.sales_qty}</th>
          <th scope="col" class="num">{lang.t.purchases_price_unit}</th>
          <th scope="col" class="num">{lang.t.sales_line}</th>
          <th scope="col"><span class="sr-only">{lang.t.sales_remove_column}</span></th>
        </tr>
      </thead>
      <tbody>
        {#each lines as line, i (line.id)}
          <tr>
            <td class="name">{line.product.name}</td>
            <td class="num">
              <input
                type="text"
                inputmode="numeric"
                bind:value={line.qtyText}
                aria-label="{lang.t.sales_qty} {line.product.name}"
              />
            </td>
            <td class="num">
              <input
                type="text"
                inputmode="decimal"
                bind:value={line.priceText}
                aria-label="{lang.t.sales_price} {line.product.name}"
              />
            </td>
            <td class="num figure">
              {#if parsed[i]}
                {formatMoney(lineTotalAmount(parsed[i]!.quantity, parsed[i]!.price) ?? '0.00')}
              {:else}
                —
              {/if}
            </td>
            <td>
              <button
                type="button"
                class="ghost"
                onclick={() => removeLine(line.id)}
                aria-label="{lang.t.sales_remove} {line.product.name}"
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
    <p class="error" role="alert">{lang.t.purchases_error_title}{error}</p>
  {/if}

  <footer class="sheet-foot">
    <span class="hint">{lang.t.purchases_draft_hint}</span>
    {#if lines.length > 0}
      <button type="button" class="ghost" onclick={clear}>{lang.t.sales_clear}</button>
    {/if}
    <span class="total-label">{lang.t.purchases_live_total}</span>
    <span class="total figure">{total === null ? '—' : formatMoney(total)}</span>
  </footer>
</section>

<style>
  .sheet {
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: var(--radius);
    padding: 18px 20px 0;
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
    color: var(--indigo);
    font-size: var(--step--1);
  }
  .supplier-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
  }
  .supplier-row label {
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-soft);
  }
  .supplier-row select,
  .supplier-new input {
    font: inherit;
    padding: 7px 9px;
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    background: var(--paper);
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
  .lines {
    width: 100%;
    border-collapse: collapse;
    margin-top: 14px;
  }
  .lines th {
    text-align: left;
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--ink-soft);
    border-bottom: 1px solid var(--rule-strong);
    padding: 6px 8px;
    font-weight: 600;
  }
  .lines td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--rule);
  }
  .lines .num {
    text-align: right;
  }
  .lines input {
    font: inherit;
    text-align: right;
    padding: 5px 7px;
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    background: var(--paper);
    width: 100%;
    font-variant-numeric: tabular-nums;
  }
  .figure {
    font-variant-numeric: tabular-nums;
    font-family: var(--font-figure);
  }
  .name {
    font-weight: 500;
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
    color: var(--indigo);
    font-size: var(--step--1);
    margin-top: 8px;
  }
  .error {
    color: var(--crimson);
    font-size: var(--step-0);
    padding: 10px 0;
  }
  .sheet-foot {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 0 16px;
    margin-top: 10px;
    border-top: 1px solid var(--rule-strong);
  }
  .hint {
    font-size: var(--step--1);
    color: var(--ink-soft);
  }
  .total-label {
    margin-left: auto;
    font-size: var(--step--1);
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--ink-soft);
  }
  .total {
    font-size: var(--step-3);
    font-weight: 600;
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
</style>
