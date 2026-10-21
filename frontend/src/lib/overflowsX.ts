/**
 * Marks an element with `overflows-x` while its content is wider than its box.
 *
 * Why this exists: a table that scrolls sideways is not the same fact as a table
 * the reader can see scrolls sideways. Measured on `/products` at 390px:
 * `scrollWidth 385 > clientWidth 334`, with `offsetHeight === clientHeight` — so
 * the engine drew no scrollbar, and the capture showed `STOCK` cut to `S` with
 * nothing to suggest the column continued. The same measurement on `/purchases`
 * cut `ESTADO` to `EST`.
 *
 * A `ResizeObserver` on both the frame and its content covers the three cases
 * that change the answer: the viewport resizing, the content re-rendering after
 * a search, and the page loading at a width different from the one it was
 * measured at. The class is removed, never left behind, because a fade over a
 * table that no longer overflows is a lie the other way.
 */
export function overflowsX(node: HTMLElement) {
  const set = (on: boolean) => node.classList.toggle('overflows-x', on);

  const measure = () => set(node.scrollWidth > node.clientWidth + 1);

  const observer = new ResizeObserver(measure);
  observer.observe(node);
  // The content can grow without the frame resizing (a longer product name, a
  // wider figure after a language switch), so it is observed separately.
  for (const child of Array.from(node.children)) observer.observe(child);

  measure();

  return {
    destroy() {
      observer.disconnect();
      set(false);
    }
  };
}
