<script lang="ts">
  import { lang } from '$lib/i18n/index.svelte';
  // T3 scope: the shell this route lives in. The draft sheet arrives in T5 and
  // the ledger in T6; until then the page shows the composition's two regions
  // with their honest empty states rather than rows invented to look finished.
</script>

<header class="band">
  <div>
    <h1>{lang.t.sales_title}</h1>
    <p>{lang.t.sales_tagline}</p>
  </div>
  <button class="action" disabled>{lang.t.sales_action} <kbd>F2</kbd></button>
</header>

<div class="workspace">
  <section class="card" aria-labelledby="draft-h">
    <div class="card-head">
      <h2 id="draft-h">{lang.t.sales_draft_title}</h2>
      <span class="hint">Not recorded yet</span>
    </div>
    <p class="pending">{lang.t.sales_draft_pending}</p>
  </section>

  <section class="ledger" aria-labelledby="ledger-h">
    <div class="ledger-head">
      <div>
        <h2 id="ledger-h">{lang.t.sales_ledger_title}</h2>
        <div class="sub">{lang.t.sales_ledger_sub}</div>
      </div>
    </div>
    <p class="pending">{lang.t.sales_ledger_pending}</p>
  </section>
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
    grid-template-columns: minmax(0, 1fr) 336px;
    gap: 22px;
    padding: 22px 26px 0 28px;
    align-items: start;
  }

  .card {
    background: var(--paper-raised);
    border: 1px solid var(--rule);
    border-radius: 4px;
  }
  .card-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 18px 12px;
    border-bottom: 1px solid var(--rule);
  }
  .card-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-3);
    font-weight: 600;
  }
  .hint {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  /* Honest placeholders: a region that is not built yet says so, rather than
     showing sample rows a reader could mistake for the shop's real day. */
  .pending {
    padding: 26px 18px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  .ledger {
    padding-bottom: 30px;
  }
  .ledger-head {
    padding-bottom: 12px;
  }
  .ledger-head h2 {
    font-family: var(--font-figure);
    font-size: var(--step-4);
    font-weight: 600;
    line-height: 1.05;
  }
  .ledger-head .sub {
    color: var(--ink-soft);
    font-size: var(--step-0);
    margin-top: 3px;
  }

  @media (max-width: 1180px) {
    .workspace {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
