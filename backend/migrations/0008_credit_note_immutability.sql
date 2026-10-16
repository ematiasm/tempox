-- v1.5 (P1, found by the independent verifier): a credit note is immutable, row
-- and all.
--
-- 0007 guarded a note's *economics* (`sale_id`, `total`) on UPDATE and wired the
-- blanket `reject_credit_note_mutation()` only to `BEFORE TRUNCATE`. Measured
-- against a live database after that migration: `UPDATE credit_notes SET
-- created_at = now()` succeeded, and so did `DELETE FROM credit_notes` for a note
-- with no child rows yet. `docs/en/architecture.md` already claimed the new
-- tables "reject UPDATE, DELETE and TRUNCATE", so the schema and the document
-- disagreed; the schema was the weaker of the two, and this closes the gap in
-- the direction the document describes.
--
-- Why the economics-only guard was not enough: `created_at` is what decides which
-- day's ledger a note appears on. A stored document whose date can be rewritten
-- is not a record — it is the same class of mistake as editing its total, and a
-- note has no legitimate UPDATE at all. A wrong note is corrected by another
-- compensating document, never by rewriting this one.
--
-- The TRUNCATE guard stays where it is, unchanged. This migration only tightens
-- the row-level rule.

CREATE OR REPLACE FUNCTION reject_credit_note_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'credit_notes are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

-- Replaced, not supplemented: `credit_notes_economics_guard` checked two columns
-- and let the rest through. Leaving both triggers installed would leave the
-- weaker one first in the queue, unchanged in effect.
DROP TRIGGER IF EXISTS credit_notes_economics_guard ON credit_notes;
DROP FUNCTION IF EXISTS guard_credit_note_economics();

DROP TRIGGER IF EXISTS credit_notes_immutable ON credit_notes;
CREATE TRIGGER credit_notes_immutable
BEFORE UPDATE OR DELETE ON credit_notes
FOR EACH ROW EXECUTE FUNCTION reject_credit_note_mutation();

-- The statement-level guard 0007 installed already points at this function, so
-- it is not recreated here. Asserted instead, because a migration that silently
-- lost it would leave the whole table truncatable:
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'credit_notes_no_truncate' AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION 'credit_notes_no_truncate is missing: the TRUNCATE guard was lost';
    END IF;
END;
$$;
