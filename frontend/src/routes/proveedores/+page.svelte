<script lang="ts">
  /**
   * `/proveedores` — who we buy from, and the way into their account.
   *
   * `v5-customer-account.md` S1, S7. Reading only: the panel that shows a supplier.s
   * balance and unpaid sales is T3, and this page does not anticipate its shape
   * beyond handing it the id.
   */
  import { lang } from '#lib/i18n/index.svelte.js';
  import { listSuppliers, type Supplier } from '#lib/api.js';
  import PartyList from '#lib/components/PartyList.svelte';
  import AccountPanel from '#lib/components/AccountPanel.svelte';
  import { getSupplierAccount } from '#lib/api.js';
  import type { AccountLike } from '#lib/accountPanels.js';
  import type { Collection, SupplierAccount } from '#lib/api.js';

  let parties = $state<Supplier[] | null>(null);
  let error = $state<string | null>(null);

  // **A failed read and an empty list are different facts**, so the error is kept
  // separate from the data instead of rendering as `[]`. The list component renders
  // each of the three states (loading, failed, empty) differently for the same reason.
  $effect(() => {
    let live = true;
    listSuppliers()
      .then((rows) => {
        if (live) parties = rows;
      })
      .catch(() => {
        if (live) error = lang.t.suppliers_error;
      });
    return () => {
      live = false;
    };
  });

  /**
   * The selected party, and its account. **`?c=<id>` is the source of truth** (`S2`),
   * so a refresh, a bookmark or a link pasted to a colleague reopens the same panel.
   *
   * Three outcomes, and they must never look alike: the account loaded, the id does
   * not exist (`null` from the API — a `404`, not zeros), and the read failed. The
   * panel renders each differently, which is why the error is kept apart from the data
   * instead of being folded into an empty account.
   */
  let selected = $state<string | null>(null);
  let account = $state<AccountLike | null>(null);
  let notFound = $state(false);
  /** The last collection's applied rows, or `null` before one is recorded. */
  let collected = $state<Collection | null>(null);
  let panelError = $state<string | null>(null);

  function setSelected(id: string | null) {
    const url = new URL(window.location.href);
    if (id === null) url.searchParams.delete('c');
    else url.searchParams.set('c', id);
    // `replaceState`: Back must leave the page, not close the panel (S2).
    history.replaceState(history.state, '', url);
    selected = id;
  }

  function open(id: string) {
    setSelected(id);
  }

  /**
   * Replaces the panel's account with the one the collection's response carries.
   *
   * **No second request** (`S6` of `v5-payments.md`): the write answers with the account read
   * in its own transaction, so what the panel shows and what it just wrote describe one
   * instant. Asking again would reopen the window slice 1 closed — a balance from one moment
   * above a document list from another.
   *
   * The wire shape is mapped exactly as the `$effect` above maps it. That duplication is the
   * honest cost of one response shape serving two callers; a shared mapper would be a third
   * thing to keep in step, and it is the panel's own fields these two lines write.
   */

  /**
   * The wire's account, as the panel reads it.
   *
   * **One mapper, two callers**: the initial read and the collection's response. The two
   * differ only in the field names the API uses (`owed`/`owed_to_them`,
   * `in_favour`/`in_our_favour`, `unpaid_sales`/`unpaid_purchases`), which is the wire being
   * honest about direction, and this is where that difference stops.
   */
  function mapAccount(result: SupplierAccount): AccountLike {
    return {
      id: result.id,
      name: result.name,
      owed: result.owed_to_them,
      inFavour: result.in_our_favour,
      unpaid: result.unpaid_purchases.map((d) => ({
        id: d.id,
        sourceDocumentId: d.source_document_id,
        total: d.total,
        outstanding: d.outstanding
      }))
    };
  }

  function applyCollection(collection: Collection) {
    account = mapAccount(collection.account as SupplierAccount);
    notFound = false;
    panelError = null;
    collected = collection;
  }

  // Read the id from the URL on arrival, **and on `popstate`** (`R3-001`).
  //
  // `replaceState` means this page never pushes an entry, but the *user* can still
  // arrive at a different URL for it — a link pasted into the address bar, or Back
  // leaving the page and Forward returning. Without the listener the panel and the URL
  // disagree from that moment on: the URL says one party and the panel shows another,
  // which is the one failure a deep-linkable panel must not have.
  $effect(() => {
    const sync = () => {
      selected = new URL(window.location.href).searchParams.get('c');
    };
    sync();
    window.addEventListener('popstate', sync);
    return () => window.removeEventListener('popstate', sync);
  });

  // Fetch whenever the selection changes. `live` guards the race a second selection
  // opens: without it a slow reply for the first party can land after the second and
  // paint the wrong account under the right name.
  $effect(() => {
    const id = selected;
    account = null;
    notFound = false;
    panelError = null;
    // **The receipt belongs to the party it was written for** (`R3-002` of
    // `review-9cff43c50976184c`, carried out of slice 2 as informational). It is page
    // state, so a new selection has to clear it here — the panel is mounted again for
    // the new party and would otherwise render the previous party's `applied` rows under
    // this party's name. Nothing else needs doing: a receipt is the answer to a write,
    // and a write that just happened is not in flight while the selection changes.
    collected = null;
    if (id === null) return;
    let live = true;
    getSupplierAccount(id)
      .then((result) => {
        if (!live) return;
        if (result === null) notFound = true;
        else account = mapAccount(result);
      })
      .catch(() => {
        if (live) panelError = lang.t.suppliers_error;
      });
    return () => {
      live = false;
    };
  });
</script>

<div class="page">
  <header class="head">
    <h1>{lang.t.suppliers_title}</h1>
    <span class="tagline">{lang.t.suppliers_tagline}</span>
  </header>
  <section class="panel" aria-labelledby="suppliers-h">
    <h2 id="suppliers-h" class="sr-only">{lang.t.suppliers_title}</h2>
    <PartyList {parties} {error} empty={lang.t.suppliers_empty} onopen={open} />
  </section>
  {#if selected !== null}
    <AccountPanel
      side="supplier"
      {account}
      {notFound}
      error={panelError}
      onclose={() => setSelected(null)}
      oncollected={applyCollection}
      {collected}
    />
  {/if}
</div>

<style>
  .page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100%;
  }
  .head {
    padding: 22px 26px 16px 28px;
    display: flex;
    align-items: baseline;
    gap: 14px;
    flex-wrap: wrap;
  }
  .head h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .tagline {
    font-size: var(--step-0);
    color: var(--ink-soft);
  }
  /* A work panel: closed surface, visible edge, 8px radius (DESIGN.md). */
  .panel {
    margin: 0 26px 26px 28px;
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: 8px;
    overflow: auto;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
