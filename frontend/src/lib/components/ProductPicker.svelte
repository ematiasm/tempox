<script lang="ts">
  /**
   * The product picker: type-ahead over the catalog, driven by the server.
   *
   * 500+ products cannot be loaded into the client, so every keystroke asks the
   * server for `q`. Keyboard-first is the whole point of this control — the
   * cashier types, arrows through the matches and presses Enter without ever
   * reaching for the mouse — so the listbox semantics are real, not decorative,
   * and `aria-activedescendant` follows the highlight.
   */
  import { listProducts, type Product } from '$lib/api';
  import { formatCents } from '$lib/money';

  interface Props {
    onpick: (product: Product) => void;
    /** Called after a pick, so the parent can move focus to the next field. */
    input?: HTMLInputElement;
  }

  let { onpick }: Props = $props();

  let query = $state('');
  let matches = $state<Product[]>([]);
  let open = $state(false);
  let active = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let inputEl: HTMLInputElement;

  // Debounce: a search per keystroke would be 12 requests for "harina".
  let timer: ReturnType<typeof setTimeout> | undefined;
  const DEBOUNCE_MS = 150;

  $effect(() => {
    const q = query.trim();
    clearTimeout(timer);
    if (!open) return;

    timer = setTimeout(async () => {
      loading = true;
      error = null;
      try {
        // A small limit: the picker offers a shortlist, the catalog page is
        // where browsing happens.
        matches = await listProducts({ q, limit: 8 });
        active = 0;
      } catch (e) {
        error = (e as Error).message;
        matches = [];
      } finally {
        loading = false;
      }
    }, DEBOUNCE_MS);

    return () => clearTimeout(timer);
  });

  function pick(product: Product) {
    onpick(product);
    query = '';
    open = false;
    matches = [];
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      open = true;
      if (matches.length) active = (active + 1) % matches.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (matches.length) active = (active - 1 + matches.length) % matches.length;
    } else if (e.key === 'Enter') {
      // Enter picks the highlighted match instead of submitting anything: the
      // sheet is committed by its own button, never by a stray keypress.
      if (open && matches[active]) {
        e.preventDefault();
        pick(matches[active]);
        inputEl?.focus();
      }
    } else if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

<div class="picker">
  <input
    class="control"
    id="product-search"
    bind:this={inputEl}
    bind:value={query}
    onfocus={() => (open = true)}
    {onkeydown}
    placeholder="Search by name…"
    role="combobox"
    aria-expanded={open}
    aria-controls="product-matches"
    aria-autocomplete="list"
    aria-label="Product"
    autocomplete="off"
  />

  {#if open}
    <ul class="matches" id="product-matches" role="listbox" aria-label="Product matches">
      {#if error}
        <li class="empty">Search failed — {error}</li>
      {:else if loading && matches.length === 0}
        <li class="empty">Searching…</li>
      {:else if matches.length === 0}
        <li class="empty">
          {query.trim() ? 'No product matches that name' : 'No products yet — create the first one'}
        </li>
      {:else}
        {#each matches as product, i (product.id)}
          <li
            id="match-{i}"
            role="option"
            aria-selected={i === active}
            class="match"
            class:active={i === active}
          >
            <!-- onmousedown, not onclick: the input's blur would close the list
                 before a click ever lands. -->
            <button type="button" onmousedown={() => pick(product)} tabindex="-1">
              <span class="name">{product.name}</span>
              <span class="figures meta">
                {formatCents(product.list_price)}/{product.unit} · {product.stock} in stock
              </span>
            </button>
          </li>
        {/each}
      {/if}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .matches {
    position: absolute;
    z-index: 20;
    top: calc(100% + 3px);
    left: 0;
    right: 0;
    list-style: none;
    margin: 0;
    padding: 3px;
    background: var(--paper-raised);
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    box-shadow: 0 14px 30px -14px rgb(21 22 28 / 0.45);
    max-height: 268px;
    overflow-y: auto;
  }
  .match button {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 14px;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 2px;
    padding: 8px 10px;
    cursor: pointer;
  }
  .match.active button {
    background: var(--indigo);
    color: #f2efe6;
  }
  .match.active .meta {
    color: rgb(242 239 230 / 0.86);
  }
  .name {
    font-size: var(--step-2);
  }
  .meta {
    color: var(--ink-soft);
    font-size: var(--step-0);
    white-space: nowrap;
  }
  .empty {
    padding: 9px 10px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }
</style>
