<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  import SaleDraft from '$lib/components/SaleDraft.svelte';
  import Ledger from '$lib/components/Ledger.svelte';

  // Bumping the token tells the ledger a sale was just recorded, so the day
  // reloads and the new document appears with its stamp.
  let refreshToken = $state(0);

  // The composition puts the page's single next action in the band. The sheet
  // exposes its own `record()`, so the band button and Ctrl+Enter are one code
  // path rather than two that can drift.
  let draft = $state<SaleDraft | undefined>(undefined);
  let ready = $state(false);

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'F2') {
      e.preventDefault();
      document.getElementById('product-search')?.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

<header class="band">
  <div>
    <h1>{lang.t.sales_title}</h1>
    <p>{lang.t.sales_tagline}</p>
  </div>
  <button
    class="action"
    disabled={!ready}
    onclick={() => draft?.record()}
    aria-keyshortcuts="F2 Control+Enter"
  >
    {lang.t.sales_action} <kbd>F2</kbd>
  </button>
</header>

<div class="workspace">
  <SaleDraft bind:this={draft} onrecorded={() => refreshToken++} onready={(r) => (ready = r)} />
  <Ledger {refreshToken} />
</div>

<style>
  .band {
    background: var(--crimson);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 26px 14px 28px;
    min-height: 78px;
  }
  .band h1 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.1;
  }
  .band p {
    color: rgb(255 255 255 / 0.94);
    font-size: var(--step-0);
    margin-top: 2px;
  }
  .action {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    background: var(--paper-raised);
    color: var(--ink);
    border: 0;
    border-radius: var(--radius);
    padding: 12px 18px;
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
    box-shadow:
      0 2px 0 rgb(0 0 0 / 0.18),
      0 8px 18px -10px rgb(0 0 0 / 0.55);
    cursor: pointer;
  }
  .action:hover:not(:disabled) {
    background: #fff;
  }
  .action:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .action kbd {
    font-family: var(--font-text);
    font-size: var(--step--1);
    font-weight: 600;
    color: var(--ink-soft);
    border: 1px solid var(--rule-strong);
    border-radius: 2px;
    padding: 2px 5px;
    background: var(--paper);
  }

  .workspace {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 24px;
    padding: 22px 26px 0 28px;
  }
</style>
