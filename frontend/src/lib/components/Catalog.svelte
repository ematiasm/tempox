<script lang="ts">
  /**
   * The catalog: what the shop sells, its list price, and the stock the ledger
   * says is on the shelf.
   *
   * Stock is never typed here. It arrives derived from the movement ledger, so
   * this screen can only ever report it — the one place stock changes is a
   * recorded event.
   */
  import { ApiError, createProduct, listProducts, type Product, type Unit } from '$lib/api';
  import { formatCents, formatQuantity, parseAmountToCents, parseQuantity } from '$lib/money';
  import { lang } from '$lib/i18n/index.svelte';

  const PAGE = 50;

  let rows = $state<Product[]>([]);
  let offset = $state(0);
  let query = $state('');
  let loading = $state(true);
  let error = $state<string | null>(null);
  let pageError = $state<string | null>(null);

  // The create form. Unit is `unit` only in v1a: `g` and `ml` stay valid in the
  // database and the API, but a gram product's price is cents per gram, so the
  // finest price the schema can express is 1¢/g = $10.00/kg. Offering `g` here
  // would let someone create a product whose real price cannot be stored.
  let name = $state('');
  let priceText = $state('');
  let stockText = $state('');
  let creating = $state(false);
  let created = $state<string | null>(null);

  let timer: ReturnType<typeof setTimeout> | undefined;

  // Search is server-side and debounced: 500+ products do not fit in the client.
  $effect(() => {
    const q = query.trim();
    const at = offset;
    clearTimeout(timer);
    timer = setTimeout(async () => {
      loading = true;
      pageError = null;
      try {
        rows = await listProducts({ q, limit: PAGE, offset: at });
      } catch (e) {
        pageError = e instanceof ApiError ? `${e.message} (${e.status})` : String(e);
        rows = [];
      } finally {
        loading = false;
      }
    }, 150);
    return () => clearTimeout(timer);
  });

  // Typing a new search restarts at the first page: page 3 of the previous
  // query is meaningless for a different query.
  function onSearch(value: string) {
    query = value;
    offset = 0;
  }

  const priceCents = $derived(parseAmountToCents(priceText));
  const stockValue = $derived(stockText.trim() === '' ? 0 : parseQuantity(stockText));
  const priceBad = $derived(priceText.trim() !== '' && priceCents === null);
  const stockBad = $derived(stockText.trim() !== '' && stockValue === null);
  const canCreate = $derived(name.trim() !== '' && priceCents !== null && !stockBad && !creating);

  /** The price label declares the unit, so the number can never be misread. */
  function priceLabel(unit: Unit) {
    return unit === 'unit' ? lang.t.sales_price_unit : `$/${unit}`;
  }

  async function create() {
    if (!canCreate) return;
    creating = true;
    error = null;
    created = null;
    try {
      const product = await createProduct({
        name: name.trim(),
        list_price: priceCents as number,
        unit: 'unit',
        initial_stock: stockValue ?? 0
      });
      created = product.name;
      name = '';
      priceText = '';
      stockText = '';
      // Show the new product rather than leaving the catalog looking unchanged:
      // an unqualified search would drop the row out of a filtered view.
      query = '';
      offset = 0;
      rows = await listProducts({ limit: PAGE, offset: 0 });
    } catch (e) {
      error = e instanceof ApiError ? `${e.message} (${e.status})` : String(e);
    } finally {
      creating = false;
    }
  }

  const hasFilters = $derived(query.trim() !== '');

  /**
   * F2 focuses the create form's first field.
   *
   * The band button announces F2, so the key has to do something: a shortcut
   * printed on a control and wired to nothing is worse than no label, because
   * the cashier tries it and concludes the app is broken.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      document.getElementById('new-name')?.focus();
    }
  }
</script>

<!-- The band belongs to the route, not to this component: two regions rendering
     their own copy of the page's heading puts two <h1>Products</h1> on one page
     and leaves a screen reader with two rival titles. -->
<svelte:window {onkeydown} />

<section class="body" aria-labelledby="catalog-h">
  <div class="head">
    <div>
      <h2 id="catalog-h">{lang.t.products_catalog_title}</h2>
      <div class="sub">{lang.t.products_catalog_sub}</div>
    </div>
    <div class="controls">
      <input
        class="control search"
        id="catalog-search"
        value={query}
        oninput={(e) => onSearch(e.currentTarget.value)}
        placeholder={lang.t.products_search}
        aria-label={lang.t.products_search}
        type="search"
      />
      <span class="pager">
        <button
          type="button"
          onclick={() => (offset = Math.max(0, offset - PAGE))}
          disabled={offset === 0 || loading}
        >
          ← {lang.t.sales_older}
        </button>
        <button
          type="button"
          onclick={() => (offset = offset + PAGE)}
          disabled={rows.length < PAGE || loading}
        >
          {lang.t.sales_newer} →
        </button>
      </span>
    </div>
  </div>

  {#if pageError}
    <div class="alert" role="alert">
      <strong>{lang.t.products_error}</strong>
      <span>{pageError}</span>
    </div>
  {:else if loading && rows.length === 0}
    <div class="skeleton" aria-hidden="true">
      <div class="bar"></div>
      <div class="bar"></div>
      <div class="bar"></div>
    </div>
  {:else if rows.length === 0}
    <div class="empty">
      {#if hasFilters}
        <p>{lang.t.products_no_match}</p>
        <button type="button" class="ghost" onclick={() => onSearch('')}>
          {lang.t.products_clear_search}
        </button>
      {:else}
        <p>{lang.t.products_empty}</p>
        <button
          type="button"
          class="ghost"
          onclick={() => document.getElementById('new-name')?.focus()}
        >
          {lang.t.products_create_first}
        </button>
      {/if}
    </div>
  {:else}
    <table class="catalog">
      <thead>
        <tr>
          <th scope="col">{lang.t.sales_product}</th>
          <th scope="col">{lang.t.products_unit}</th>
          <th scope="col" class="num">{lang.t.products_list_price}</th>
          <th scope="col" class="num">{lang.t.products_stock}</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as product (product.id)}
          <tr>
            <td class="pname">{product.name}</td>
            <td>
              <span class="unit-chip figures">{product.unit}</span>
            </td>
            <td class="num">
              <span class="faces figures">{formatCents(product.list_price)}</span>
              <span class="per">{priceLabel(product.unit)}</span>
            </td>
            <td class="num">
              <!-- Zero is not "no data": an empty shelf is a fact the shop needs. -->
              <span
                class="stock figures"
                class:low={product.stock > 0 && product.stock < 10}
                class:out={product.stock === 0}
              >
                {formatQuantity(product.stock, product.unit)}
              </span>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="row-foot">
      <span>
        {rows.length}
        {rows.length === 1 ? lang.t.products_product_one : lang.t.products_products}
        {#if hasFilters}· “{query.trim()}”{/if}
      </span>
      <span class="derived">
        <span class="stamp-mini figures">{lang.t.products_derived}</span>
        {lang.t.products_derived_note}
      </span>
    </div>
  {/if}
</section>

<section class="body new" aria-labelledby="new-h">
  <div class="head">
    <div>
      <h2 id="new-h">{lang.t.products_new_title}</h2>
      <div class="sub">{lang.t.products_new_sub}</div>
    </div>
  </div>

  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      create();
    }}
  >
    <div class="field wide">
      <label class="label" for="new-name">{lang.t.products_name}</label>
      <input
        class="control"
        id="new-name"
        bind:value={name}
        placeholder={lang.t.products_name_placeholder}
        autocomplete="off"
      />
    </div>
    <div class="field">
      <label class="label" for="new-unit">{lang.t.products_unit}</label>
      <!-- One option, deliberately: see the note above the script. -->
      <select class="control" id="new-unit">
        <option value="unit">unit</option>
      </select>
    </div>
    <div class="field">
      <label class="label" for="new-price">{lang.t.sales_price_unit}</label>
      <input
        class="control figures"
        id="new-price"
        bind:value={priceText}
        class:bad={priceBad}
        inputmode="decimal"
        placeholder="0.00"
      />
    </div>
    <div class="field">
      <label class="label" for="new-stock">{lang.t.products_initial_stock}</label>
      <input
        class="control figures"
        id="new-stock"
        bind:value={stockText}
        class:bad={stockBad}
        inputmode="numeric"
        placeholder="0"
      />
    </div>
    <button type="submit" class="create" disabled={!canCreate}>
      {creating ? lang.t.products_creating : lang.t.products_create}
    </button>
  </form>

  {#if created}
    <p class="ok" role="status">{lang.t.products_created(created)}</p>
  {/if}
  {#if error}
    <p class="error" role="alert">
      <strong>{lang.t.products_create_error}</strong>
      {error}
    </p>
  {/if}
  {#if priceBad}
    <p class="warn">{lang.t.products_invalid}</p>
  {/if}
</section>

<style>
  .body {
    margin: 22px 26px 0 28px;
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 4px;
  }
  .new {
    margin-bottom: 30px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 18px;
    padding: 14px 18px 12px;
    border-bottom: 1px solid var(--rule);
  }
  .head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 2px;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .search {
    width: 300px;
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
    white-space: nowrap;
  }
  .pager button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  table.catalog {
    width: 100%;
    border-collapse: collapse;
  }
  .catalog th {
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
  .catalog td {
    padding: 11px 12px;
    border-bottom: 1px solid var(--rule);
    vertical-align: middle;
  }
  .catalog th:first-child,
  .catalog td:first-child {
    padding-left: 18px;
  }
  .catalog th:last-child,
  .catalog td:last-child {
    padding-right: 18px;
  }
  .catalog tbody tr:hover {
    background: rgb(43 49 98 / 0.035);
  }
  .num {
    text-align: right;
  }
  .pname {
    font-size: var(--step-2);
  }
  .unit-chip {
    font-weight: 600;
    letter-spacing: 0.08em;
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    padding: 1px 6px;
    font-size: var(--step-1);
    color: var(--indigo);
  }
  .faces {
    font-size: var(--step-2);
    font-weight: 600;
  }
  .per {
    color: var(--ink-soft);
    font-family: var(--font-figure);
    font-size: var(--step-0);
    margin-left: 4px;
  }
  .stock {
    font-size: var(--step-3);
    font-weight: 700;
  }
  .stock.low {
    color: var(--crimson);
  }
  .stock.out {
    color: var(--ink-soft);
  }

  .row-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 11px 18px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
  .derived {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .stamp-mini {
    font-family: var(--font-figure);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.13em;
    font-size: var(--step--1);
    color: var(--crimson);
    border: 2px solid currentColor;
    border-radius: 2px;
    padding: 2px 7px 1px;
    transform: rotate(-6.5deg);
    mix-blend-mode: multiply;
  }

  .form {
    display: grid;
    grid-template-columns: minmax(0, 1.6fr) 120px 150px 130px auto;
    gap: 12px;
    align-items: end;
    padding: 16px 18px;
  }
  .field {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .form .control {
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    padding: 9px 11px;
  }
  .form .control.bad {
    border-color: var(--crimson);
    background: rgb(168 27 42 / 0.05);
  }
  .create {
    background: var(--crimson);
    color: #fff;
    border: 0;
    border-radius: var(--radius);
    padding: 11px 16px;
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    cursor: pointer;
    box-shadow: 0 2px 0 var(--crimson-deep);
    white-space: nowrap;
  }
  .create:disabled {
    background: var(--paper-sunk);
    color: var(--ink-soft);
    box-shadow: none;
    cursor: default;
  }

  .ok {
    margin: 0;
    padding: 10px 18px;
    border-top: 1px solid var(--rule);
    background: rgb(43 49 98 / 0.05);
    font-size: var(--step-0);
  }
  .error {
    margin: 0;
    padding: 10px 18px;
    border-top: 1px solid rgb(168 27 42 / 0.35);
    background: rgb(168 27 42 / 0.07);
    font-size: var(--step-0);
  }
  .error strong {
    font-family: var(--font-figure);
    color: var(--crimson-deep);
    margin-right: 6px;
  }
  .warn {
    margin: 0;
    padding: 0 18px 12px;
    color: var(--crimson-deep);
    font-size: var(--step-0);
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
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 14px;
    padding: 34px 18px;
    color: var(--ink-soft);
    font-size: var(--step-1);
  }
  .skeleton {
    display: grid;
    gap: 10px;
    padding: 18px;
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
    width: 58%;
  }
  .bar:nth-child(2) {
    width: 76%;
  }
  .bar:nth-child(3) {
    width: 44%;
  }
  @keyframes sweep {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -100% 0;
    }
  }

  .alert {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 14px 18px;
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

  @media (prefers-reduced-motion: reduce) {
    .bar {
      animation: none;
    }
  }

  @media (max-width: 1180px) {
    .head {
      flex-direction: column;
      align-items: flex-start;
    }
    .form {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .form .wide {
      grid-column: 1 / -1;
    }
    .create {
      grid-column: 1 / -1;
    }
  }
</style>
