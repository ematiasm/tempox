/**
 * The lifetime of a "this screen could not reload" warning (S1-S4).
 *
 * A written document and a successful re-read are two different things. A credit
 * note can land on the server while the reload that follows it fails, and the
 * two failures mean opposite things to the cashier: "the refund did not happen"
 * invites a retry, while "the refund happened and this screen is behind" does
 * not. Reporting both as the first wrote a second partial note on top of the
 * first, which is why the second message exists at all.
 *
 * That message then needs an owner. Its first implementation was a map plus three
 * `delete` calls spread across click handlers, and it got the lifetime backwards
 * in both directions: a successful reload never cleared it, so it stayed on
 * screen contradicting figures that had just been brought up to date, and
 * unrelated actions erased it. An operator who learns to ignore a warning is
 * worse off than one who never saw it, because the day it is true they will not
 * read it.
 *
 * So the rule lives here, whole: a notice lives exactly as long as the stale
 * state it describes.
 *
 * - `mark` a document when its reload failed.
 * - `retireAll` when a reload finally succeeds. This is the rule, not a cleanup:
 *   once the figures are current, a warning about them being stale is a lie.
 * - `forget` one document when the cashier deliberately deals with that document
 *   again. It never touches another document's notice.
 *
 * Deliberately pure and free of Svelte state, so the rule is pinned by a unit
 * test instead of by clicking through two failure modes in a browser. The same
 * reason `salesRefund.ts` and `supplierReturns.ts` exist. The backing map is
 * injectable for exactly one reason: a Svelte component passes a `SvelteMap` so
 * the warning re-renders when it is retired, without this module importing
 * anything from Svelte.
 */
export interface StaleNotice {
  /** The warning for a document, or `undefined` when it is up to date. */
  get(documentId: string): string | undefined;
  /** Record that this document's reload failed, replacing any earlier message. */
  mark(documentId: string, message: string): void;
  /** The cashier is dealing with this document again; its notice no longer applies. */
  forget(documentId: string): void;
  /** A reload succeeded: nothing on this screen is stale any more. */
  retireAll(): void;
  /** The documents currently carrying a warning, in insertion order. */
  documents(): string[];
  /** Whether any warning is showing. Cheaper to read than `documents().length`. */
  readonly empty: boolean;
}

export function createStaleNotice(notices: Map<string, string> = new Map()): StaleNotice {
  return {
    get(documentId) {
      return notices.get(documentId);
    },

    mark(documentId, message) {
      notices.set(documentId, message);
    },

    forget(documentId) {
      notices.delete(documentId);
    },

    retireAll() {
      notices.clear();
    },

    documents() {
      return [...notices.keys()];
    },

    get empty() {
      return notices.size === 0;
    }
  };
}
