import { describe, expect, it } from 'vitest';
import { createStaleNotice } from '#lib/screenTrust.js';

/**
 * When a "this screen could not reload" warning is allowed to exist (S1-S4).
 *
 * The warning exists because a written document and a re-read of the day are two
 * different successes: a credit note can land while the reload that follows
 * fails, and reporting that as "not annulled" invited the cashier to write a
 * second one. So the warning says the document *does* exist and only the screen
 * is behind.
 *
 * Its first implementation got the lifetime backwards in both directions,
 * because nothing owned that lifetime: it was a map plus three `delete` calls
 * scattered across click handlers. It survived a successful reload,
 * contradicting figures that had just been brought up to date, and it was
 * erased by actions that had nothing to do with it. A warning an operator learns
 * to ignore is worse than no warning, because the day it is true they will not
 * read it.
 *
 * The rule that replaces it is one line: the notice lives exactly as long as the
 * stale state it describes. A successful reload retires it, a fresh failure
 * replaces it, and an action on a different document does not touch it.
 */

describe('createStaleNotice', () => {
  it('is empty when nothing has failed', () => {
    const notice = createStaleNotice();
    expect(notice.get('doc-1')).toBeUndefined();
    expect(notice.empty).toBe(true);
  });

  it('records a failure against the document it belongs to', () => {
    const notice = createStaleNotice();
    notice.mark('doc-1', 'listSales failed (500)');
    expect(notice.get('doc-1')).toBe('listSales failed (500)');
    expect(notice.empty).toBe(false);
  });

  it('keeps each document separate, so one failure never shows on another', () => {
    // The first version was a single global flag, which rendered the warning
    // under every expanded row. A document the cashier never touched must not be
    // told its screen is unreliable.
    const notice = createStaleNotice();
    notice.mark('doc-1', 'failed');
    expect(notice.get('doc-1')).toBe('failed');
    expect(notice.get('doc-2')).toBeUndefined();
  });

  it('replaces the message when the same document fails again', () => {
    const notice = createStaleNotice();
    notice.mark('doc-1', 'first failure');
    notice.mark('doc-1', 'second failure');
    expect(notice.get('doc-1')).toBe('second failure');
  });

  it('retires every notice when a reload succeeds, which is the whole rule', () => {
    // S1: the defect. A reload that works makes the warning a lie: the figures on
    // screen are current, so "this screen could not reload" contradicts them. No
    // reload path ever cleared it, so it was permanent.
    const notice = createStaleNotice();
    notice.mark('doc-1', 'could not reload');
    notice.mark('doc-2', 'could not reload');

    notice.retireAll();

    expect(notice.get('doc-1')).toBeUndefined();
    expect(notice.get('doc-2')).toBeUndefined();
    expect(notice.empty).toBe(true);
  });

  it('retires nothing when there was nothing to retire, and stays usable', () => {
    const notice = createStaleNotice();
    notice.retireAll();
    expect(notice.empty).toBe(true);
    notice.mark('doc-1', 'failed');
    expect(notice.get('doc-1')).toBe('failed');
  });

  it('forgets one document without touching the others', () => {
    // Starting a new refund on a row is a deliberate "I am dealing with this one
    // again"; it says nothing about any other row.
    const notice = createStaleNotice();
    notice.mark('doc-1', 'failed');
    notice.mark('doc-2', 'failed');

    notice.forget('doc-1');

    expect(notice.get('doc-1')).toBeUndefined();
    expect(notice.get('doc-2')).toBe('failed');
  });

  it('does not clear a warning because a different document had a failure', () => {
    // S2: the inverse defect. Marking another row, forgetting another row, or any
    // action that is not "this document was dealt with" or "the day reloaded"
    // must leave the original warning intact -- erasing it would hide the one
    // message the cashier still needs.
    const notice = createStaleNotice();
    notice.mark('doc-1', 'could not reload');

    notice.mark('doc-2', 'another failure');
    notice.forget('doc-2');
    notice.forget('doc-3');

    expect(notice.get('doc-1')).toBe('could not reload');
  });

  it('lists the documents it is currently warning about', () => {
    // The component renders a retry when this is non-empty, and a test asserts
    // nothing lingers after a good reload. Derived from the map rather than
    // tracked alongside it, so the two can never disagree.
    const notice = createStaleNotice();
    notice.mark('doc-1', 'failed');
    notice.mark('doc-2', 'failed');
    notice.forget('doc-1');
    expect(notice.documents()).toEqual(['doc-2']);
  });

  it('stops reporting a document once it is retired', () => {
    const notice = createStaleNotice();
    notice.mark('doc-1', 'failed');
    notice.retireAll();
    expect(notice.documents()).toEqual([]);
  });
});
