<script lang="ts">
  /**
   * The catalog: what the shop sells, its list price, and the stock the ledger
   * says is on the shelf.
   *
   * Stock is never typed here. It arrives derived from the movement ledger, so
   * this screen can only ever report it — the one place stock changes is a
   * recorded event.
   */
  import { ApiError, createProduct, listProducts, type Product, type Unit } from '#lib/api.js';
  import { formatMoney, formatQuantity, parseAmount, parseInitialStock } from '#lib/money.js';
  import { lang } from '#lib/i18n/index.svelte.js';
  import { overflowsX } from '#lib/overflowsX.js';

  interface Props {
    /**
     * Reports what this page is currently showing: the rows on screen and
     * whether more exist beyond them.
     *
     * It deliberately does NOT report a total. `GET /api/products` returns a
     * plain array with no count, so the only number this component can compute
     * is `rows.length` — capped at `PAGE`. Calling that "the products the shop
     * sells" would print 50 for a 500-product catalog, which is the same defect
     * this branch already fixed twice on the money side: a total derived from
     * the loaded page and presented as the whole. Rather than ship a third one,
     * the header says what is true: how many are listed, and whether to page.
     * A real total needs a `COUNT(*)` from the API and is not this slice.
     */
    onshown?: (shown: { count: number; more: boolean } | null) => void;
  }

  let { onshown }: Props = $props();

  const PAGE = 50;

  let rows = $state<Product[]>([]);
  let offset = $state(0);
  let query = $state('');
  let loading = $state(true);
  let error = $state<string | null>(null);
  let pageError = $state<string | null>(null);

  // The create form. Unit is `unit` only in v1a: `g` and `ml` stay valid in the
  // database and the API, but a gram product's price is a currency amount, so
  // the finest price the schema can express is 1 cent. Offering `g` here would
  // let someone create a product whose real price cannot be stored.
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

  const priceAmount = $derived(parseAmount(priceText));
  // `parseInitialStock`, not `parseQuantity`: an empty field and a typed `0` both
  // mean the shelf is empty, which is the ordinary state of a product nobody has
  // bought yet. The sale parser refuses `0` on purpose (a movement of zero units
  // is not a movement) and reusing it here marked the field invalid, disabled the
  // create button, and blamed the name and the price for it.
  const stockValue = $derived(parseInitialStock(stockText));
  const priceBad = $derived(priceText.trim() !== '' && priceAmount === null);
  const stockBad = $derived(stockText.trim() !== '' && stockValue === null);
  const canCreate = $derived(name.trim() !== '' && priceAmount !== null && !stockBad && !creating);

  // Which field is wrong, named. One sentence for both fields used to read
  // "fill in the name and the price" while the price was filled and the stock was
  // not, so the reader was told to fix what was already right.
  const invalidField = $derived(
    priceBad ? lang.t.products_invalid_price : stockBad ? lang.t.products_invalid_stock : null
  );
  const missingField = $derived(
    name.trim() === ''
      ? lang.t.products_missing_name
      : priceAmount === null
        ? lang.t.products_missing_price
        : null
  );

  /** The price label declares the unit, so the number can never be misread. */
  function priceLabel(unit: Unit) {
    return lang.fmt.priceUnit(unit);
  }

  async function create() {
    if (!canCreate) return;
    creating = true;
    error = null;
    created = null;
    try {
      const product = await createProduct({
        name: name.trim(),
        list_price: priceAmount as string,
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

  // The register's size, whenever a read lands. `null` when the header must stay
  // silent rather than claim a number it has not earned: while the first read is
  // in flight, and when the read FAILED. A failed read leaves `rows` empty, and
  // reporting that as 0 puts "0 PRODUCTOS" on a page whose catalog could not be
  // fetched — the screen would be stating a fact about the shop that it derived
  // from its own error.
  // How many columns the reader cannot see, measured by the `overflowsX` action.
  let hiddenColumns = $state(0);

  const shown = $derived(
    pageError || (loading && rows.length === 0)
      ? null
      : { count: rows.length, more: rows.length >= PAGE }
  );
  $effect(() => {
    onshown?.(shown);
  });
</script>

<!-- The band belongs to the route, not to this component: two regions rendering
     their own copy of the page's heading puts two <h1>Products</h1> on one page
     and leaves a screen reader with two rival titles. -->
<div class="workspace">
  <section class="panel panel-catalog" aria-labelledby="catalog-h">
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
      <!-- The rows scroll inside the panel. Without this frame the table grew to
           its content's height — 2,611px of it — inside a panel that clips at
           874, so everything past the first screenful was unreachable. The panel
           declared `overflow: hidden` and nothing scrolled: the redesign
           promised a scrolling catalog and shipped a truncated one. -->
      <div class="table-frame" use:overflowsX={(n) => (hiddenColumns = n)}>
        <table class="catalog">
          <thead>
            <tr>
              <th scope="col">{lang.t.sales_product}</th>
              <th scope="col" class="unit-col">{lang.t.products_unit}</th>
              <th scope="col" class="num">{lang.t.products_list_price}</th>
              <th scope="col" class="num">{lang.t.products_stock}</th>
            </tr>
          </thead>
          <tbody>
            {#each rows as product (product.id)}
              <tr>
                <td class="pname">{product.name}</td>
                <td class="unit-col">
                  <span class="unit-chip figures">{lang.fmt.unit(product.unit)}</span>
                </td>
                <td class="num">
                  <span class="faces figures">{formatMoney(product.list_price)}</span>
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
      <!-- The count moved to the page header, beside the heading, where the other
         two surfaces carry the size of what is on screen. What stays here is the
         one thing a reader of the stock column must know: those numbers are
         derived, never typed. It used to be a rotated stamp; DESIGN.md's Don't
         list names that shape, and a stamp is not how a computed column
         explains itself. -->
      <div class="row-foot">
        <span class="derived">
          <span class="derived-mark">{lang.t.products_derived}</span>
          {lang.t.products_derived_note}
        </span>
      </div>
    {/if}
  </section>

  <section class="panel panel-new" aria-labelledby="new-h">
    <div class="head">
      <div>
        <h2 id="new-h">{lang.t.products_new_title}</h2>
        <div class="sub">{lang.t.products_new_sub}</div>
      </div>
    </div>

    <form
      id="new-product"
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
          <option value="unit">{lang.fmt.unit('unit')}</option>
        </select>
      </div>
      <div class="field">
        <label class="label" for="new-price">{lang.fmt.priceUnit('unit')}</label>
        <input
          class="control figures"
          id="new-price"
          bind:value={priceText}
          class:bad={priceBad}
          inputmode="decimal"
          placeholder={lang.fmt.amountHint()}
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
    {#if invalidField}
      <p class="warn">{invalidField}</p>
    {/if}

    <!-- The page's single action, at the foot of the form it closes and always on
       screen. It used to sit inline in the form's grid, which put it after the
       fields on a wide page and below a paginated table on a narrow one — an
       action you have to scroll to find is one the shop will not use. The
       disabled state carries a hint for the same reason it does on /sales: a
       greyed control with no explanation is a control nobody learns exists. -->
    <div class="action-foot">
      {#if !canCreate && !creating && missingField}
        <span class="why">{missingField}</span>
      {/if}
      <button
        type="submit"
        class="do"
        form="new-product"
        disabled={!canCreate}
        aria-keyshortcuts="F2"
      >
        {creating ? lang.t.products_creating : lang.t.products_create} <kbd>F2</kbd>
      </button>
    </div>
  </section>
</div>

<style>
  /* Two closed panels, separated by a 24px gap AND their own edges, exactly as
     /sales and /purchases do it. The border is what separates them; the gap only
     keeps the rules from touching. The catalog is the wide column because it is
     what the screen is for — 500+ rows of it. */
  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(360px, 460px);
    gap: var(--panel-gap);
    align-items: stretch;
    padding: 0 26px 26px 28px;
    min-height: 0;
  }
  /* Grid items default to `min-height: auto`, which refuses to shrink below
     their content. Without this the panels keep their natural height, the grid
     track keeps theirs, and the action lands below the fold. */
  .workspace > .panel {
    min-height: 0;
  }
  .panel {
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: var(--panel-radius);
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }
  /* The catalog scrolls its rows between a fixed head and a fixed foot, so the
     page itself never scrolls and the panels divide a known height. */
  .panel-catalog {
    min-height: 480px;
  }
  .table-frame {
    position: relative;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
    scrollbar-gutter: stable;
    scrollbar-width: auto;
    scrollbar-color: var(--ink-soft) var(--rule);
  }
  /* The sideways cut, made visible without a gesture. Applied by the
     `overflowsX` action only while the content is actually wider than the box,
     so a table that fits wears nothing. */
  /* `:global` because the class is applied by the `overflowsX` action at
     runtime; the compiler cannot see it in the markup and would drop the rule. */
  /* The mirror of the right fade: once the reader has scrolled, columns have
     left the view on the left side and nothing said so. It is keyed to
     `overflows-left`, not to `can-scroll`, so it appears only when something is
     actually out of view in that direction. */
  .table-frame:global(.overflows-left)::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    bottom: 0;
    width: 28px;
    pointer-events: none;
    background: linear-gradient(to left, transparent, var(--surface) 72%);
  }
  .table-frame:global(.overflows-x)::after {
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
  .catalog thead th {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface);
  }
  .panel-new {
    overflow: visible;
  }

  @media (max-width: 1080px) {
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
    .panel-catalog {
      max-height: 70vh;
    }
  }

  /* At 390px the table scrolled sideways and cut `STOCK` to `S` — the one number
     this surface exists to report was the one you had to swipe to find. `UNIDAD`
     is the column that can go: every product here is sold by the unit, the unit
     is already printed inside the price cell (`$ / unidad`), and it is the same
     value on every row. Dropping it buys `STOCK` its place in the first view
     without hiding information the row does not carry twice. */
  @media (max-width: 620px) {
    .unit-col {
      display: none;
    }
    /* The headers carried `nowrap` so the desktop columns stay on one line. At
       390px that forced the table to 375px inside a 334px frame even after
       `UNIDAD` was dropped — `STOCK` ended at 344 and nothing was actually
       hidden, yet the frame still scrolled. Letting the two long labels wrap
       ("Precio de lista", "Precio de
lista") removes a scroll that revealed
       nothing, which is worse than no scroll: it teaches a swipe that gains the
       reader nothing. */
    .catalog th {
      white-space: normal;
    }
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
    background: var(--surface);
    border-radius: var(--radius);
    padding: 7px 11px;
    font-size: var(--step-0);
    font-weight: 600;
    color: var(--ink);
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
    color: var(--ink-soft);
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
    color: var(--ink);
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
  /* Stock is a state, so it uses the state colours and never the accent: the
     accent belongs to the balance on /sales and to the active destination, and a
     third job would dilute it.
     An empty shelf is soft ink, not the void red. Void is for a document that
     was annulled or refunded — something that went wrong. Zero stock is a fact
     the shop already knows about a product it just created, and painting every
     new row red teaches the operator to ignore the colour that matters. The
     number is printed either way, so nothing depends on the tone alone. */
  .stock.low {
    color: var(--state-pending);
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
  /* A label, not a stamp: `DESIGN.md`'s Don't list names the rotated rubber
     stamp, and a computed column does not need one to say it is computed. */
  .derived-mark {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 700;
    letter-spacing: 0.13em;
    text-transform: uppercase;
    color: var(--ink-soft);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    padding: 1px 6px;
  }

  /* The narrow column is a single stack: five fields squeezed side by side in
     460px is what put two labels on top of each other and pressed the action
     into the hint's line. The wide row keeps its two-up pairing. */
  .form {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
    align-content: start;
    padding: 16px 18px;
  }
  .form .wide {
    grid-column: auto;
  }
  .label {
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink-soft);
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
    border-color: var(--accent);
    background: rgb(168 27 42 / 0.05);
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
    color: #084a6c;
    margin-right: 6px;
  }
  .warn {
    margin: 0;
    padding: 0 18px 12px;
    color: #084a6c;
    font-size: var(--step-0);
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
    color: #084a6c;
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
  }

  /* The page's single action, pinned at the foot of the form it closes. Same
     component as /sales and /purchases: a solid ink bar, never the accent, and
     a dashed outline with a stated reason when it cannot fire yet. */
  .action-foot {
    flex-shrink: 0;
    margin-top: auto;
    padding: 16px 18px 18px;
    border-top: 1px solid var(--rule);
    background: var(--surface);
  }
  .action-foot .why {
    display: block;
    margin: 0 0 8px;
    font-size: var(--step--1);
    color: var(--ink-soft);
    text-align: center;
  }
  .do {
    width: 100%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: var(--ink);
    color: var(--surface);
    border: 1px solid var(--ink);
    border-radius: var(--radius);
    padding: 12px 18px;
    font-family: var(--font-figure);
    font-size: var(--step-2);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    cursor: pointer;
  }
  .do:hover:not(:disabled) {
    background: #0a1622;
  }
  .do:disabled {
    background: var(--surface);
    color: var(--ink-soft);
    border: 1px dashed var(--rule-strong);
    cursor: default;
  }
  .do kbd {
    font-family: var(--font-text);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: none;
    color: inherit;
    border: 1px solid currentColor;
    border-radius: 2px;
    padding: 1px 5px;
    opacity: 0.75;
  }
</style>
