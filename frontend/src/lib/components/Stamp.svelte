<script lang="ts">
  /**
   * The stamp — the direction's one signature move, and the only authored
   * motion in this surface.
   *
   * A confirmed sale wears CONFIRMED in crimson ink; an annulled one wears
   * ANULADO struck through, in indigo-black. An annulment is a correction, not
   * an alarm: colouring it crimson would make a cancelled document look like
   * the worst thing on the page and would spend the accent that belongs to the
   * confirmed stamp.
   *
   * `land` runs the press: the stamp comes down oversized and settles. It is
   * off for every stamp on an initial render, so a list of forty documents does
   * not perform forty presses at once; the caller turns it on for the document
   * that has just been created.
   */
  interface Props {
    label: string;
    tone?: 'confirmed' | 'annulled';
    size?: 'cell' | 'inline' | 'hero';
    land?: boolean;
  }

  let { label, tone = 'confirmed', size = 'inline', land = false }: Props = $props();
</script>

<span
  class="stamp"
  class:land
  class:annulled={tone === 'annulled'}
  class:cell={size === 'cell'}
  class:hero={size === 'hero'}
>
  <span class="ink" data-text={label}>{label}</span>
</span>

<style>
  .stamp {
    display: inline-block;
    font-family: var(--font-figure);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.13em;
    font-size: var(--step-1);
    line-height: 1.1;
    color: var(--crimson);
    border: 2px solid currentColor;
    border-radius: 2px;
    padding: 4px 10px 3px;
    /* Real registration: a stamp is never square to the page. */
    transform: rotate(-6.5deg);
    /* Ink bites the paper instead of floating over it. */
    mix-blend-mode: multiply;
    position: relative;
    white-space: nowrap;
  }

  .stamp.cell {
    font-size: var(--step-1);
    letter-spacing: 0.12em;
    padding: 3px 8px 2px;
  }

  .stamp.hero {
    font-size: var(--step-4);
    padding: 8px 16px 6px;
  }

  /* The uneven second impression of a rubber stamp: the same word, offset a
   * hair, at a fraction of the opacity. */
  .ink::after {
    content: attr(data-text);
    position: absolute;
    inset: 4px 10px 3px 11px;
    opacity: 0.28;
  }
  .stamp.cell .ink::after {
    inset: 3px 8px 2px 9px;
  }
  .stamp.hero .ink::after {
    inset: 8px 16px 6px 17px;
  }

  .annulled {
    color: var(--ink);
  }
  /* Struck through: the cancellation is legible as one, without a second word. */
  .annulled::before {
    content: '';
    position: absolute;
    left: -4px;
    right: -4px;
    top: 50%;
    height: 2px;
    background: currentColor;
    transform: rotate(-1.5deg) translateY(-1px);
    opacity: 0.85;
  }

  .land {
    animation: stamp-land 420ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes stamp-land {
    0% {
      transform: rotate(-6.5deg) scale(1.42);
      opacity: 0;
    }
    62% {
      opacity: 0.92;
    }
    100% {
      transform: rotate(-6.5deg) scale(1);
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .land {
      animation: none;
    }
  }
</style>
