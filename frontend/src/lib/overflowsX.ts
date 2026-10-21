/**
 * Marks an element with `overflows-x` while its content is wider than its box,
 * and reports how many columns sit beyond the visible cut.
 *
 * Why this exists: a table that scrolls sideways is not the same fact as a table
 * the reader can see scrolls sideways. Measured on `/products` at 390px:
 * `scrollWidth 385 > clientWidth 334`, with `offsetHeight === clientHeight` — so
 * the engine drew no scrollbar, and the capture showed `STOCK` cut to `S` with
 * nothing to suggest the column continued. The same measurement on `/purchases`
 * cut `ESTADO` to `EST`.
 *
 * The fraction is the part that says how much is missing, not merely that
 * something is: the finish review asked for "a count of what is beyond it",
 * because a fade alone says "there is more" without saying whether it is one
 * column or five, and a reader cannot decide whether the swipe is worth it. It
 * counts `<th>` elements whose right edge passes the box's right edge, which is
 * the reader's own question — how many columns am I not seeing.
 *
 * A `ResizeObserver` on both the frame and its content covers the three cases
 * that change the answer: the viewport resizing, the content re-rendering after
 * a search, and the page loading at a width different from the one it was
 * measured at. The class and the attribute are removed, never left behind,
 * because a cue over a table that no longer overflows is a lie the other way.
 */
export function overflowsX(node: HTMLElement, onchange?: (hidden: number) => void) {
  const set = (on: boolean, hidden = 0) => {
    node.classList.toggle('overflows-x', on);
    if (on && hidden > 0) node.setAttribute('data-hidden-columns', String(hidden));
    else node.removeAttribute('data-hidden-columns');
    // The component renders the count from this, so the measurement stays the
    // only source of truth and there is no second copy of it in Svelte state.
    onchange?.(on ? hidden : 0);
  };

  const measure = () => {
    const over = node.scrollWidth > node.clientWidth + 1;
    if (!over) {
      node.classList.remove('can-scroll', 'overflows-left');
      return set(false);
    }

    // Count the headers whose right edge is past the visible cut. Both rects are
    // viewport-relative, so this stays honest at any scroll offset: scroll right
    // and the count falls, which is exactly what the reader sees.
    const edge = node.getBoundingClientRect().right;
    const headers = Array.from(node.querySelectorAll('th')) as HTMLElement[];
    const hidden = headers.filter((h) => h.getBoundingClientRect().right > edge + 1).length;

    // Two independent facts, and the first version conflated them. `can-scroll`
    // says the content is wider than the box — that never changes. `overflows-x`
    // says *something is still past the right edge* — that clears when the reader
    // reaches the end, because the fade it drives must not sit on top of the last
    // column at maximum scroll, covering content when nothing remains to its
    // right. Meanwhile `overflows-left` marks the mirror case, so the left edge
    // says a column has been scrolled out of view.
    node.classList.toggle('can-scroll', true);
    node.classList.toggle('overflows-left', node.scrollLeft > 1);
    set(hidden > 0, hidden);
  };

  const observer = new ResizeObserver(measure);
  observer.observe(node);
  // The content can grow without the frame resizing (a longer product name, a
  // wider figure after a language switch), so it is observed separately.
  for (const child of Array.from(node.children)) observer.observe(child);
  node.addEventListener('scroll', measure, { passive: true });

  measure();

  return {
    destroy() {
      observer.disconnect();
      node.removeEventListener('scroll', measure);
      set(false);
    }
  };
}
