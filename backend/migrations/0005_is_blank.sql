-- One definition of "blank", for every guard that asks.
--
-- `R3-003`/`R3-004`/`R3-005` of `review-1abfbd892fed09cb` are one finding reported three
-- times, and measuring it turned up **seventeen** constraints carrying the same defect:
--
--     btrim(name) <> ''
--
-- `btrim`'s default trim set is a single space. Rust's `str::trim` removes the whole
-- `char::is_whitespace` set. So every one of those guards accepts a value made of a tab, a
-- newline, or a non-breaking space while the route in front of it refuses the same value:
-- two layers disagreeing about what "blank" means. `0003` fixed that for one constraint by
-- spelling the set out inline; this generalises it, because the inline spelling was itself
-- the thing that had to be remembered sixteen more times.
--
-- **`[[:space:]]` is not the answer either**, measured before choosing:
--
--     SELECT E'\u00A0' ~ '^[[:space:]]*$';   -- false
--
-- PostgreSQL's POSIX class does not include U+00A0. The set has to be spelled out, and it
-- has to be spelled out **once**.
--
-- The set below is exactly `char::is_whitespace`'s: space, tab, newline, carriage return,
-- vertical tab, form feed, the C1 NEL, and the Unicode separators from U+00A0 through
-- U+3000.

CREATE OR REPLACE FUNCTION is_blank(value text) RETURNS boolean AS $$
    SELECT btrim(
        value,
        E' \t\n\r\f\v' || U&'\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007'
                      || U&'\2008\2009\200A\2028\2029\202F\205F\3000'
    ) = ''
$$ LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE;

COMMENT ON FUNCTION is_blank(text) IS
    'True when the value is empty or consists only of whitespace, using the same set Rust''s str::trim removes. Every not-blank guard calls this instead of btrim, whose default set is one space.';

-- ── Every not-blank constraint now calls it ───────────────────────────────
--
-- **An explicit table rather than regex surgery over `pg_get_constraintdef`.** I tried the
-- regex first and it produced `CHECK ((btrim(name) <> '::text))` — a broken definition that
-- aborted the migration with "argument of NOT must be type boolean". Rewriting a constraint
-- by pattern is exactly the operation that has failed here before (`0020` copying a closed
-- set's members by hand); naming the table and column is duller and cannot silently mangle a
-- definition it misreads.
--
-- Every entry is `(table, constraint, column)`. The `DO` block below **refuses to continue**
-- if any one of them is missing, so a rename cannot make this file quietly do nothing.
DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT * FROM (VALUES
            ('accounts',              'accounts_name_not_blank',              'name'),
            ('expense_categories',    'expense_categories_name_not_blank',    'name'),
            ('payment_methods',       'payment_methods_name_not_blank',       'name'),
            ('annulments',            'annulments_created_by_not_blank',      'created_by'),
            ('count_adjustments',     'count_adjustments_created_by_not_blank','created_by'),
            ('expenses',              'expenses_created_by_not_blank',        'created_by'),
            ('expenses',              'expenses_payee_not_blank',             'payee'),
            ('money_entries',         'money_entries_created_by_not_blank',   'created_by'),
            ('obligations',           'obligations_created_by_not_blank',     'created_by'),
            ('obligations_adjustments','obligations_adjustments_created_by_not_blank','created_by'),
            ('opening_balances',      'opening_balances_created_by_not_blank','created_by'),
            ('payables',              'payables_created_by_not_blank',        'created_by'),
            ('payables_adjustments',  'payables_adjustments_created_by_not_blank','created_by'),
            ('payable_settlements',   'payable_settlements_created_by_not_blank','created_by'),
            ('payments',              'payments_created_by_not_blank',        'created_by'),
            ('settlements',           'settlements_created_by_not_blank',     'created_by'),
            ('transfers',             'transfers_created_by_not_blank',       'created_by')
        ) AS t(tbl, con, col)
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint
             WHERE conname = target.con
               AND conrelid = target.tbl::regclass
        ) THEN
            RAISE EXCEPTION
                'expected constraint % on table % to exist; this migration rewrites a fixed list and refuses to skip one',
                target.con, target.tbl;
        END IF;

        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', target.tbl, target.con);
        EXECUTE format(
            'ALTER TABLE %s ADD CONSTRAINT %I CHECK (NOT is_blank(%I))',
            target.tbl, target.con, target.col
        );
    END LOOP;
END;
$$;

-- ── Prove the sweep, in both directions ───────────────────────────────────
--
-- LAST, for the reason `0012` records. The negative half is the one that matters: a
-- constraint left on `btrim`'s default set is a guard that reads as present and does not
-- hold, and this is the only place that can say so.
DO $$
DECLARE
    stale bigint;
    using_it bigint;
BEGIN
    -- **`expense_categories_name_not_blank` is rewritten here too, and that is `R3-004`
    -- being closed rather than tolerated.** `0003` spelled out the ASCII set inline, which
    -- fixed tabs and newlines but not U+00A0 — measured: that constraint still accepted a
    -- name of one non-breaking space. The inline spelling was the bug, not the fix.
    SELECT count(*) INTO stale
      FROM pg_constraint
     WHERE contype = 'c'
       AND connamespace = 'public'::regnamespace
       AND pg_get_constraintdef(oid) LIKE '%btrim(%'
       AND pg_get_constraintdef(oid) NOT LIKE '%is_blank(%';

    IF stale > 0 THEN
        RAISE EXCEPTION
            '% not-blank constraint(s) still trim with btrim''s default set; refusing to continue',
            stale;
    END IF;

    SELECT count(*) INTO using_it
      FROM pg_constraint
     WHERE contype = 'c'
       AND connamespace = 'public'::regnamespace
       AND pg_get_constraintdef(oid) LIKE '%is_blank(%';

    IF using_it < 17 THEN
        RAISE EXCEPTION
            'expected at least 17 constraints to call is_blank, found %; refusing to continue',
            using_it;
    END IF;

    -- The function agrees with Rust on the cases that motivated it, and disagrees on a real
    -- value. Both halves, because a function that said "blank" to everything would pass the
    -- first three.
    IF NOT is_blank(U&'\00A0') OR NOT is_blank(E'\t\n') OR NOT is_blank('')
       OR is_blank('a') OR is_blank(U&'\00A0x') THEN
        RAISE EXCEPTION
            'is_blank does not agree with Rust about what whitespace is; refusing to continue';
    END IF;
END;
$$;
