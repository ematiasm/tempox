-- v1.6: money becomes a decimal amount, and the product unit stops qualifying a price.
--
-- **What changed and why.** v1 stored money as an integer count of cents in
-- BIGINT. That was never a float problem — `checked_mul` and `checked_add` on
-- integers are exact. It was a *resolution* problem, and the resolution problem
-- came from the phrase "cents **per base unit**": pricing a gram product in
-- whole cents per gram makes 1¢/g the finest step, so $12.00/kg (1.2¢/g) could
-- not be written down at all (plan-v1a.md recorded it as "the resolution
-- limit"). The product `unit` was the thing that made a price need to be that
-- small. The unit is now decorative — a display label, not a dimension of the
-- price — so a price is just a currency amount and two decimals is right.
--
-- **Two decimals, enforced by value, in a column that declares six.** The
-- obvious shape is `NUMERIC(14,2)`, and it is wrong, for a reason that was
-- measured against a live database before this file was written:
--
--   * PostgreSQL rounds an over-precise value to the declared scale WITHOUT an
--     error: "If the scale of a value to be stored is greater than the declared
--     scale of the column, the system will round the value to the specified
--     number of fractional digits." Measured: `NUMERIC(14,2)` accepted `1.235`
--     and stored `1.24`.
--   * A CHECK is evaluated after that rounding, so with a declared scale of 2
--     *no guard can be written*: `CHECK (scale(v) <= 2)` accepted `1.235` too,
--     because by the time it ran the value was `1.24`.
--   * Declaring 6 and guarding on the *value* fixes it. `CHECK (v = round(v, 2))`
--     is independent of how the number was spelled and of what the column
--     declares: measured, it accepts `25.00`, `25.000000` and `0`, and rejects
--     `1.235` and `0.036` with a loud constraint violation.
--   * `CHECK (scale(v) <= 2)` was also measured and rejected as the guard: it is
--     representation-dependent (`scale('25.00') = 2` but `scale('25.000000') = 6`),
--     so it refuses a legitimate amount for the crime of having been written
--     with trailing zeroes.
--
-- So: the column stores what the application sent, and the guard refuses what is
-- not a whole number of cents. The last four digits of the scale are never used
-- by a correct writer; they exist so that an incorrect one is rejected instead
-- of silently rounded. Magnitude stays bounded by the declaration: 10^8, which
-- is 99,999,999.99 — a `numeric field overflow` is loud, and a guard for it
-- would not be.
--
-- **No data translation, and none is needed.** The database was empty when this
-- migration was written (the owner ran `docker compose down -v`). There is no
-- row where `250` means $2.50 and has to become `2.50`: a BIGINT column cannot
-- hold a cent, so the question does not arise. The cast below is a type change
-- on an empty table, not a value conversion.
--
-- **Money is never a float.** The Rust model is `rust_decimal::Decimal` and the
-- wire format is a decimal string; no binary float touches an amount anywhere in
-- this path. That is the rule v1 already had and it is unchanged.

-- ── Money: BIGINT cents -> decimal amounts ──────────────────────────────────

ALTER TABLE products
    ALTER COLUMN list_price TYPE NUMERIC(14,6);

ALTER TABLE sales
    ALTER COLUMN total TYPE NUMERIC(14,6);

ALTER TABLE sale_items
    ALTER COLUMN unit_price TYPE NUMERIC(14,6),
    ALTER COLUMN line_total TYPE NUMERIC(14,6);

-- The three constraints each of these tables already carries from 0002 and 0005
-- (`>= 0` on all four columns) are unaffected by the type change and stay exactly
-- as they are. These are the new ones, and they are about precision, not sign.

ALTER TABLE products
    ADD CONSTRAINT products_list_price_whole_cents
    CHECK (list_price = round(list_price, 2));

ALTER TABLE sales
    ADD CONSTRAINT sales_total_whole_cents
    CHECK (total = round(total, 2));

ALTER TABLE sale_items
    ADD CONSTRAINT sale_items_unit_price_whole_cents
    CHECK (unit_price = round(unit_price, 2));

ALTER TABLE sale_items
    ADD CONSTRAINT sale_items_line_total_whole_cents
    CHECK (line_total = round(line_total, 2));

-- `sales.total = SUM(sale_items.line_total)` is deliberately NOT enforced here.
-- It was a write-path invariant in v1 and it stays one; the owner deferred the
-- database-level version to the v1.5 reversal work, where a reversal is a
-- compensating row and a hard CHECK would fight it. Recorded so the absence
-- reads as a decision rather than an oversight.

-- ── The product unit becomes decorative ─────────────────────────────────────

-- 0003 locked a product's unit once it had history, on the reasoning that "its
-- stock movements and sale lines are denominated in the old unit". With the unit
-- no longer qualifying a price, that sentence is false and the lock has no
-- remaining job. The `unit` columns themselves and their
-- CHECK (unit IN ('unit','g','ml')) stay: the ledger still renders "1500 g"
-- verbatim, and the column is still what a receipt snapshots.
DROP TRIGGER IF EXISTS products_unit_guard ON products;
DROP FUNCTION IF EXISTS guard_product_unit();
