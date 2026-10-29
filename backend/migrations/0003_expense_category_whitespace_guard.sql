-- v4-expenses E1-T1 fix: the non-blank guard trims whitespace, not just spaces.
--
-- `R3-001` of `review-32b53a14e35954c8`, a WARNING the review left as informational and
-- that is nevertheless a real hole:
--
-- > the DB nonblank guard uses `btrim(name) <> ''`, whose default trim set is spaces, so a
-- > direct SQL insert/update can store a name made only of tabs/newlines while the route
-- > trims every Rust whitespace.
--
-- **Measured, not argued**, on this database before the fix:
--
--     SELECT btrim(E'\t\n') = '';                    -- f   ← the CHECK passes
--     SELECT btrim(E'\t\n', E' \t\n\r\f\v') = '';    -- t   ← after the fix
--
-- and the consequence was reachable: `INSERT INTO expense_categories (name) VALUES
-- (E'\t')` succeeded. The route refuses that same value, because Rust's `str::trim`
-- removes the whole `char::is_whitespace` set, so the two layers disagreed — which is the
-- one thing `0002`'s own comment promised they would not do ("every refusal here is
-- enforced again in the database, because a `psql` session bypasses this file entirely").
--
-- **The trim set is spelled out rather than relying on `btrim`'s default**, and it matches
-- what Rust trims: space, tab, newline, carriage return, vertical tab and form feed. The
-- CHECK **validates and does not rewrite**, so a name legitimately stored with surrounding
-- whitespace keeps it — `\t Publicidad \n` is accepted and stored exactly as given. The
-- route is where trimming happens, because that is the layer that owns the request shape.
--
-- Written as a new version rather than an edit to `0002`, for the reason `0002` itself
-- records about `0001`: sqlx keys a migration by version *and* checksum, so editing an
-- applied file either fails on every database that ran it or silently skips the new SQL.

-- The old constraint is dropped by name and re-added: `CHECK` has no `ALTER ... UPDATE`.
ALTER TABLE expense_categories
    DROP CONSTRAINT IF EXISTS expense_categories_name_not_blank;

ALTER TABLE expense_categories
    ADD CONSTRAINT expense_categories_name_not_blank
    CHECK (btrim(name, E' \t\n\r\f\v') <> '');

-- ── Pre-check: no stored row may now violate the stricter rule ────────────
--
-- A `CHECK` added to a populated table validates existing rows, so a row written under the
-- old rule would abort the whole migration with whatever constraint happened to fail next.
-- This block turns that into a diagnostic that names the data and the remedy, in the
-- position that reports the real cause — the discipline `0012` records.
DO $$
DECLARE
    offenders bigint;
BEGIN
    SELECT count(*) INTO offenders
      FROM expense_categories
     WHERE btrim(name, E' \t\n\r\f\v') = '';

    IF offenders > 0 THEN
        RAISE EXCEPTION
            '% expense_categories row(s) hold a name made only of whitespace under the old, space-only guard. This migration does not delete them: inspect them and either give them a real name or retire them, because a category is history the moment an expense names it.',
            offenders;
    END IF;
END;
$$;

-- ── Prove the guard holds now, in both directions ─────────────────────────
--
-- The positive half matters as much as the negative one: a guard that refused every name
-- with whitespace in it would pass a one-sided assertion while breaking the shop's own
-- vocabulary. **Neither half inserts anything**, and that is deliberate — an inserted row
-- could not be removed again, because the delete trigger is exactly what this table keeps.
-- The two statements below evaluate the CHECK's own expression, which is the same thing the
-- constraint evaluates.
DO $$
DECLARE
    refused  boolean;
    accepted boolean;
    seeded   bigint;
BEGIN
    refused := (btrim(E'\t\n', E' \t\n\r\f\v') = '');
    IF NOT refused THEN
        RAISE EXCEPTION
            'the whitespace guard would still accept a tab-and-newline name; refusing to continue';
    END IF;

    accepted := (btrim(E'\t Publicidad \n', E' \t\n\r\f\v') <> '');
    IF NOT accepted THEN
        RAISE EXCEPTION
            'the whitespace guard would refuse a name with a real character in it; refusing to continue';
    END IF;

    -- **`R3-001` of `review-63c8b89cb1e8f768`, and it blocked the upgrade.** The first
    -- version asserted `count(*) = 6`, which is true only on a database that has never had
    -- a category created. `0002` exists so the shop *can* add its own — that is `D2`'s whole
    -- point — and the route creates them, so any database where somebody added "Fletes"
    -- failed this migration and could never apply the whitespace fix. A migration that
    -- refuses to run on correct data is worse than the hole it repairs.
    --
    -- What the assertion actually owed is weaker and still meaningful: **the six seeded
    -- names are all still there**, which is what proves this migration did not change the
    -- vocabulary while re-cutting a constraint over it. Expressed as "none of the seed is
    -- missing" instead of "nothing else exists", it holds on a database of any size.
    SELECT count(*) INTO seeded
      FROM expense_categories
     WHERE name IN (
             'Alquiler', 'Sueldos', 'Servicios',
             'Mantenimiento', 'Impuestos', 'Otros'
           );

    IF seeded <> 6 THEN
        RAISE EXCEPTION
            'expected all 6 seeded expense categories to still be present, found %; refusing to continue',
            seeded;
    END IF;
END;
$$;
