<script lang="ts">
  /**
   * One document in the day's book: the slip, as both `/sales` and `/purchases`
   * print it.
   *
   * A document is a slip, and the two surfaces drew the same slip twice — the
   * same head, the same chevron, the same body, the same struck-through state,
   * the same keyboard contract — differing only in which fields they print and
   * what they open. That duplicated markup and 157 lines of identical CSS, and it
   * had already drifted once: the sales ledger grew a running balance in its head
   * and the purchases ledger rightly did not, and nothing in the code said which
   * of those was the rule rather than an accident.
   *
   * The two ledgers themselves are NOT one component, and this is not a step
   * towards making them one. Their scripts diverge in 621 lines because the
   * documents genuinely differ: a sale carries `credit_notes`, a purchase carries
   * `supplier_returns`, and only the sale has a running balance. Folding two
   * incompatible data models into one 2,600-line component behind direction flags
   * would create the conditional monster that makes the next change unreadable.
   * What was actually the same is the slip, so the slip is what moved here.
   *
   * Markup that differs goes through snippets rather than flags: `who`, `meta`
   * and `detail` are passed in, so each ledger still says in its own file what it
   * prints, and this component owns only the frame around them.
   */
  import type { Snippet } from 'svelte';
  import { formatMoney, formatTime } from '#lib/money.js';

  interface Props {
    /** The document's id. It is the identity a test selects by, never position. */
    id: string;
    /** When the document was recorded, rendered on the shop's clock. */
    createdAt: string;
    /** The shop's timezone, as the server names it (S19). */
    zone?: string;
    /** How much of the id is printed. Eight characters is the shop's convention. */
    idLength?: number;
    /**
     * The prefix of the detail panel's element id.
     *
     * Kept because the two surfaces already name their panels differently —
     * `lines-<id>` on sales, `purchase-lines-<id>` on purchases — and that id is
     * referenced by `aria-controls` and by tests. Renaming one to match the other
     * would be a change nobody asked for, so the difference is a prop.
     */
    idPrefix?: string;
    /** The document's amount. */
    total: string;
    /** Whether the document opens. */
    open: boolean;
    /**
     * Whether the document contributes nothing to the day: fully refunded or
     * fully returned. It strikes the total and still prints the balance it
     * inherits, because a blank in the column breaks the continuous read.
     */
    void: boolean;
    /**
     * The balance this document inherits, when the surface has one.
     *
     * Only `/sales` passes it. A purchase has no running balance in this design,
     * and the fact that this prop is optional is how that asymmetry is stated in
     * code instead of being rediscovered from a screenshot.
     */
    balance?: string;
    /** Toggles the document. The whole slip is the control. */
    ontoggle: () => void;
    /** Who the document is against: a customer, a supplier. */
    who: Snippet;
    /** The method and the state word, beside each other in the slip's meta line. */
    meta: Snippet;
    /** The lines, the notes and the form, rendered beneath the open slip. */
    detail: Snippet;
  }

  let {
    id,
    createdAt,
    zone,
    idLength = 8,
    idPrefix = 'lines-',
    total,
    open,
    void: isVoid,
    balance,
    ontoggle,
    who,
    meta,
    detail
  }: Props = $props();

  const shortId = $derived(id.slice(0, idLength));
</script>

<!-- The whole slip is the control. The chevron used to be the only thing that
     opened a document, which means aiming a mouse at a 12px triangle — on a shop
     PC, several times an hour. A slip is what a person points at when they want
     that document.

     No `<button>` wraps it: the slip is a keyboard-reachable element with its own
     role and state, and the visible chevron stays as the affordance that tells
     the eye it opens.

     No `aria-label`, deliberately. An accessible name on a container *replaces*
     everything inside it, so labelling the slip "Open the document #abc123" made
     a screen reader skip the time, the party, the state and the amount — the four
     things the slip exists to say. The document id is the first thing printed,
     and the expanded state is announced by `aria-expanded`. -->
<!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
<article
  class="slip doc"
  class:open
  class:void={isVoid}
  data-doc-row={id}
  role="button"
  tabindex="0"
  aria-expanded={open}
  aria-controls="{idPrefix}{id}"
  onclick={ontoggle}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      ontoggle();
    }
  }}
>
  <div class="slip-head">
    <div class="slip-id">
      <svg class="chev" class:open viewBox="0 0 12 12" aria-hidden="true" focusable="false">
        <path
          d="M4.5 2.5 8 6l-3.5 3.5"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <span class="figures slip-no">#{shortId}</span>
      <span class="figures slip-time">{formatTime(new Date(createdAt), zone)}</span>
    </div>
    {#if balance !== undefined}
      <span class="slip-bal figures" class:void={isVoid}>{formatMoney(balance)}</span>
    {/if}
  </div>

  <div class="slip-body">
    <div class="slip-who">
      {@render who()}
      <span class="who-meta">{@render meta()}</span>
    </div>
    <span class="slip-total figures" class:void={isVoid}>{formatMoney(total)}</span>
  </div>
</article>

{#if open}
  <div class="lines" id="{idPrefix}{id}" data-doc-detail={id}>
    {@render detail()}
  </div>
{/if}

<style>
  /* The slip is a document, not a table row: a closed surface whose head sits on
     the ground and whose body stays on white. No column is shared with any other
     document, which is what lets a long product name wrap without pushing every
     other row out of alignment. */
  .slip {
    background: var(--surface);
    border: 1px solid var(--rule);
    border-radius: var(--panel-radius);
    overflow: hidden;
    cursor: pointer;
  }
  .slip.open {
    border-color: var(--rule-strong);
  }
  .slip:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .slip.void {
    /* A document that contributes nothing is still readable, never removed: the
       correction is what the shop came to look at. */
    background: color-mix(in srgb, var(--ground) 60%, var(--surface));
  }

  .slip-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 14px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--rule);
    background: var(--ground);
  }
  .slip-id {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }
  .chev {
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--ink-soft);
    transition: transform 120ms ease;
    transform: translateY(1px);
  }
  .chev.open {
    transform: rotate(90deg) translateY(1px);
  }
  .slip-no {
    font-size: var(--step-2);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .slip-time {
    font-size: var(--step--1);
    color: var(--ink-soft);
    font-variant-numeric: tabular-nums;
  }

  /* The balance, where the surface keeps one. It is the statement's signature: a
     number that reads as one continuous column down the day. */
  .slip-bal {
    font-size: var(--step-1);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
    white-space: nowrap;
  }
  .slip-bal.void {
    color: var(--ink-soft);
    text-decoration: line-through;
  }

  .slip-body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 14px;
    align-items: end;
    padding: 8px 14px 10px;
  }
  .slip-who {
    min-width: 0;
  }
  .who-meta {
    display: block;
    font-size: var(--step--1);
    color: var(--ink-soft);
    margin-top: 1px;
  }
  .slip-total {
    font-size: var(--step-3);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
  }
  .slip-total.void {
    color: var(--ink-soft);
    text-decoration: line-through;
  }

  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
</style>
