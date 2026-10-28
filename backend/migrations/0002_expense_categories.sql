-- v4-expenses E1-T1: a category is a row, never a closed set.
--
-- `odd/tasks/v4-expenses.md` S1 and `D2`. An expense is money leaving that touches no
-- stock — the everyday member of the family `0018` opened with three one-off documents —
-- and how the shop classifies it is shop data.
--
-- **Why this is `0002` and not a section appended to `0001`.** `0001` is the squashed
-- chain and it is already applied everywhere: `sqlx` keys a migration by version *and*
-- checksum, so editing it either fails on every existing database or silently skips the
-- new SQL on one that had already recorded version 1. A new version is the only shape
-- that runs everywhere.
--
-- **Why a table rather than a closed set.** This schema has paid for enumerating a
-- vocabulary three times: `stock_movements.reason` (`0011` replaced six such constraints
-- with one shape-derived trigger), `money_entries.document_kind` (`0020` copied the
-- members by hand and **silently dropped `annulment`**, turning every annulment into a
-- 500), and `obligations.source_document_kind` (`0023` paid for an entire duplicated
-- table to avoid re-cutting it). Adding a category must cost an `INSERT`.
--
-- **`is_active` and no delete**, for the reason `payment_methods` has the same pair: an
-- expense that named a category is history, so the row cannot go away, and a picker must
-- be able to stop offering it. The trigger below is the mirror of
-- `reject_payment_method_delete`, including the `TRUNCATE` half — a row-level guard never
-- sees `TRUNCATE`, the lesson `0004` recorded and `0008` repaired.
--
-- **The names are not translated.** A category is shop data, exactly as a supplier's name
-- is: both languages render the same row, and a dictionary would make two shops'
-- categories disagree while looking identical.

CREATE TABLE IF NOT EXISTS expense_categories (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    is_active  BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT expense_categories_name_not_blank CHECK (btrim(name) <> '')
);

-- ── The seed, in the shop's own words ─────────────────────────────────────
--
-- Six rows, and they are a starting vocabulary rather than a rule: the shop adds its own
-- with a `POST`. Fixed ids are **deliberately absent**, unlike `cash`/`walk-in`/
-- `one-off-supplier`: nothing in the server resolves a category without a lookup, so a
-- fixed id would be a constant nobody reads and a constraint on rows that may be renamed.
INSERT INTO expense_categories (name) VALUES
    ('Alquiler'),
    ('Sueldos'),
    ('Servicios'),
    ('Mantenimiento'),
    ('Impuestos'),
    ('Otros')
ON CONFLICT (name) DO NOTHING;

-- ── A category cannot be deleted ──────────────────────────────────────────
CREATE OR REPLACE FUNCTION reject_expense_category_delete() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        'expense_categories cannot be deleted: is_active is how a category is retired';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS expense_categories_no_delete ON expense_categories;
CREATE TRIGGER expense_categories_no_delete
BEFORE DELETE ON expense_categories
FOR EACH ROW EXECUTE FUNCTION reject_expense_category_delete();

DROP TRIGGER IF EXISTS expense_categories_no_truncate ON expense_categories;
CREATE TRIGGER expense_categories_no_truncate
BEFORE TRUNCATE ON expense_categories
FOR EACH STATEMENT EXECUTE FUNCTION reject_expense_category_delete();

-- ── Prove the guards exist ────────────────────────────────────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('expense_categories', 'expense_categories_no_delete'),
              ('expense_categories', 'expense_categories_no_truncate')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 2 THEN
        RAISE EXCEPTION
            'expected 2 expense_categories guards, found %; refusing to continue', present;
    END IF;

    IF (SELECT count(*) FROM expense_categories) <> 6 THEN
        RAISE EXCEPTION
            'expected the 6 seeded expense categories, found %; refusing to continue',
            (SELECT count(*) FROM expense_categories);
    END IF;
END;
$$;
