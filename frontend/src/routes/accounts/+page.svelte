<script lang="ts">
  import { lang } from '#lib/i18n/index.svelte.js';
  import { formatMoney } from '#lib/money.js';
  import {
    ApiError,
    createAccount,
    createPaymentMethod,
    listAccounts,
    patchAccount,
    patchPaymentMethod,
    type AccountWithMethods,
    type PaymentMethod
  } from '#lib/api.js';

  // The shop's money, account by account.
  //
  // **Every write here is a real operation on where the money lands**, which is why
  // this screen is deliberately plain: it lists, it renames, it retires, and it moves
  // the till. It does not try to be a dashboard. The figures are the loudest thing on
  // the page because that is what this world weights, and the actions sit inside the
  // row they act on rather than in a band across the top.
  //
  // **The till can be moved from here, and that is an accepted risk, not an
  // oversight.** `PRODUCT.md` says an affordance only the owner understands is a
  // defect, and roles are v5 — so today anyone who can reach this screen can move the
  // shop's default account. The alternative was to leave the operation API-only,
  // which hides a real job behind curl. The risk is named on the button's hint rather
  // than discovered later.
  let accounts = $state<AccountWithMethods[] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state<string | null>(null);
  // Distinct from `accounts === null`, which is also the failed state. Without it
  // the panel had to guess whether an empty list meant "still reading" or "the read
  // broke", and the first version guessed wrong in BOTH directions: a failure
  // rendered "no accounts yet" and a load rendered "could not read".
  let reading = $state(true);

  let newAccountName = $state('');
  let renaming = $state<string | null>(null);
  let renameText = $state('');
  let newMethodFor = $state<string | null>(null);
  let newMethodName = $state('');

  async function reload() {
    // **`reading` is only for the FIRST read.** It used to be set on every call, so
    // every write — create, rename, promote, retire — replaced the whole list with
    // "reading" for the length of its round-trip. Two consequences, both real: the
    // screen flashed on every action, and the `<ul>` unmounted and remounted, which
    // is what made the e2e flake — the locator was valid, then detached, then valid
    // again, and Playwright lost it mid-wait.
    //
    // The `busy` key already says an action is in flight, per row. A refresh after a
    // write keeps the previous list on screen and swaps it when the new one arrives.
    if (accounts === null) reading = true;
    try {
      accounts = await listAccounts();
      error = null;
    } catch (e) {
      error = e instanceof ApiError ? `${lang.t.accounts_read_failed} (${e.status})` : String(e);
      accounts = null;
    } finally {
      reading = false;
    }
  }

  // One read on mount. No polling: nothing on this screen changes except by an action
  // taken here, and every action reloads.
  $effect(() => {
    void reload();
  });

  /** Runs one write, reports its failure, and re-reads. Keeps the busy key per row so
   * a slow write does not freeze the whole screen. */
  async function write(key: string, action: () => Promise<unknown>) {
    busy = key;
    try {
      await action();
      await reload();
    } catch (e) {
      error = e instanceof ApiError ? `${lang.t.accounts_write_failed} (${e.status})` : String(e);
    } finally {
      busy = null;
    }
  }

  function submitNewAccount(e: SubmitEvent) {
    e.preventDefault();
    const name = newAccountName.trim();
    if (!name) return;
    void write('new-account', async () => {
      await createAccount(name);
      newAccountName = '';
    });
  }

  function submitRename(account: AccountWithMethods) {
    const name = renameText.trim();
    if (!name) return;
    void write(`rename-${account.id}`, async () => {
      await patchAccount(account.id, { name });
      renaming = null;
    });
  }

  function submitNewMethod(account: AccountWithMethods) {
    const name = newMethodName.trim();
    if (!name) return;
    void write(`method-${account.id}`, async () => {
      await createPaymentMethod(name, account.id);
      newMethodName = '';
      newMethodFor = null;
    });
  }

  /**
   * Promote an account to the shop's till.
   *
   * The server clears the previous default in the same transaction, because the
   * database guarantees exactly one at commit — so this is one call, not two, and the
   * client never has to take the till away from somewhere first.
   */
  function makeDefault(account: AccountWithMethods) {
    void write(`default-${account.id}`, () => patchAccount(account.id, { is_default: true }));
  }

  /** Retire or restore a method. Retiring never deletes: history keeps resolving it. */
  function toggleMethod(method: PaymentMethod) {
    void write(`toggle-${method.id}`, () =>
      patchPaymentMethod(method.id, { is_active: !method.is_active })
    );
  }
</script>

<div class="page">
  <header class="head">
    <div class="head-line">
      <h1>{lang.t.accounts_title}</h1>
      <span class="tagline">{lang.t.accounts_tagline}</span>
    </div>
  </header>

  {#if error !== null}
    <p class="alert" role="alert">{error}</p>
  {/if}

  <div class="panels">
    <section class="panel">
      <div class="panel-head">
        <h2>{lang.t.accounts_title}</h2>
        <form class="new" onsubmit={submitNewAccount}>
          <input
            bind:value={newAccountName}
            placeholder={lang.t.accounts_new_placeholder}
            aria-label={lang.t.accounts_new_placeholder}
          />
          <button class="action" type="submit" disabled={busy !== null || !newAccountName.trim()}>
            {lang.t.accounts_new}
          </button>
        </form>
      </div>

      {#if reading}
        <p class="empty">{lang.t.accounts_loading}</p>
      {:else if accounts === null}
        <!-- The alert above already names the failure; this line says there is
             nothing to show, which is a different statement from "no accounts". -->
        <p class="empty">{lang.t.accounts_read_failed}</p>
      {:else if accounts.length === 0}
        <p class="empty">{lang.t.accounts_empty}</p>
      {:else}
        <ul class="list">
          {#each accounts as account (account.id)}
            <li class="row" class:is-default={account.is_default}>
              <div class="row-head">
                {#if renaming === account.id}
                  <form
                    class="rename"
                    onsubmit={(e) => {
                      e.preventDefault();
                      submitRename(account);
                    }}
                  >
                    <input bind:value={renameText} aria-label={lang.t.accounts_rename} />
                    <button class="action" type="submit" disabled={busy !== null}>
                      {lang.t.accounts_save}
                    </button>
                  </form>
                {:else}
                  <span class="name">{account.name}</span>
                  <button
                    class="link"
                    onclick={() => {
                      renaming = account.id;
                      renameText = account.name;
                    }}
                  >
                    {lang.t.accounts_rename}
                  </button>
                {/if}

                {#if account.is_default}
                  <span class="tick">{lang.t.accounts_is_default}</span>
                {:else}
                  <button
                    class="link"
                    title={lang.t.accounts_till_hint}
                    disabled={busy !== null}
                    onclick={() => makeDefault(account)}
                  >
                    {lang.t.accounts_make_default}
                  </button>
                {/if}

                <!-- Formatted, not the raw wire string. The balance arrives as
                     `0.00`; every other money figure in this app renders through the
                     active language, and a screen that printed the wire form would be
                     the one place `$ 1.234,56` became `1234.56`. The e2e caught it
                     because it asserts what a person reads, not what the API returns. -->
                <span class="figure">{formatMoney(account.balance)}</span>
              </div>

              <div class="methods">
                <span class="methods-label">{lang.t.accounts_methods}</span>
                {#if account.methods.length === 0}
                  <span class="none">—</span>
                {:else}
                  {#each account.methods as method (method.id)}
                    <span class="method" class:retired={!method.is_active}>
                      {method.name}
                      <button
                        class="link"
                        disabled={busy !== null}
                        onclick={() => toggleMethod(method)}
                      >
                        {method.is_active ? lang.t.accounts_deactivate : lang.t.accounts_activate}
                      </button>
                    </span>
                  {/each}
                {/if}

                {#if newMethodFor === account.id}
                  <form
                    class="inline"
                    onsubmit={(e) => {
                      e.preventDefault();
                      submitNewMethod(account);
                    }}
                  >
                    <input
                      bind:value={newMethodName}
                      placeholder={lang.t.accounts_new_method_placeholder}
                      aria-label={lang.t.accounts_new_method_placeholder}
                    />
                    <button class="action" type="submit" disabled={busy !== null}>
                      {lang.t.accounts_save}
                    </button>
                  </form>
                {:else}
                  <button
                    class="link"
                    onclick={() => {
                      newMethodFor = account.id;
                      newMethodName = '';
                    }}
                  >
                    {lang.t.accounts_new_method}
                  </button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>

<style>
  .page {
    padding: 24px;
    display: grid;
    gap: 24px;
    align-content: start;
  }

  .head-line {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  h1 {
    font-family: var(--figure);
    font-size: var(--step-3);
    font-weight: 600;
    margin: 0;
  }

  .tagline {
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  .alert {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--rule-strong);
    border-left: 2px solid var(--state-void);
    border-radius: var(--rounded-control);
    color: var(--state-void);
    font-size: var(--step-0);
  }

  .panel {
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: var(--rounded-panel);
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--rule);
  }

  h2 {
    font-family: var(--figure);
    font-size: var(--step-1);
    font-weight: 600;
    margin: 0;
  }

  .new,
  .rename,
  .inline {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  input {
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--rule-strong);
    border-radius: var(--rounded-control);
    padding: 10px 11px;
    font-size: var(--step-0);
  }

  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  button {
    font-family: inherit;
    cursor: pointer;
  }

  .action {
    background: var(--ink);
    color: var(--surface);
    border: 1px solid var(--ink);
    border-radius: var(--rounded-control);
    padding: 10px 14px;
    font-family: var(--figure);
    font-size: var(--step-0);
    font-weight: 600;
  }

  .action:disabled {
    background: var(--surface);
    color: var(--ink-soft);
    border: 1px dashed var(--rule-strong);
    cursor: not-allowed;
  }

  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font-size: var(--step--1);
    text-decoration: underline;
  }

  .link:disabled {
    color: var(--ink-soft);
    cursor: not-allowed;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .row {
    padding: 12px 16px;
    border-bottom: 1px solid var(--rule);
  }

  .row:last-child {
    border-bottom: none;
  }

  /* The till is marked by a left rule, not a filled block: DESIGN.md's state rule is
     a tick plus readable text, never a badge. */
  .row.is-default {
    border-left: 2px solid var(--accent);
    padding-left: 14px;
  }

  .row-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .name {
    font-family: var(--figure);
    font-size: var(--step-1);
    font-weight: 600;
  }

  .tick {
    font-size: var(--step--1);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

  .figure {
    margin-left: auto;
    font-family: var(--figure);
    font-variant-numeric: tabular-nums;
    font-size: var(--step-2);
    font-weight: 600;
    color: var(--accent);
  }

  .methods {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 6px;
    font-size: var(--step--1);
    color: var(--ink-soft);
  }

  .methods-label {
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .method {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    color: var(--ink);
  }

  /* A retired method stays legible: documents still resolve it, and hiding it would
     make an old row unreadable. */
  .method.retired {
    color: var(--state-pending);
  }

  .none {
    color: var(--ink-soft);
  }

  .empty {
    margin: 0;
    padding: 16px;
    color: var(--ink-soft);
    font-size: var(--step-0);
  }

  @media (max-width: 860px) {
    .page {
      padding: 16px;
    }

    .panel-head {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
