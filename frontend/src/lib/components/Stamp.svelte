<script lang="ts">
  /**
   * A document's state, as text plus a 2px tick — never a filled pill and never
   * a rotated rubber stamp.
   *
   * The statement world has one rule for state: it is a fact printed in the
   * margin, not a badge glued on. A printed account marks a reversed entry with
   * a word and a rule across it; it does not decorate it. So this renders the
   * word in the document's own voice, with a short tick of colour beside it that
   * carries the meaning:
   *
   *   - `confirmed` is ink. Nothing is wrong, so the state does not borrow the
   *     accent or a warning colour. A page of confirmations should read quiet.
   *   - `annulled` is the correction red, and the row it belongs to is struck
   *     through by its owner, not by this component.
   *   - `pending` is amber, for a document that exists but has not settled.
   *
   * Colour is the only thing these three share; everything else is the same
   * shape, so the eye reads state by hue and position rather than by a change of
   * silhouette.
   */
  interface Props {
    label: string;
    tone?: 'confirmed' | 'annulled' | 'pending';
  }

  let { label, tone = 'confirmed' }: Props = $props();
</script>

<span class="state" class:annulled={tone === 'annulled'} class:pending={tone === 'pending'}>
  <span class="tick" aria-hidden="true"></span>{label}
</span>

<style>
  /* The tick is decorative and the word is the state, so the word is not
     hidden from a screen reader and the tick is. */
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-figure);
    font-size: var(--step--1);
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--ink);
    white-space: nowrap;
  }

  .tick {
    width: 2px;
    height: 12px;
    background: currentColor;
    flex-shrink: 0;
  }

  .annulled {
    color: var(--state-void);
  }

  .pending {
    color: var(--state-pending);
  }
</style>
