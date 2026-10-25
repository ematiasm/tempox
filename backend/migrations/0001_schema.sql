-- Tempox schema, in one migration.
--
-- **This is the whole chain, squashed.** It replaces twenty-two files that applied in
-- sequence (`0001`..`0023`, with `0016` absent — it was deleted when the `credit` account was
-- retired, and the gap was never renumbered because `sqlx` keys migrations by version). The
-- application has never shipped and no database holds data worth carrying, so the owner's
-- decision was to collapse the history rather than keep twenty-two increments that exist only
-- to explain how the schema got here.
--
-- **Nothing was rewritten, only concatenated**, in the original order. That matters: several
-- migrations re-cut a function that an earlier one wrote (`CREATE OR REPLACE` is not additive,
-- and `0022` silently dropped a `FOR UPDATE` doing it once), and others alter a column a third
-- later relaxed. Reordering or hand-merging would have reintroduced exactly that class of
-- defect. The equivalence was **measured, not assumed**: applied to an empty database, its
-- `pg_dump --schema-only` matches the pre-squash schema object for object.
--
-- **What is lost, and it is real:** the per-migration commentary that explained why each rule
-- exists. The decisions themselves live in `odd/tasks/*.md` — `v4-settlement.md`,
-- `v4-receivables.md`, `v4-payables.md`, `v3-accounts.md` and the rest — which is where a reader
-- should look for the reasoning. Every `-- ── section ──` heading from the originals is kept
-- below, because that is the schema's table of contents.
--
-- Reading order, if you are new: the tables first (`notes`, `sales`, `purchases`, then the
-- ledger `stock_movements` and `money_entries`), then the guards that make them append-only,
-- then `accounts`/`payment_methods` and the documents' destinations, then v4's debts
-- (`obligations`, `payables`) and their settlements.


-- ═══════════════════════════════════════════════════════════════════════════
-- From 0001_notes.sql
-- ═══════════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS notes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    text       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS notes_created_at_idx ON notes (created_at DESC);

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0002_v1_sales_stock.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v1: sales slice — products, customers, sales, sale_items, stock_movements.
-- Money is stored as BIGINT cents. The walk-in customer is the default for
-- sales with no named customer.

CREATE TABLE IF NOT EXISTS customers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS products (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    list_price BIGINT NOT NULL CHECK (list_price >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sales (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id    UUID NOT NULL REFERENCES customers (id),
    total          BIGINT NOT NULL CHECK (total > 0),
    payment_method TEXT NOT NULL DEFAULT 'cash',
    status         TEXT NOT NULL DEFAULT 'confirmed'
                   CHECK (status IN ('draft', 'confirmed', 'cancel', 'done')),
    reversed_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sale_items (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sale_id      UUID NOT NULL REFERENCES sales (id),
    product_id   UUID NOT NULL REFERENCES products (id),
    product_name TEXT NOT NULL,
    quantity     INTEGER NOT NULL CHECK (quantity > 0),
    unit_price   BIGINT NOT NULL CHECK (unit_price > 0),
    line_total   BIGINT NOT NULL CHECK (line_total > 0)
);

CREATE TABLE IF NOT EXISTS stock_movements (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products (id),
    quantity   INTEGER NOT NULL CHECK (quantity <> 0),
    reason     TEXT NOT NULL,
    sale_id    UUID REFERENCES sales (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- walk-in: the default customer for sales with no named customer.
-- A real fiscal category, seeded with a fixed id so the API can reference it.
INSERT INTO customers (id, name)
VALUES ('00000000-0000-0000-0000-000000000001', 'walk-in')
ON CONFLICT DO NOTHING;

CREATE INDEX IF NOT EXISTS sales_created_at_idx ON sales (created_at DESC);
CREATE INDEX IF NOT EXISTS sale_items_sale_id_idx ON sale_items (sale_id);
CREATE INDEX IF NOT EXISTS stock_movements_product_id_idx ON stock_movements (product_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0003_units_and_history_guards.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v1 units of measure + history guards.
-- Quantities are integers in the product's base unit (pieces, grams, ml) —
-- the same smallest-unit pattern as money in cents.

ALTER TABLE products ADD COLUMN IF NOT EXISTS unit TEXT NOT NULL DEFAULT 'unit'
    CHECK (unit IN ('unit', 'g', 'ml'));

ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS unit TEXT NOT NULL DEFAULT 'unit'
    CHECK (unit IN ('unit', 'g', 'ml'));

-- A product with history cannot change its unit: its stock movements and
-- sale lines are denominated in the old unit.
CREATE OR REPLACE FUNCTION guard_product_unit() RETURNS trigger AS $$
BEGIN
    IF NEW.unit <> OLD.unit THEN
        IF EXISTS (SELECT 1 FROM stock_movements WHERE product_id = OLD.id)
           OR EXISTS (SELECT 1 FROM sale_items WHERE product_id = OLD.id) THEN
            RAISE EXCEPTION 'unit of product % cannot change: it has history', OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS products_unit_guard ON products;
CREATE TRIGGER products_unit_guard
BEFORE UPDATE OF unit ON products
FOR EACH ROW EXECUTE FUNCTION guard_product_unit();

-- Sale lines are pure history. A correction is a compensating row (reversal),
-- never an edit.
CREATE OR REPLACE FUNCTION reject_sale_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'sale_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sale_items_immutable ON sale_items;
CREATE TRIGGER sale_items_immutable
BEFORE UPDATE OR DELETE ON sale_items
FOR EACH ROW EXECUTE FUNCTION reject_sale_item_mutation();

-- A sale's economics are immutable. Only the status transition may be
-- written (reversal sets status/reversed_at) — never total, customer, or
-- payment method.
CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total <> OLD.total
       OR NEW.customer_id <> OLD.customer_id
       OR NEW.payment_method <> OLD.payment_method THEN
        RAISE EXCEPTION 'sale % economics are immutable: status may change, money may not', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sales_economics_guard ON sales;
CREATE TRIGGER sales_economics_guard
BEFORE UPDATE ON sales
FOR EACH ROW EXECUTE FUNCTION guard_sale_economics();

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0004_ledger_guards.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v1 (P1): make the movement ledger append-only and reason-coherent.
--
-- plan-v1.md calls the ledger the reason "why did stock change?" is always
-- answerable, and decision 15 says the database enforces the history guards.
-- 0003 guarded sale_items and a sale's economics, but the ledger itself stayed
-- freely editable: nothing in the schema stopped `UPDATE stock_movements` or
-- `DELETE FROM stock_movements`. Verified against a live database before this
-- migration: both were allowed, which silently breaks stock reconciliation and
-- makes the balance's provenance unknowable.
--
-- The product-delete hole is fixed transitively: a product's history is its
-- movements, and once they cannot be removed, the existing
-- stock_movements_product_id_fkey keeps the product in place. A product with
-- no movements and no sale lines is still deletable, which is correct — it has
-- no history to lose.

-- Every balance change is an append. A correction is a compensating row
-- (reserved for the v1.5 reversal), never an edit to a row already written.
CREATE OR REPLACE FUNCTION reject_ledger_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'stock_movements are append-only: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS stock_movements_append_only ON stock_movements;
CREATE TRIGGER stock_movements_append_only
BEFORE UPDATE OR DELETE ON stock_movements
FOR EACH ROW EXECUTE FUNCTION reject_ledger_mutation();

-- TRUNCATE does not fire FOR EACH ROW triggers: it is a set operation, so the
-- trigger above never sees it and the whole ledger can be emptied in one
-- statement. This statement-level guard closes that path. Verified before
-- writing it: TRUNCATE stock_movements was allowed and left zero rows.
DROP TRIGGER IF EXISTS stock_movements_no_truncate ON stock_movements;
CREATE TRIGGER stock_movements_no_truncate
BEFORE TRUNCATE ON stock_movements
FOR EACH STATEMENT EXECUTE FUNCTION reject_ledger_mutation();

-- Only the two reasons v1 defines. The grammar has to stay closed: an
-- unrecognized reason is a balance change whose cause cannot be named, and the
-- v1.5 reversal adds its own value in a new migration rather than reusing one.
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale'));

-- A sale movement without its sale cannot be traced back to the document that
-- caused it; an initial movement pointing at a sale contradicts itself.
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_sale_id_coherent
    CHECK (
        (reason = 'sale' AND sale_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL)
    );

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0005_bonus_lines.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v1 (P3): a bonus line is a real line whose price is zero.
--
-- plan-v1.md allows `products.list_price >= 0` but forced
-- `sale_items.unit_price > 0`, so a product created at price 0 could be
-- created and then not sold through the default path: the omitted
-- `unit_price` resolved to the product's list price, the money check
-- rejected it, and the caller got a 400 for a product the catalog accepted.
--
-- The rule that wins: a line may be given away. A bonus (regalo) is a real
-- event — the stock leaves the shop, so the row must exist — that happens to
-- carry no money. Three constraints move together, because relaxing only the
-- per-line ones would let the application accept a sale and the schema
-- reject it: a sale whose lines are all bonus lines totals zero too.
--
-- Negative amounts stay impossible everywhere: money cannot be un-earned by
-- typing a minus sign. The strictness moves from `> 0` to `>= 0`, nothing more.

ALTER TABLE sale_items
    DROP CONSTRAINT sale_items_unit_price_check,
    ADD CONSTRAINT sale_items_unit_price_check CHECK (unit_price >= 0);

ALTER TABLE sale_items
    DROP CONSTRAINT sale_items_line_total_check,
    ADD CONSTRAINT sale_items_line_total_check CHECK (line_total >= 0);

-- A fully bonified sale is worth 0 and still real: the stock moved, the
-- document exists, cash is simply unchanged by it.
ALTER TABLE sales
    DROP CONSTRAINT sales_total_check,
    ADD CONSTRAINT sales_total_check CHECK (total >= 0);

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0006_money_numeric.sql
-- ═══════════════════════════════════════════════════════════════════════════

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

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0007_credit_notes.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v1.5: reversing a sale with a credit note.
--
-- **The shape.** A sale is an immutable event (0003 locked its economics and
-- its lines). Correcting a mistake therefore cannot be an edit: it is a second
-- document — a *credit note* — that returns stock and money. The corrected sale
-- is never written: `status` stays `'confirmed'`, `reversed_at` stays NULL, its
-- `total` and `customer_id` are untouched. Reversal is a compensating row, and
-- this migration gives it somewhere to live.
--
-- **Why `status`/`reversed_at` are not used.** v1 reserved them for a lifecycle
-- (draft -> confirmed -> cancel -> done) that never happened, and 0003's
-- `guard_sale_economics` still permits those two columns to be written. A status
-- column answers "what state is this document in"; the question here is "how
-- much has been returned", which is a quantity that accumulates and must be
-- auditable line by line. A status cannot carry a partial refund, and it cannot
-- say which lines came back. The note carries both, so the note is the record
-- and the sale stays exactly as it was recorded.
--
-- **Partial and cumulative.** Several notes may be issued against one sale while
-- the quantity refunded per line never exceeds what was sold on that line. The
-- guard is enforced in the write path, inside the sale's row lock, so two
-- concurrent refunds cannot both pass it (see `credit_note_items_sale_item_id_idx`).
--
-- **Bonus lines stay coherent.** 0005 allows a line priced at zero, so a refund
-- returns what was charged: a bonus line returns `0.00` while its stock still
-- comes back. Amounts here follow that same `>= 0` rule, and money is the same
-- decimal contract 0006 established for the sale path.

-- ── The document ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS credit_notes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sale_id    UUID NOT NULL REFERENCES sales (id),
    total      NUMERIC(14,6) NOT NULL
               -- Same money contract as `sales.total` (0006): two decimals is
               -- money, the declared scale of 6 is what lets the guard refuse a
               -- finer value instead of PostgreSQL rounding it away first.
               CHECK (total >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credit_notes_total_whole_cents CHECK (total = round(total, 2))
);

-- The lines of the note. Name, unit and price are snapshots of the corrected
-- sale line, so the note reads as a self-contained receipt even if the catalog
-- changes later — the same defence-in-depth 0002 chose for `sale_items`.
CREATE TABLE IF NOT EXISTS credit_note_items (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credit_note_id UUID NOT NULL REFERENCES credit_notes (id),
    sale_item_id   UUID NOT NULL REFERENCES sale_items (id),
    product_id     UUID NOT NULL REFERENCES products (id),
    product_name   TEXT NOT NULL,
    quantity       INTEGER NOT NULL CHECK (quantity > 0),
    unit_price     NUMERIC(14,6) NOT NULL
                   CHECK (unit_price >= 0),
    line_total     NUMERIC(14,6) NOT NULL
                   CHECK (line_total >= 0),
    unit           TEXT NOT NULL CHECK (unit IN ('unit', 'g', 'ml')),
    CONSTRAINT credit_note_items_unit_price_whole_cents
        CHECK (unit_price = round(unit_price, 2)),
    CONSTRAINT credit_note_items_line_total_whole_cents
        CHECK (line_total = round(line_total, 2))
);

CREATE INDEX IF NOT EXISTS credit_notes_sale_id_idx ON credit_notes (sale_id);
CREATE INDEX IF NOT EXISTS credit_note_items_credit_note_id_idx
    ON credit_note_items (credit_note_id);
-- The cumulative guard reads this sum inside the sale's row lock; the index is
-- what keeps that read from being a scan of the whole table per refund.
CREATE INDEX IF NOT EXISTS credit_note_items_sale_item_id_idx
    ON credit_note_items (sale_item_id);

-- `credit_notes.total = SUM(its items' line_total)` is deliberately not a CHECK.
-- It is a write-path invariant, exactly as `sales.total = SUM(sale_items.line_total)`
-- is (0006 recorded the same decision for the sale path): a row-level CHECK
-- cannot see the sibling rows, and a deferred constraint would turn a bug in one
-- handler into a commit-time failure nobody can attribute.

-- ── History guards, in the style 0003 established ───────────────────────────

-- A note's lines are pure history too. A wrong note is corrected by another
-- compensating document, never by rewriting this one.
CREATE OR REPLACE FUNCTION reject_credit_note_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'credit_note_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_note_items_immutable ON credit_note_items;
CREATE TRIGGER credit_note_items_immutable
BEFORE UPDATE OR DELETE ON credit_note_items
FOR EACH ROW EXECUTE FUNCTION reject_credit_note_item_mutation();

-- The append-only rule above covers UPDATE and DELETE; this closes the one
-- statement that can empty the table without firing it. TRUNCATE does not run
-- FOR EACH ROW triggers — the same hole 0004 closed for `stock_movements`.
DROP TRIGGER IF EXISTS credit_note_items_no_truncate ON credit_note_items;
CREATE TRIGGER credit_note_items_no_truncate
BEFORE TRUNCATE ON credit_note_items
FOR EACH STATEMENT EXECUTE FUNCTION reject_credit_note_item_mutation();

-- A note's identity and its money are immutable. Nothing here may change after
-- the row exists.
CREATE OR REPLACE FUNCTION guard_credit_note_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.sale_id <> OLD.sale_id OR NEW.total <> OLD.total THEN
        RAISE EXCEPTION 'credit note % economics are immutable', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_economics_guard ON credit_notes;
CREATE TRIGGER credit_notes_economics_guard
BEFORE UPDATE ON credit_notes
FOR EACH ROW EXECUTE FUNCTION guard_credit_note_economics();

-- A statement-level TRUNCATE trigger cannot read NEW or OLD — there is no row
-- to pass it — so the row-level guard above cannot run here. This function only
-- raises, which is what a statement-level trigger can do.
CREATE OR REPLACE FUNCTION reject_credit_note_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'credit_notes are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_no_truncate ON credit_notes;
CREATE TRIGGER credit_notes_no_truncate
BEFORE TRUNCATE ON credit_notes
FOR EACH STATEMENT EXECUTE FUNCTION reject_credit_note_mutation();

-- ── The ledger learns one more reason ───────────────────────────────────────

-- 0004 left this note for exactly this migration: "the v1.5 reversal adds its
-- own value in a new migration rather than reusing one".
ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS credit_note_id UUID REFERENCES credit_notes (id);

ALTER TABLE stock_movements
    DROP CONSTRAINT stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale', 'credit_note'));

-- The movement still points at both documents it belongs to: the sale whose
-- stock left, and the note that brought it back. The old coherence rule is
-- replaced rather than kept, because it was written for a two-reason vocabulary
-- and would now be false for a credit-note row.
--
-- `sale_id` stays NOT NULL for a credit note on purpose. A refund that cannot be
-- traced to the document it corrects is not auditable, and the sale is exactly
-- what the ledger entry explains.
ALTER TABLE stock_movements
    DROP CONSTRAINT stock_movements_sale_id_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_document_coherent
    CHECK (
        (reason IN ('sale', 'credit_note') AND sale_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL)
    );
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_credit_note_coherent
    CHECK (
        (reason = 'credit_note') = (credit_note_id IS NOT NULL)
    );

-- A guard worth having: the two constraints above are only true if every row
-- already in the ledger satisfies them, and `ADD CONSTRAINT` will happily
-- succeed against an empty table while a later INSERT lands outside them. If a
-- pre-existing row contradicts the new vocabulary — a `'sale'` row with no
-- `sale_id`, say, which 0004 allowed only in the other direction — the migration
-- says so instead of leaving a constraint that the data already violates.
DO $$
DECLARE
    incoherent bigint;
BEGIN
    SELECT count(*) INTO incoherent FROM stock_movements
     WHERE (reason IN ('sale', 'credit_note') AND sale_id IS NULL)
        OR (reason = 'initial' AND sale_id IS NOT NULL);
    IF incoherent > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that contradict the v1.5 coherence rule; fix them before migrating',
            incoherent;
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS stock_movements_credit_note_id_idx
    ON stock_movements (credit_note_id);

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0008_credit_note_immutability.sql
-- ═══════════════════════════════════════════════════════════════════════════

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

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0009_purchases.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v2: the purchases slice — suppliers, purchases, purchase_items.
--
-- The mirror of 0002, with one deliberate asymmetry: a purchase has no status.
-- A sale carries a reserved lifecycle because v1's reversal needed it; a purchase
-- is a confirmed fact the moment it exists, so "the row exists" and "it counts"
-- stay the same statement (plan-v2.md, "Why not one document with a status").
--
-- One file, like 0002: the tables, the ledger's new reason and the guards are one
-- atomic schema change (v1 decision 9).

-- ── Suppliers ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS suppliers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- one-off-supplier: the default supplier for purchases with no named supplier,
-- exactly as `walk-in` is the default customer. A real category, seeded with a
-- fixed id so the API can reference it without a lookup.
--
-- The stored name is English and is a contract: it reaches the API and any
-- export. The localized display string ("Proveedor ocasional") lives in the
-- frontend dictionary, which is how `walk-in` survived es-AR unchanged.
INSERT INTO suppliers (id, name)
VALUES ('00000000-0000-0000-0000-000000000002', 'one-off-supplier')
ON CONFLICT DO NOTHING;

-- ── Purchases ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS purchases (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    supplier_id UUID NOT NULL REFERENCES suppliers (id),
    total       NUMERIC(14,6) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Sign and precision, the two rules 0006 established for money. `>= 0`
    -- rather than `> 0` matches `sales` since 0005 relaxed it: a zero-total
    -- document is expressible (free stock in, a bonus line out) and contributes
    -- nothing to the derived totals.
    CONSTRAINT purchases_total_check CHECK (total >= 0),
    CONSTRAINT purchases_total_whole_cents CHECK (total = round(total, 2))
);

CREATE TABLE IF NOT EXISTS purchase_items (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    purchase_id  UUID NOT NULL REFERENCES purchases (id),
    product_id   UUID NOT NULL REFERENCES products (id),
    product_name TEXT NOT NULL,
    quantity     INTEGER NOT NULL CHECK (quantity > 0),
    unit_price   NUMERIC(14,6) NOT NULL,
    line_total   NUMERIC(14,6) NOT NULL,
    unit         TEXT NOT NULL,

    CONSTRAINT purchase_items_unit_price_check CHECK (unit_price >= 0),
    CONSTRAINT purchase_items_unit_price_whole_cents CHECK (unit_price = round(unit_price, 2)),
    CONSTRAINT purchase_items_line_total_check CHECK (line_total >= 0),
    CONSTRAINT purchase_items_line_total_whole_cents CHECK (line_total = round(line_total, 2))
);

-- No unique constraint on (purchase_id, product_id): the same product can
-- legitimately arrive twice in one purchase at two different costs, and the
-- sales side already allows repeated lines.

CREATE INDEX IF NOT EXISTS purchases_created_at_idx ON purchases (created_at DESC);
CREATE INDEX IF NOT EXISTS purchases_supplier_id_idx ON purchases (supplier_id);
CREATE INDEX IF NOT EXISTS purchase_items_purchase_id_idx ON purchase_items (purchase_id);
CREATE INDEX IF NOT EXISTS purchase_items_product_id_idx ON purchase_items (product_id);

-- ── The ledger learns one more reason ───────────────────────────────────────
--
-- 0007 did this for 'credit_note' and left the note explaining why the
-- vocabulary grows in a new migration instead of reusing an existing value.
-- Same move here, extended to `purchase_id`.

ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS purchase_id UUID REFERENCES purchases (id);

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale', 'credit_note', 'purchase'));

-- The coherence rule is recreated rather than extended, because 0007 wrote it
-- against a three-reason vocabulary (`sale`, `credit_note` => sale_id NOT NULL;
-- everything else => sale_id NULL) and a purchase row would violate it.
--
-- `sale_id` stays NOT NULL for a credit note on purpose, as 0007 argued: a
-- refund that cannot be traced to the document it corrects is not auditable.
-- A purchase has no sale to point at, which is why the reasons split by which
-- document they carry instead of by "document or initial".
-- 0007's defensive check, and it matters MORE here than it did there, for two
-- reasons. First, this development database carries real sales and credit notes.
-- Second, and the one that cost a verification round: `ADD CONSTRAINT`
-- validates the existing rows, so if this pre-check ran *after* the constraints
-- it protects, PostgreSQL would report `check constraint ... is violated by some
-- row` and the migration's own count would never be printed — a diagnostic that
-- names the symptom instead of the cause. It therefore runs BEFORE them, right
-- after the reason vocabulary is recreated.
DO $$
DECLARE
    incoherent bigint;
BEGIN
    SELECT count(*) INTO incoherent FROM stock_movements
     WHERE (reason IN ('sale', 'credit_note') AND sale_id IS NULL)
        OR (reason = 'initial' AND (sale_id IS NOT NULL OR purchase_id IS NOT NULL))
        OR (reason NOT IN ('sale', 'credit_note', 'initial') AND sale_id IS NULL AND purchase_id IS NULL);
    IF incoherent > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that contradict the v2 coherence rule; fix them before migrating',
            incoherent;
    END IF;
END;
$$;

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_document_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_document_coherent
    CHECK (
        (reason IN ('sale', 'credit_note') AND sale_id IS NOT NULL)
        OR (reason = 'purchase' AND purchase_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL AND purchase_id IS NULL)
    );

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_purchase_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_purchase_coherent
    CHECK ((reason = 'purchase') = (purchase_id IS NOT NULL));

-- The direction of a purchase movement is not a style preference: stock comes in.
-- The ledger accepts negative quantities (a sale is one), so without this a
-- purchase could silently remove stock while the purchase document says it
-- arrived — the exact class of invisible inconsistency the append-only ledger
-- exists to prevent.
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_purchase_is_incoming;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_purchase_is_incoming
    CHECK (reason <> 'purchase' OR quantity > 0);


CREATE INDEX IF NOT EXISTS stock_movements_purchase_id_idx
    ON stock_movements (purchase_id);

-- ── Immutability guards ─────────────────────────────────────────────────────
--
-- Written correctly the first time, which is 0008's lesson learned in advance:
-- 0007 shipped a blanket reject function applied only to TRUNCATE, so UPDATE and
-- DELETE succeeded while the docs claimed they did not. Both operations are
-- guarded here from the start.
--
-- Unlike `sales`, there is no permitted UPDATE: a purchase has no status and no
-- reversed_at, so its economics and its timestamps are equally final.

CREATE OR REPLACE FUNCTION reject_purchase_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'purchases are immutable: a correction is a compensating document, not an edit';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION reject_purchase_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'purchase_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS purchases_immutable ON purchases;
CREATE TRIGGER purchases_immutable
BEFORE UPDATE OR DELETE ON purchases
FOR EACH ROW EXECUTE FUNCTION reject_purchase_mutation();

DROP TRIGGER IF EXISTS purchases_no_truncate ON purchases;
CREATE TRIGGER purchases_no_truncate
BEFORE TRUNCATE ON purchases
FOR EACH STATEMENT EXECUTE FUNCTION reject_purchase_mutation();

DROP TRIGGER IF EXISTS purchase_items_immutable ON purchase_items;
CREATE TRIGGER purchase_items_immutable
BEFORE UPDATE OR DELETE ON purchase_items
FOR EACH ROW EXECUTE FUNCTION reject_purchase_item_mutation();

DROP TRIGGER IF EXISTS purchase_items_no_truncate ON purchase_items;
CREATE TRIGGER purchase_items_no_truncate
BEFORE TRUNCATE ON purchase_items
FOR EACH STATEMENT EXECUTE FUNCTION reject_purchase_item_mutation();

-- 0008 ends with an assertion that its guard actually exists, because a silently
-- missing trigger is exactly the failure it was written to repair. The same check
-- belongs here, and it is cheap: four triggers, one query.
--
-- It runs LAST, after the triggers it checks are created, because that is the
-- only position where a failure names the real cause. This migration was first
-- written with the check in the middle, and the measurement was unambiguous: a
-- re-run reported the *constraint* that happened to fail next instead of the
-- guard that was actually absent. A diagnostic that names the wrong cause costs
-- more than no diagnostic, so the check sits after every CREATE TRIGGER.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('purchases', 'purchases_immutable'),
              ('purchases', 'purchases_no_truncate'),
              ('purchase_items', 'purchase_items_immutable'),
              ('purchase_items', 'purchase_items_no_truncate')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 4 THEN
        RAISE EXCEPTION
            'expected 4 v2 immutability triggers, found %; refusing to continue', present;
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0010_supplier_returns.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v2.5: returning goods to their supplier.
--
-- **The shape, and why it is a second document.** A purchase is an immutable
-- event: 0009 gave `purchases` and `purchase_items` triggers that reject UPDATE,
-- DELETE and TRUNCATE. Correcting one therefore cannot be an edit, and deleting
-- it would erase a real movement of money and stock. A return appends: it is a
-- document that names the purchase it corrects, and the purchase keeps every byte
-- it was recorded with.
--
-- **This is the mirror of 0007's credit note**, with the direction reversed.
-- There, goods came back from a customer and the till paid out. Here, goods go
-- back to a supplier and the till is paid back. The machinery is the same because
-- the problem is: partial, cumulative, and bounded per line.
--
-- **What this migration does NOT do.** It does not add a column to `purchases`.
-- Whether a purchase is fully returned is *derived* from its returns, exactly as
-- a sale's annulment is derived from its notes rather than written onto it.
--
-- **No purchase orders.** `plan-v2.5.md` records the owner's decision to defer
-- them, and nothing here anticipates that document.

-- ── The document ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS supplier_returns (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    purchase_id UUID NOT NULL REFERENCES purchases (id),
    total       NUMERIC(14,6) NOT NULL
                -- The same money contract 0006 established: two decimals is
                -- money, and the declared scale of 6 is what lets the guard
                -- refuse a finer value instead of PostgreSQL rounding it away
                -- before any CHECK runs.
                CHECK (total >= 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT supplier_returns_total_whole_cents CHECK (total = round(total, 2))
);

-- The lines. Name, unit and price are snapshots of the corrected purchase line,
-- so the return reads as a self-contained document even if the catalog changes —
-- the same defence-in-depth 0002 chose for `sale_items`.
--
-- There is no price override on purpose (plan-v2.5.md, decision 3): a return
-- values what was paid, read from the purchase line. The client never sends a
-- price, which removes the whole class of "the return is worth more than the
-- purchase". A supplier settling at a different value would be a different
-- document, not a field here.
CREATE TABLE IF NOT EXISTS supplier_return_items (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    supplier_return_id UUID NOT NULL REFERENCES supplier_returns (id),
    purchase_item_id   UUID NOT NULL REFERENCES purchase_items (id),
    product_id         UUID NOT NULL REFERENCES products (id),
    product_name       TEXT NOT NULL,
    quantity           INTEGER NOT NULL CHECK (quantity > 0),
    unit_price         NUMERIC(14,6) NOT NULL CHECK (unit_price >= 0),
    line_total         NUMERIC(14,6) NOT NULL CHECK (line_total >= 0),
    unit               TEXT NOT NULL,
    CONSTRAINT supplier_return_items_unit_price_whole_cents
        CHECK (unit_price = round(unit_price, 2)),
    CONSTRAINT supplier_return_items_line_total_whole_cents
        CHECK (line_total = round(line_total, 2))
);

CREATE INDEX IF NOT EXISTS supplier_returns_purchase_id_idx
    ON supplier_returns (purchase_id);
CREATE INDEX IF NOT EXISTS supplier_return_items_return_id_idx
    ON supplier_return_items (supplier_return_id);
-- The cumulative guard sums this column inside the purchase's row lock; the index
-- is what keeps that read from being a scan of the whole table per return.
CREATE INDEX IF NOT EXISTS supplier_return_items_purchase_item_id_idx
    ON supplier_return_items (purchase_item_id);

-- `supplier_returns.total = SUM(its items' line_total)` is deliberately not a
-- CHECK: it is a write-path invariant, as it is for sales (0006) and credit
-- notes (0007). A row-level CHECK cannot see sibling rows.

-- ── The ledger learns one more reason ───────────────────────────────────────

-- The vocabulary has grown this way twice now: 0007 added 'credit_note', 0009
-- added 'purchase'. Each time the coherence rule had to be re-cut, and each
-- migration left the note saying so.
ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS supplier_return_id UUID REFERENCES supplier_returns (id);

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale', 'credit_note', 'purchase', 'supplier_return'));

-- 0009's defensive pre-check, and it runs HERE — before the constraints below,
-- not after them. That ordering is not stylistic: `ADD CONSTRAINT` validates the
-- existing rows, so a pre-check placed after can never fire, and PostgreSQL
-- reports `check constraint ... is violated by some row` instead of the migration's
-- own count. That exact defect shipped in 0009 and was found by an independent
-- verifier; the test suite asserts the ordering so it cannot come back.
DO $$
DECLARE
    incoherent bigint;
BEGIN
    SELECT count(*) INTO incoherent FROM stock_movements
     WHERE (reason IN ('sale', 'credit_note') AND sale_id IS NULL)
        OR (reason IN ('purchase', 'supplier_return') AND purchase_id IS NULL)
        OR (reason = 'initial' AND (sale_id IS NOT NULL OR purchase_id IS NOT NULL OR supplier_return_id IS NOT NULL))
        OR (reason NOT IN ('sale', 'credit_note', 'initial') AND sale_id IS NULL AND purchase_id IS NULL);
    IF incoherent > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that contradict the v2.5 coherence rule; fix them before migrating',
            incoherent;
    END IF;
END;
$$;

-- The coherence rule is recreated rather than extended, because each migration
-- that adds a reason has had to widen it and the previous one hardcodes the
-- vocabulary it knew.
--
-- A supplier return names BOTH documents, exactly as a credit note does: the
-- purchase it corrects, and the return that caused it. A movement that cannot be
-- traced to the document it corrects is not auditable.
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_document_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_document_coherent
    CHECK (
        (reason IN ('sale', 'credit_note') AND sale_id IS NOT NULL)
        OR (reason = 'purchase' AND purchase_id IS NOT NULL)
        OR (reason = 'supplier_return' AND purchase_id IS NOT NULL AND supplier_return_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL AND purchase_id IS NULL AND supplier_return_id IS NULL)
    );

-- 0009's purchase-equality rule hardcodes a two-value vocabulary —
-- `(reason = 'purchase') = (purchase_id IS NOT NULL)` — so a `supplier_return`
-- row, which legitimately carries a `purchase_id` (the document it corrects),
-- violates it. The rule is re-cut here for the same reason 0009 re-cut 0007's:
-- every migration that adds a reason has to widen the constraints that
-- enumerate the vocabulary, and this test caught exactly that.
--
-- The new shape says what each reason owes its documents, which is what the
-- original meant before it became an enumeration:
--   * a purchase carries its purchase and nothing else;
--   * a return carries its purchase AND its return document.
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_purchase_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_purchase_coherent
    CHECK ((reason IN ('purchase', 'supplier_return')) = (purchase_id IS NOT NULL));

-- The equality, not a one-sided `supplier_return => id` check. A one-sided rule
-- lets a `sale` row carry a `supplier_return_id`, which 0009 measured as a real
-- hole in the mirror constraint.
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_supplier_return_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_supplier_return_coherent
    CHECK ((reason = 'supplier_return') = (supplier_return_id IS NOT NULL));

-- The direction is the whole point of the slice. Stock LEAVES on a return, and
-- the ledger accepts negative quantities in general (a sale is one), so without
-- this rule a return could add stock while its document says the goods went back.
-- 0009 added the mirror of this for purchases (`quantity > 0`).
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_supplier_return_is_outgoing;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_supplier_return_is_outgoing
    CHECK (reason <> 'supplier_return' OR quantity < 0);

CREATE INDEX IF NOT EXISTS stock_movements_supplier_return_id_idx
    ON stock_movements (supplier_return_id);

-- ── Immutability guards ─────────────────────────────────────────────────────
--
-- Written correctly in the creating migration, which is 0007->0008's lesson: 0007
-- applied its blanket reject function only to TRUNCATE, so UPDATE and DELETE
-- succeeded while the docs claimed they did not.
--
-- Unlike `sales`, there is no permitted UPDATE at all: a return has no status and
-- nothing about it may change once it exists.

CREATE OR REPLACE FUNCTION reject_supplier_return_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'supplier_returns are immutable: a correction is another compensating document, not an edit';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION reject_supplier_return_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'supplier_return_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS supplier_returns_immutable ON supplier_returns;
CREATE TRIGGER supplier_returns_immutable
BEFORE UPDATE OR DELETE ON supplier_returns
FOR EACH ROW EXECUTE FUNCTION reject_supplier_return_mutation();

DROP TRIGGER IF EXISTS supplier_returns_no_truncate ON supplier_returns;
CREATE TRIGGER supplier_returns_no_truncate
BEFORE TRUNCATE ON supplier_returns
FOR EACH STATEMENT EXECUTE FUNCTION reject_supplier_return_mutation();

DROP TRIGGER IF EXISTS supplier_return_items_immutable ON supplier_return_items;
CREATE TRIGGER supplier_return_items_immutable
BEFORE UPDATE OR DELETE ON supplier_return_items
FOR EACH ROW EXECUTE FUNCTION reject_supplier_return_item_mutation();

DROP TRIGGER IF EXISTS supplier_return_items_no_truncate ON supplier_return_items;
CREATE TRIGGER supplier_return_items_no_truncate
BEFORE TRUNCATE ON supplier_return_items
FOR EACH STATEMENT EXECUTE FUNCTION reject_supplier_return_item_mutation();

-- The guard-existence assertion, placed LAST so that a failure names the missing
-- trigger rather than whatever statement happens to fail next. 0009 shipped this
-- check in the middle and a re-run reported the constraint instead of the guard.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('supplier_returns', 'supplier_returns_immutable'),
              ('supplier_returns', 'supplier_returns_no_truncate'),
              ('supplier_return_items', 'supplier_return_items_immutable'),
              ('supplier_return_items', 'supplier_return_items_no_truncate')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 4 THEN
        RAISE EXCEPTION
            'expected 4 v2.5 immutability triggers, found %; refusing to continue', present;
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0011_ledger_integrity.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- Ledger integrity: one trigger instead of six constraints that enumerate the
-- vocabulary.
--
-- **The problem this fixes.** `0007`, `0009` and `0010` each added a ledger reason,
-- and each one had to re-write two or three CHECK constraints because those
-- constraints enumerated the reasons:
--
--   stock_movements_reason_known            IN ('initial','sale','credit_note','purchase','supplier_return')
--   stock_movements_document_coherent       IN ('sale','credit_note') AND reason = 'purchase' AND ...
--   stock_movements_purchase_coherent       IN ('purchase','supplier_return')
--   stock_movements_credit_note_coherent    = 'credit_note'
--   stock_movements_supplier_return_coherent= 'supplier_return'
--   stock_movements_purchase_is_incoming    = 'purchase'
--   stock_movements_supplier_return_is_outgoing = 'supplier_return'
--
-- The next reason would have had to touch three or four of those again, and the
-- migration that forgot one would fail in a way that names the wrong cause. That
-- is the same class of defect as `0009`'s pre-check ordering: the failure message
-- points at a constraint instead of at the rule.
--
-- **What was not checked at all.** Measured on the live schema before this
-- migration was written: the constraints validated the *documents* a movement
-- names and never the *product*. Two rows were accepted that should not exist:
--
--   * a `supplier_return` movement for a product that is not in the purchase;
--   * a `credit_note` movement for a product that is not in the sale.
--
-- And a third, worse one: a credit note belonging to sale B could be attached to a
-- movement naming sale A. The foreign key proves the note exists, never that the
-- pair agrees.
--
-- **The rule, derived from the row's own shape.** A movement names one of:
--
--   no document          -> an `initial` movement (stock entered by hand)
--   sale_id              -> stock left on a sale
--   sale_id + note       -> stock came back on a credit note
--   purchase_id          -> stock arrived on a purchase
--   purchase_id + return -> stock went back on a supplier return
--
-- Each shape implies its own direction and its own reason, so neither has to be
-- enumerated to be enforced. This is the point of the migration: `reason` stays as
-- the human-readable name of the row (and `reason_known` still declares the
-- closed vocabulary, which is unavoidable — you have to say what reasons exist),
-- but no *other* constraint has to know the list.

-- ── The shape rule ──────────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION ledger_movement_is_coherent() RETURNS trigger AS $$
DECLARE
    expected_reason text;
    expected_positive boolean;
BEGIN
    -- ── 1. Shape: which documents may coexist ──────────────────────────────
    --
    -- `initial` names nothing; every other shape names exactly one parent, and a
    -- child document is only meaningful beside its parent. A sale and a purchase
    -- on the same row would be a movement belonging to two transactions at once,
    -- which no balance can explain.
    IF NEW.sale_id IS NOT NULL AND NEW.purchase_id IS NOT NULL THEN
        RAISE EXCEPTION
            'stock movement % names both a sale and a purchase: a movement belongs to one document',
            NEW.id;
    END IF;
    IF NEW.credit_note_id IS NOT NULL AND NEW.sale_id IS NULL THEN
        RAISE EXCEPTION
            'stock movement % carries a credit note but no sale: the note corrects a sale',
            NEW.id;
    END IF;
    IF NEW.supplier_return_id IS NOT NULL AND NEW.purchase_id IS NULL THEN
        RAISE EXCEPTION
            'stock movement % carries a supplier return but no purchase: the return corrects a purchase',
            NEW.id;
    END IF;

    -- ── 2. The shape implies the reason and the direction ──────────────────
    --
    -- This CASE is the ONE place that maps a shape to a name. It is not an
    -- enumeration of the vocabulary in the sense that made the old constraints
    -- brittle: adding a reason means adding a shape here, which is the shape's
    -- own definition, in one place, instead of editing three unrelated CHECKs.
    IF NEW.sale_id IS NULL AND NEW.purchase_id IS NULL THEN
        expected_reason := 'initial';
        expected_positive := NULL;   -- an initial movement may go either way
    ELSIF NEW.sale_id IS NOT NULL AND NEW.credit_note_id IS NULL THEN
        expected_reason := 'sale';
        expected_positive := false;
    ELSIF NEW.sale_id IS NOT NULL THEN
        expected_reason := 'credit_note';
        expected_positive := true;
    ELSIF NEW.purchase_id IS NOT NULL AND NEW.supplier_return_id IS NULL THEN
        expected_reason := 'purchase';
        expected_positive := true;
    ELSE
        expected_reason := 'supplier_return';
        expected_positive := false;
    END IF;

    IF NEW.reason <> expected_reason THEN
        RAISE EXCEPTION
            'stock movement % says reason % but its shape says %',
            NEW.id, NEW.reason, expected_reason;
    END IF;

    IF expected_positive IS NOT NULL AND (NEW.quantity > 0) <> expected_positive THEN
        RAISE EXCEPTION
            'stock movement % is a % and must be %signed',
            NEW.id, expected_reason,
            CASE WHEN expected_positive THEN 'positive-' ELSE 'negative-' END;
    END IF;

    -- ── 3. Belonging: the product must be in the document it names ──────────
    --
    -- The gap the constraints never closed. A movement is how a product's stock
    -- changes; if the product is not on the document, the movement explains stock
    -- that the document never touched, and nothing later can reconstruct why.
    IF NEW.sale_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM sale_items si
         WHERE si.sale_id = NEW.sale_id AND si.product_id = NEW.product_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % names product % which is not on sale %',
            NEW.id, NEW.product_id, NEW.sale_id;
    END IF;

    IF NEW.purchase_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM purchase_items pi
         WHERE pi.purchase_id = NEW.purchase_id AND pi.product_id = NEW.product_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % names product % which is not on purchase %',
            NEW.id, NEW.product_id, NEW.purchase_id;
    END IF;

    -- ── 4. The child document must belong to the parent ────────────────────
    --
    -- A foreign key proves the note exists. It says nothing about whether the note
    -- corrects THIS sale, and a note on another sale is a correction of a document
    -- the movement does not mention. Measured as accepted before this migration.
    IF NEW.credit_note_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM credit_notes cn
         WHERE cn.id = NEW.credit_note_id AND cn.sale_id = NEW.sale_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % carries credit note % which does not correct sale %',
            NEW.id, NEW.credit_note_id, NEW.sale_id;
    END IF;

    IF NEW.supplier_return_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM supplier_returns sr
         WHERE sr.id = NEW.supplier_return_id AND sr.purchase_id = NEW.purchase_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % carries supplier return % which does not correct purchase %',
            NEW.id, NEW.supplier_return_id, NEW.purchase_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Row-level, on insert. `UPDATE` is refused outright by the append-only guard
-- (`stock_movements_append_only`, 0004), so there is no update path to validate —
-- but the trigger is declared for both, because a guard that depends on another
-- guard still being present is a guard with a hidden dependency.
DROP TRIGGER IF EXISTS stock_movements_coherent ON stock_movements;
CREATE TRIGGER stock_movements_coherent
BEFORE INSERT OR UPDATE ON stock_movements
FOR EACH ROW EXECUTE FUNCTION ledger_movement_is_coherent();

-- ── Retire the six enumeration constraints ──────────────────────────────────
--
-- Every rule they enforced is now in the trigger above, derived from the row's
-- shape. They are dropped only after the trigger exists, so there is no window in
-- which the ledger is unguarded.
--
-- `reason_known` STAYS: the closed vocabulary is a fact about the domain, not a
-- restatement of a shape, and it gives a typo a clear error instead of a shape
-- mismatch. It is the one place that lists the reasons.
--
-- `quantity_check` STAYS: it is not about documents or reasons at all.
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_document_coherent;
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_purchase_coherent;
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_credit_note_coherent;
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_supplier_return_coherent;
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_purchase_is_incoming;
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_supplier_return_is_outgoing;

-- ── Prove the existing ledger satisfies the new rule ────────────────────────
--
-- The trigger only validates rows written from now on. If a row already in the
-- ledger would be refused today, this migration must say so rather than leave a
-- rule that the data already violates — the same defensive check `0007`, `0009`
-- and `0010` each carried, and here it is the last line of defence for the
-- claims above.
DO $$
DECLARE
    offenders bigint;
BEGIN
    SELECT count(*) INTO offenders
      FROM stock_movements m
     WHERE
       -- shape: at most one parent, and no orphan child
          (m.sale_id IS NOT NULL AND m.purchase_id IS NOT NULL)
       OR (m.credit_note_id IS NOT NULL AND m.sale_id IS NULL)
       OR (m.supplier_return_id IS NOT NULL AND m.purchase_id IS NULL)
       -- reason agrees with the shape
       OR m.reason IS DISTINCT FROM (
            CASE
              WHEN m.sale_id IS NULL AND m.purchase_id IS NULL THEN 'initial'
              WHEN m.sale_id IS NOT NULL AND m.credit_note_id IS NULL THEN 'sale'
              WHEN m.sale_id IS NOT NULL THEN 'credit_note'
              WHEN m.supplier_return_id IS NULL THEN 'purchase'
              ELSE 'supplier_return'
            END)
       -- direction agrees with the shape
       OR (m.reason = 'sale' AND m.quantity >= 0)
       OR (m.reason = 'credit_note' AND m.quantity <= 0)
       OR (m.reason = 'purchase' AND m.quantity <= 0)
       OR (m.reason = 'supplier_return' AND m.quantity >= 0)
       -- the product belongs to the document
       OR (m.sale_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM sale_items si
              WHERE si.sale_id = m.sale_id AND si.product_id = m.product_id))
       OR (m.purchase_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM purchase_items pi
              WHERE pi.purchase_id = m.purchase_id AND pi.product_id = m.product_id))
       -- the child document belongs to the parent
       OR (m.credit_note_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM credit_notes cn
              WHERE cn.id = m.credit_note_id AND cn.sale_id = m.sale_id))
       OR (m.supplier_return_id IS NOT NULL AND NOT EXISTS (
             SELECT 1 FROM supplier_returns sr
              WHERE sr.id = m.supplier_return_id AND sr.purchase_id = m.purchase_id));

    IF offenders > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that the ledger-integrity rule refuses; inspect them before migrating',
            offenders;
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0012_accounts_payment_methods.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v3 (part 1 of 2): the vocabulary — accounts, payment methods, and the guards
-- that keep "where the money moved" meaningful.
--
-- **Why this is two migrations and not one.** The previous single file carried two
-- unrelated classes of risk: creating a small vocabulary plus its invariants, and
-- rewriting the destination of every document already in the database. Native review
-- found a defect in each half (a deadlocked default-account guard, then a race in the
-- same guard), and a file that big makes each reviewer re-read both halves. Split,
-- `0013` holds the data rewrite and this file holds the vocabulary, so each is
-- reviewed against one question.
--
-- **The split point is a dependency, not a line count.** The guards below are about
-- accounts and methods alone; the guard in `0013` (`guard_payment_method_account`)
-- queries the four documents, so it cannot live here — those tables have no
-- `payment_method_id` until `0013` adds it.
--
-- **What this file does NOT do.** No balance column: a balance is a `SUM` over
-- documents, derived by the view in `0014`. No `kind` on `accounts`: that
-- classification was argued for and withdrawn, because its three values are
-- indistinguishable in v3 and locating the till is already solved by a fixed id —
-- the mechanism `WALK_IN_CUSTOMER_ID` (`sales.rs`) and `ONE_OFF_SUPPLIER_ID`
-- (`purchases.rs`) established.
--
-- The direction is the requirement: a payment method belongs to exactly one
-- account; an account has zero or more methods; an event names the *method* and the
-- method resolves the *account*. A method shared across accounts would make every
-- per-account balance unreconstructable.

-- ── Accounts ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS accounts (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    -- Whether this is the account the shop means by "the till". Exactly one row
    -- may hold it, enforced below. It carries two real consumers — the `/sales`
    -- footer and the default selection in the sale's payment-method picker — so
    -- it is configuration with use, not a preventive classification.
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A blank name would render as an empty row in the picker, which is worse
    -- than a missing account. Trimmed before the check, so '   ' is refused too.
    CONSTRAINT accounts_name_not_blank CHECK (btrim(name) <> '')
);

-- At most one default account. A partial unique index rather than a CHECK,
-- because "exactly one row has this value" is a statement about the table, and a
-- row-level CHECK cannot see sibling rows.
CREATE UNIQUE INDEX IF NOT EXISTS accounts_one_default_idx
    ON accounts (is_default) WHERE is_default;

-- ── Payment methods ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payment_methods (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts (id),
    -- The stored name is English and is a contract: it reaches the API and any
    -- export. "Efectivo"/"Crédito" live in the frontend dictionary, which is how
    -- `walk-in` survived es-AR unchanged.
    name       TEXT NOT NULL UNIQUE,
    -- Retires a method from the picker while its history stays readable. It is a
    -- **picker** concern only: the write paths accept an inactive method, which is
    -- what keeps the `walk-in`+credit guard from passing for the wrong reason (a
    -- refusal caused by inactivity rather than by the rule under test).
    is_active  BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT payment_methods_name_not_blank CHECK (btrim(name) <> '')
);

CREATE INDEX IF NOT EXISTS payment_methods_account_id_idx
    ON payment_methods (account_id);

-- ── Seeds ───────────────────────────────────────────────────────────────────
--
-- Fixed ids, exactly as `walk-in` (`…0001`) and `one-off-supplier` (`…0002`) are.
-- A fixed id is what lets the server find a known row without a lookup and
-- without a classification column.
--
-- **`credit` is not seeded, and it never was, in this history.** v3 seeded an
-- account (`…0005`) and an inactive method (`…0006`) named `credit` so that a
-- guard about "a credit sale may not be walk-in" had a row that could make it
-- fail. That guard is gone and so is the row: debt is a document (`obligations`,
-- `payables`) and there is no account that holds a right to be paid. `S7` of
-- `v4-settlement.md` retired it, and the owner's decision was to rewrite the
-- history rather than add a migration that deletes a row the initial schema
-- should never have created — the application has never shipped, so no database
-- holds a document naming it.
--
-- **This leaves only `cash` as a seeded destination**, which is what the write
-- paths already assume: `CASH_ACCOUNT_ID` and `CASH_PAYMENT_METHOD_ID` in
-- `models/account.rs` are the only fixed ids the server resolves without a
-- lookup.
INSERT INTO accounts (id, name, is_default)
VALUES ('00000000-0000-0000-0000-000000000003', 'cash', true)
ON CONFLICT DO NOTHING;

INSERT INTO payment_methods (id, account_id, name, is_active)
VALUES ('00000000-0000-0000-0000-000000000004',
        '00000000-0000-0000-0000-000000000003', 'cash', true)
ON CONFLICT DO NOTHING;

-- ── The default account always has an active method ─────────────────────────

-- The owner's invariant. It is a statement about **other rows**, so no CHECK can
-- express it, and it has three ways to be violated:
--   * the last active method of the default account is deactivated;
--   * the last active method of the default account is moved to another account;
--   * an account is made default while having no active method.
-- A fourth path is closed by refusing DELETE on both tables (below): if a method
-- could be deleted, this invariant would be unreachable-except-by-delete.
--
-- The triggers are created **after** the seeds, because the seed inserts the
-- default account before any method exists. A trigger created earlier would fail
-- on the very rows it is meant to protect the boot of.
CREATE OR REPLACE FUNCTION guard_default_account_keeps_active_method() RETURNS trigger AS $$
DECLARE
    is_default_account boolean;
BEGIN
    -- Fires when a method stops being active, or leaves the account. Both would
    -- remove the last active method from a default account.
    IF (OLD.is_active AND NOT NEW.is_active)
       OR NEW.account_id IS DISTINCT FROM OLD.account_id THEN
        -- **The lock is the guard, not a hint.** Reproduced with two real sessions
        -- before this line existed: A deactivates m1 and B deactivates m2
        -- concurrently, each sees the other method as still active, both commit, and
        -- the default account is left with **zero** active methods. An EXISTS check
        -- alone cannot see a concurrent uncommitted deactivation, so nothing
        -- serialized the two writers. `FOR UPDATE` on the account row does: the
        -- second transaction blocks until the first commits, then sees the real
        -- state and refuses. Found by native review (`R3-001`, second occurrence) as
        -- an inferential finding and confirmed by reproduction.
        SELECT a.is_default INTO is_default_account
          FROM accounts a WHERE a.id = OLD.account_id FOR UPDATE;

        IF is_default_account THEN
            IF NOT EXISTS (
                SELECT 1 FROM payment_methods pm
                 WHERE pm.account_id = OLD.account_id
                   AND pm.is_active
                   AND pm.id <> OLD.id
            ) THEN
                RAISE EXCEPTION
                    'account % is the default and would be left with no active payment method',
                    OLD.account_id;
            END IF;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_default_account_guard ON payment_methods;
CREATE TRIGGER payment_methods_default_account_guard
BEFORE UPDATE OF is_active, account_id ON payment_methods
FOR EACH ROW EXECUTE FUNCTION guard_default_account_keeps_active_method();

CREATE OR REPLACE FUNCTION guard_new_default_account_has_active_method() RETURNS trigger AS $$
BEGIN
    -- Covers both "make an existing account default" and "create it already
    -- default". `is_default = false` is always allowed: it can only *reduce* the
    -- set of defaults, and the partial unique index already allows zero.
    IF NEW.is_default AND (TG_OP = 'INSERT' OR NOT OLD.is_default) THEN
        -- The same race as the deactivation path, in the other direction: promoting
        -- an account while its last active method is concurrently deactivated would
        -- otherwise leave a default with nothing active. Locking the candidate row
        -- makes the two writers serialize on the same object.
        IF TG_OP = 'UPDATE' THEN
            PERFORM 1 FROM accounts WHERE id = NEW.id FOR UPDATE;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM payment_methods pm
             WHERE pm.account_id = NEW.id AND pm.is_active
        ) THEN
            RAISE EXCEPTION
                'account % cannot be the default: it has no active payment method',
                NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_default_needs_method_guard ON accounts;
CREATE TRIGGER accounts_default_needs_method_guard
BEFORE INSERT OR UPDATE OF is_default ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_new_default_account_has_active_method();

-- **Moving the default, and the deadlock this replaced.** The first version of
-- this migration refused every true→false transition here, and `_one_default_idx`
-- refuses a second true row while the current one is still true. Measured, the two
-- together made the default **permanently fixed** to whichever account was seeded:
--
--   apagar cash primero            → 'cannot stop being the default'
--   prender credit primero        → duplicate key on accounts_one_default_idx
--
-- No ordering worked. The invariant was also mis-stated: "the shop always has
-- exactly one till with an active method" is a statement about the table at the
-- end of a transaction, and a BEFORE trigger cannot see that state. It is therefore
-- a **deferrable constraint**, not a trigger: within one transaction the shop may
-- briefly have zero or two defaults, and the check runs at COMMIT.
--
-- The fix was found by native review (`R3-001`, critical) and verified by hand:
-- the tests in this slice had encoded the bug, asserting that unsetting the default
-- was an error instead of that a *transition* was legal as long as one always
-- remained.
--
-- What remains as a trigger is only the part that a constraint cannot express:
-- the account being made default must already have an active method, because that
-- is a fact about the candidate, not about the final count.
CREATE OR REPLACE FUNCTION guard_new_default_account_has_active_method() RETURNS trigger AS $$
BEGIN
    IF NEW.is_default AND (TG_OP = 'INSERT' OR NOT OLD.is_default) THEN
        IF NOT EXISTS (
            SELECT 1 FROM payment_methods pm
             WHERE pm.account_id = NEW.id AND pm.is_active
        ) THEN
            RAISE EXCEPTION
                'account % cannot be the default: it has no active payment method',
                NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_default_needs_method_guard ON accounts;
CREATE TRIGGER accounts_default_needs_method_guard
BEFORE INSERT OR UPDATE OF is_default ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_new_default_account_has_active_method();

-- The count invariant, deferred to COMMIT. `accounts_one_default_idx` still
-- guarantees *at most* one at any instant; this guarantees *exactly* one at the end
-- of every transaction, which is the rule the shop actually needs: the `/sales`
-- footer and the sale picker always have an account to fall back to.
--
-- **A CONSTRAINT TRIGGER, not a deferred CHECK.** Measured while writing this:
-- PostgreSQL refuses `CHECK constraints cannot be marked DEFERRABLE`, on every
-- version — only UNIQUE, PRIMARY KEY, EXCLUDE and FOREIGN KEY may be deferred, and a
-- partial unique index (which is what `accounts_one_default_idx` is) cannot be a
-- deferrable constraint at all. The mechanism designed exactly for "validate the
-- final state of a table at COMMIT" is a constraint trigger, and that is what this
-- is: AFTER, FOR EACH ROW, DEFERRABLE INITIALLY DEFERRED.
CREATE OR REPLACE FUNCTION assert_exactly_one_default_account() RETURNS trigger AS $$
DECLARE
    defaults bigint;
BEGIN
    SELECT count(*) INTO defaults FROM accounts WHERE is_default;
    IF defaults <> 1 THEN
        RAISE EXCEPTION
            'the shop must have exactly one default account at commit; found %', defaults;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_exactly_one_default ON accounts;
CREATE CONSTRAINT TRIGGER accounts_exactly_one_default
AFTER INSERT OR UPDATE OR DELETE ON accounts
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION assert_exactly_one_default_account();

-- ── No deletes ──────────────────────────────────────────────────────────────
--
-- Deletion would erase the explanation of a balance: a method's documents would
-- keep a foreign key to a row that no longer says which account the money went to.
-- `is_active` is the answer to "I do not want it in the picker any more", and that
-- is the only retirement both tables need.
--
-- Both `UPDATE` and `DELETE` are guarded where mutation is refused at all, and
-- `TRUNCATE` is a separate statement-level trigger: it does not fire FOR EACH ROW,
-- so a row-level guard never sees it. `0007` shipped exactly that mistake — a
-- blanket reject function applied only to TRUNCATE, so UPDATE and DELETE succeeded
-- while the docs claimed they did not. `0008` repaired it; that lesson is applied
-- here up front rather than repaired later.
CREATE OR REPLACE FUNCTION reject_account_delete() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'accounts cannot be deleted: is_active on its methods is how a destination is retired';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION reject_payment_method_delete() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'payment_methods cannot be deleted: is_active is how a method is retired';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_no_delete ON accounts;
CREATE TRIGGER accounts_no_delete
BEFORE DELETE ON accounts
FOR EACH ROW EXECUTE FUNCTION reject_account_delete();

DROP TRIGGER IF EXISTS accounts_no_truncate ON accounts;
CREATE TRIGGER accounts_no_truncate
BEFORE TRUNCATE ON accounts
FOR EACH STATEMENT EXECUTE FUNCTION reject_account_delete();

DROP TRIGGER IF EXISTS payment_methods_no_delete ON payment_methods;
CREATE TRIGGER payment_methods_no_delete
BEFORE DELETE ON payment_methods
FOR EACH ROW EXECUTE FUNCTION reject_payment_method_delete();

DROP TRIGGER IF EXISTS payment_methods_no_truncate ON payment_methods;
CREATE TRIGGER payment_methods_no_truncate
BEFORE TRUNCATE ON payment_methods
FOR EACH STATEMENT EXECUTE FUNCTION reject_payment_method_delete();

-- ── Prove the guards exist ──────────────────────────────────────────────────
--
-- LAST, after every CREATE TRIGGER, because that is the only position where a
-- failure names the real cause. `0009` shipped this check in the middle and a re-run
-- reported the constraint that happened to fail next instead of the guard that was
-- actually absent; a diagnostic that names the wrong cause costs more than no
-- diagnostic.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payment_methods', 'payment_methods_default_account_guard'),
              ('payment_methods', 'payment_methods_no_delete'),
              ('payment_methods', 'payment_methods_no_truncate'),
              ('accounts', 'accounts_default_needs_method_guard'),
              ('accounts', 'accounts_no_delete'),
              ('accounts', 'accounts_no_truncate'),
              ('accounts', 'accounts_exactly_one_default')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 7 THEN
        RAISE EXCEPTION
            'expected 7 v3 account/method triggers, found %; refusing to continue', present;
    END IF;

    -- The count invariant is a DEFERRABLE constraint trigger, and its deferrability
    -- is the whole point: without it the shop could end a transaction with no till
    -- and nothing else here would notice.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'accounts_exactly_one_default'
           AND condeferrable
           AND condeferred
    ) THEN
        RAISE EXCEPTION
            'accounts_exactly_one_default is missing or not DEFERRABLE INITIALLY DEFERRED; refusing to continue';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0013_documents_payment_destination.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v3 (part 2 of 2): the four documents learn where the money went.
--
-- **This half is the data rewrite**, and it is separate because it carries a
-- different risk from `0012`: it adds a NOT NULL column to four populated tables,
-- fills it for every row already in the database, and drops the column it replaces.
-- Reviewing "did the vocabulary and its guards survive?" and "did the backfill
-- rewrite history correctly?" are two questions, so they are two files.
--
-- **The backfill cannot invent a destination.** `0002`'s `DEFAULT 'cash'` is the
-- proof that every row already in these tables means "the till", so pointing them at
-- the seeded `cash` method states a fact rather than making one up.
--
-- **Why `ADD COLUMN ... NOT NULL DEFAULT` and not a four-step backfill.** Measured:
-- `0007`, `0009` and `0010` guard `credit_notes`, `purchases` and `supplier_returns`
-- with triggers that reject `UPDATE` outright
-- (`credit_notes are immutable: corrections are compensating rows, not edits`), so
-- `ADD COLUMN nullable` -> `UPDATE` -> `SET NOT NULL` fails on three of four tables.
-- Disabling those triggers for the length of a migration is the wrong trade: they are
-- the guards most worth trusting, and switching them off while history is written is
-- exactly what this repository refuses. `ADD COLUMN ... NOT NULL DEFAULT` fills
-- existing rows as a catalog change — no row is updated, no trigger fires — and the
-- `DEFAULT` is dropped immediately afterwards so it cannot become a second way of
-- writing a document.

-- ── The four documents learn where the money went ───────────────────────────

-- Added nullable, backfilled, then set NOT NULL. The order is not stylistic: a
-- `NOT NULL` on a populated table fails immediately, and the backfill cannot
-- invent a destination — it records the only one that existed. `0002`'s
-- `DEFAULT 'cash'` is the proof that every row already in these tables means "the
-- till", so pointing them at the seeded `cash` method states a fact.
--
-- The alternative — leaving the column nullable so old history has no
-- destination — was refused: it would create two classes of document and every
-- consumer would need two code paths forever.
--
-- **Why `ADD COLUMN ... NOT NULL DEFAULT` in one statement instead of an
-- `UPDATE` backfill.** Measured while writing this file: the `UPDATE` form fails
-- on three of the four tables, because `0007`, `0009` and `0010` guard them with
-- triggers that reject `UPDATE` outright (`credit_notes are immutable: corrections
-- are compensating rows, not edits`). Disabling those triggers for the length of a
-- migration would mean the guards most worth trusting are exactly the ones switched
-- off while history is rewritten. `ADD COLUMN ... NOT NULL DEFAULT` fills existing
-- rows as a catalog change — no row is updated, no trigger fires — which is both
-- safer and the reason the guards stay untouched.
--
-- The `DEFAULT` is dropped immediately afterwards so it cannot become a second way
-- of writing a document: after this point a new row with no destination is refused,
-- and the write paths name the method explicitly.
ALTER TABLE sales
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE credit_notes
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE purchases
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE supplier_returns
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);

ALTER TABLE sales            ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE credit_notes     ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE purchases        ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE supplier_returns ALTER COLUMN payment_method_id DROP DEFAULT;

-- The view joins documents to methods on these columns, and the balance of one
-- account reads through that join. Without them the per-branch `account_id`
-- predicate cannot be pushed down.
CREATE INDEX IF NOT EXISTS sales_payment_method_id_idx            ON sales (payment_method_id);
CREATE INDEX IF NOT EXISTS credit_notes_payment_method_id_idx     ON credit_notes (payment_method_id);
CREATE INDEX IF NOT EXISTS purchases_payment_method_id_idx        ON purchases (payment_method_id);
CREATE INDEX IF NOT EXISTS supplier_returns_payment_method_id_idx ON supplier_returns (payment_method_id);

-- ── Retire `sales.payment_method`, and re-cut the guard that froze it ───────

-- The old TEXT column held one value and nothing wrote it. Keeping it beside the
-- FK would be exactly the dead vocabulary this slice rejects elsewhere, and the
-- `C: hybrid` wire shape was rejected for the same reason: two sources of the
-- same name, one of which goes stale.
--
-- **The guard must be re-cut in this same file, and this is not optional.**
-- `guard_sale_economics` (`0003`) compares `NEW.payment_method <> OLD.payment_method`.
-- A plpgsql body is not resolved when it is created, so dropping the column leaves
-- the function in place and *valid-looking* until the next UPDATE, when it fails at
-- runtime with `column "payment_method" does not exist`. Worse, if it were simply
-- dropped from the comparison, a recorded sale's destination could be rewritten
-- after the fact — paying 25.00 at the till and later moving the money to the bank
-- statement, which is the history rewrite this repository refuses everywhere.
--
-- `IS DISTINCT FROM` rather than `<>`, deliberately: `NULL <> value` is `NULL`, and
-- an `IF` whose condition is `NULL` does not fire. Today both columns are NOT NULL,
-- so the two spellings behave identically — but the guard's whole job is to keep
-- holding when a future migration relaxes that, and a guard that silently stops
-- guarding is the defect `0011` was written to kill.
ALTER TABLE sales DROP COLUMN IF EXISTS payment_method;

CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total IS DISTINCT FROM OLD.total
       OR NEW.customer_id IS DISTINCT FROM OLD.customer_id
       OR NEW.payment_method_id IS DISTINCT FROM OLD.payment_method_id THEN
        RAISE EXCEPTION 'sale % economics are immutable: status may change, money may not', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── A method's account is immutable once history names it ───────────────────

-- Moving a method between accounts would rewrite yesterday's balance today: the
-- document still says "this method", the method now says "another account", and the
-- account's historical total changes without a single document being written.
--
-- The condition is **history**, not the field: a method no document names is a typo
-- and must stay correctable, or a method attached to the wrong account could never
-- be fixed. That is also why `name` and `is_active` stay editable — the identity
-- that must not move is which account the money lands in, not the spelling.
CREATE OR REPLACE FUNCTION guard_payment_method_account() RETURNS trigger AS $$
BEGIN
    IF NEW.account_id IS DISTINCT FROM OLD.account_id THEN
        IF EXISTS (SELECT 1 FROM sales            WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM credit_notes     WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM purchases        WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM supplier_returns WHERE payment_method_id = OLD.id) THEN
            RAISE EXCEPTION
                'payment method % cannot move to another account: documents already name it',
                OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_account_guard ON payment_methods;
CREATE TRIGGER payment_methods_account_guard
BEFORE UPDATE OF account_id ON payment_methods
FOR EACH ROW EXECUTE FUNCTION guard_payment_method_account();

-- ── Every existing document satisfies the new rule ──────────────────────────

-- The NOT NULL above proves the column is populated; the foreign key proves it
-- points at a method; this names the rule the slice actually claims — every document
-- says where the money moved — and its failure names the data rather than a trigger.
DO $$
DECLARE
    stranded bigint;
BEGIN
    SELECT count(*) INTO stranded
      FROM (
            SELECT payment_method_id FROM sales
            UNION ALL SELECT payment_method_id FROM credit_notes
            UNION ALL SELECT payment_method_id FROM purchases
            UNION ALL SELECT payment_method_id FROM supplier_returns
           ) AS documents
      LEFT JOIN payment_methods pm ON pm.id = documents.payment_method_id
     WHERE pm.id IS NULL;

    IF stranded > 0 THEN
        RAISE EXCEPTION
            'v3: % document(s) name a payment method that does not exist; refusing to continue',
            stranded;
    END IF;
END;
$$;

-- ── Prove this half's guard exists ──────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'payment_methods_account_guard'
           AND tgrelid = 'payment_methods'::regclass
           AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION
            'payment_methods_account_guard is missing; refusing to continue';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0014_money_movements_view.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v3: `money_movements`, the single definition of "money moved".
--
-- **Why a view and not a table.** The stock ledger's movement IS the primary fact:
-- `sale_items.quantity` says what was sold, but nothing else says how much stock
-- moved. With money the document total is already the fact, so a mirror table would
-- add no information — only a second assertion about the same fact, needing a
-- coherence trigger per document kind forever, plus a backfill over immutable rows.
-- A view stores nothing, cannot drift from the documents, and needs no backfill.
-- Balances stay sums, never stored fields.
--
-- **Why it exists at all, rather than a `SUM` per call site.** Four consumers are
-- already planned (the general balance, one account's balance, the daily footer, and
-- a v6 statement), and with credit they become seven or eight. Each hand-written
-- `UNION` re-derives the sign and the destination, and the one that forgets a branch
-- returns a plausible wrong number instead of an error — which is exactly the class
-- of defect `0011` was written to kill for stock. One named definition, one place to
-- extend when a document is added.
--
-- **One row per EFFECT, not per document.** Today every document has one effect, but
-- a collection (v4) and a transfer (v4) each have two: money leaving one account and
-- arriving at another. Writing the view on "one row per document" would force a shape
-- change the moment credit exists, which is the thing choosing a view was supposed to
-- avoid.
--
-- **Sign convention, declared once: positive is money entering the account.**
--   sale              +total   (money came in)
--   credit_note       -total   (money went back to the customer)
--   purchase          -total   (money went out to the supplier)
--   supplier_return   +total   (the supplier gave it back)
--
-- Rediscovering that convention in four call sites is how two of them end up with the
-- opposite sign.
--
-- **What it does NOT do.** It validates nothing, it is not indexed, and it must never
-- be written to: it is a read model. The coherence rules live in the documents' own
-- triggers (`0011` for stock, `0012`/`0013` for destinations).

CREATE OR REPLACE VIEW money_movements AS
-- A sale puts money into the account its method resolves to. `status = 'confirmed'`
-- is the same predicate `/api/cash` used and `list_sales` applies: a sale carries a
-- reserved lifecycle whose other states must not count. `T2` drops that column, and
-- when it does this branch loses the filter because "the row exists" and "it counts"
-- become the same statement — which is already true of the other three.
SELECT s.payment_method_id          AS payment_method_id,
       pm.account_id                AS account_id,
       'sale'::text                 AS document_kind,
       s.id                         AS document_id,
       s.total                      AS amount,
       s.created_at                 AS occurred_at
  FROM sales s
  JOIN payment_methods pm ON pm.id = s.payment_method_id
 WHERE s.status = 'confirmed'

UNION ALL
-- A refund returns money to the customer, through the method the sale was paid with.
SELECT cn.payment_method_id, pm.account_id, 'credit_note'::text, cn.id, -cn.total, cn.created_at
  FROM credit_notes cn
  JOIN payment_methods pm ON pm.id = cn.payment_method_id

UNION ALL
-- A purchase pays the supplier.
SELECT p.payment_method_id, pm.account_id, 'purchase'::text, p.id, -p.total, p.created_at
  FROM purchases p
  JOIN payment_methods pm ON pm.id = p.payment_method_id

UNION ALL
-- A supplier return brings money back.
SELECT sr.payment_method_id, pm.account_id, 'supplier_return'::text, sr.id, sr.total, sr.created_at
  FROM supplier_returns sr
  JOIN payment_methods pm ON pm.id = sr.payment_method_id;

COMMENT ON VIEW money_movements IS
'Read model: one row per effect of money on an account. Positive is money entering. Never written to; balances are SUM over this view.';

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0015_drop_sale_lifecycle.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v3 (last): a sale has no lifecycle, because nothing ever had one.
--
-- **What was there.** `0002` reserved `status` (`draft | confirmed | cancel | done`)
-- and `reversed_at` for a lifecycle v1 never used. Measured over the whole life of
-- the system: **no write path in `backend/src` has ever set either column.** Their
-- only reads were `WHERE status = 'confirmed'` in two queries, and a dead
-- `status === 'cancel' || reversed_at !== null` branch in the ledger that was
-- unreachable by construction — nothing could set `'cancel'`.
--
-- **Why they were wrong, and it is not "unused".** `0007` argued it when it built
-- the credit note: *a status answers what state a document is in; the question here
-- is how much has been returned, which is a quantity that accumulates and must be
-- auditable line by line.* The same argument kills `done`: a customer owing 10.000
-- who pays 3.000 makes `done` unrepresentable without `partially_done`, and the enum
-- then grows with every document in the business. That is precisely the defect
-- `0011` was written to kill for stock — constraints enumerating a vocabulary that
-- every new document re-cuts.
--
-- **What replaces it: nothing.** "The row exists" and "it counts" become one
-- statement, exactly as they already were for `purchases`, `credit_notes` and
-- `supplier_returns`. An annulment is derived from the credit notes and from nothing
-- else, which is what the client already did.
--
-- **The risk this migration actually carries is the DROP, not the view.** A
-- `DROP COLUMN` on populated tables is the one irreversible act in the whole slice,
-- so the two pre-checks below run *before* anything is altered, and they refuse
-- rather than guess. Both were measured empty on the development database and on the
-- test database before this file was written.

-- ── Pre-checks: refuse to destroy a fact ────────────────────────────────────
--
-- If any sale says anything other than `confirmed`, then something in this database
-- *did* use the lifecycle and dropping it would erase that fact silently. If any sale
-- carries `reversed_at`, the system recorded a reversal through the column rather than
-- through a credit note, and removing it would lose the only trace. Neither can happen
-- on a schema whose write paths never touch them — which is exactly why the check is
-- worth having: it proves that claim about *this* data instead of assuming it.
DO $$
DECLARE
    not_confirmed bigint;
    reversed bigint;
BEGIN
    SELECT count(*) INTO not_confirmed FROM sales WHERE status <> 'confirmed';
    SELECT count(*) INTO reversed      FROM sales WHERE reversed_at IS NOT NULL;

    IF not_confirmed > 0 OR reversed > 0 THEN
        RAISE EXCEPTION
            'sales still use the lifecycle (% rows with status <> confirmed, % with reversed_at set); '
            'a credit note is the record of a reversal, so these must be migrated rather than dropped',
            not_confirmed, reversed;
    END IF;
END;
$$;

-- ── The full-totals snapshot, taken before the drop ─────────────────────────
--
-- The columns this file removes are read by `money_movements`, so that view has to be
-- recreated in the same transaction. To prove the recreation changed no answer, the
-- per-account totals are captured here and compared after. A migration that rewrites a
-- read model owes that comparison; without it, "the view still works" would be a claim
-- about compilation, not about arithmetic.
-- `DROP` first: a `TEMP TABLE` lives for the session, and a migrator that re-runs this
-- file within one connection — a retry after a failed attempt, which is exactly how
-- `0009`'s re-run defect was found — would otherwise fail on the name rather than on
-- anything true. `R3-001` flagged the unguarded create; this is the guard.
DROP TABLE IF EXISTS v3_totals_before_drop;
CREATE TEMP TABLE v3_totals_before_drop AS
SELECT account_id, SUM(amount) AS total
  FROM money_movements
 GROUP BY account_id;

-- ── Retire the lifecycle ────────────────────────────────────────────────────

-- The guard is re-cut BEFORE the columns go, for the ordering reason `0013` recorded:
-- a plpgsql body is not resolved when it is created, so a function still naming a
-- dropped column stays valid-looking until the next UPDATE fails at runtime with
-- `column ... does not exist`. Keeping the guard's remaining comparisons is what
-- matters — `total`, `customer_id` and `payment_method_id` are still immutable.
CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total IS DISTINCT FROM OLD.total
       OR NEW.customer_id IS DISTINCT FROM OLD.customer_id
       OR NEW.payment_method_id IS DISTINCT FROM OLD.payment_method_id THEN
        RAISE EXCEPTION 'sale % economics are immutable: a correction is a credit note, never an edit', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- **The view goes first, and the first attempt got this wrong.** `money_movements`
-- reads `s.status`, so PostgreSQL refuses the drop with
-- `cannot drop column status of table sales because other objects depend on it`.
-- Measured, not guessed: the dependency was found by creating the view on the test
-- database and asking `pg_depend` which objects referenced the column. Recreating the
-- view without the filter in the same transaction, before the `DROP`, is the only
-- order that works.
-- ── The view without a filter ───────────────────────────────────────────────
--
-- Every remaining branch reads a document whose existence IS its validity, so the
-- sale branch loses its predicate and the four branches finally agree on what counts.
-- **The aliases are not decoration.** `CREATE OR REPLACE VIEW` refuses a column
-- rename, and the first attempt omitted them: PostgreSQL took the first branch's
-- third column to be named `text` and failed with
-- `cannot change name of view column "document_kind" to "text"`. Measured, then fixed.
CREATE OR REPLACE VIEW money_movements AS
SELECT s.payment_method_id          AS payment_method_id,
       pm.account_id                AS account_id,
       'sale'::text                 AS document_kind,
       s.id                         AS document_id,
       s.total                      AS amount,
       s.created_at                 AS occurred_at
  FROM sales s
  JOIN payment_methods pm ON pm.id = s.payment_method_id

UNION ALL
SELECT cn.payment_method_id, pm.account_id, 'credit_note'::text, cn.id, -cn.total, cn.created_at
  FROM credit_notes cn
  JOIN payment_methods pm ON pm.id = cn.payment_method_id

UNION ALL
SELECT p.payment_method_id, pm.account_id, 'purchase'::text, p.id, -p.total, p.created_at
  FROM purchases p
  JOIN payment_methods pm ON pm.id = p.payment_method_id

UNION ALL
SELECT sr.payment_method_id, pm.account_id, 'supplier_return'::text, sr.id, sr.total, sr.created_at
  FROM supplier_returns sr
  JOIN payment_methods pm ON pm.id = sr.payment_method_id;

COMMENT ON VIEW money_movements IS
'Read model: one row per effect of money on an account. Positive is money entering. Never written to; balances are SUM over this view.';

ALTER TABLE sales DROP COLUMN IF EXISTS status;
ALTER TABLE sales DROP COLUMN IF EXISTS reversed_at;

-- ── The drop changed no figure ──────────────────────────────────────────────
--
-- The assertion this migration exists to make. Every account that existed before the
-- drop must hold exactly the same total after it, and no account may have appeared.
DO $$
DECLARE
    differing bigint;
BEGIN
    SELECT count(*) INTO differing
      FROM v3_totals_before_drop before
      FULL JOIN (
            SELECT account_id, SUM(amount) AS total
              FROM money_movements
             GROUP BY account_id
           ) after ON after.account_id = before.account_id
     WHERE before.total IS DISTINCT FROM after.total;

    IF differing > 0 THEN
        RAISE EXCEPTION
            'dropping the sale lifecycle changed % account total(s); the view no longer reproduces the ledger',
            differing;
    END IF;
END;
$$;

-- ── Prove the guard survived ────────────────────────────────────────────────
--
-- Last, so a failure names the guard rather than whatever statement happens to fail
-- next — the ordering `0009` shipped wrong and `0010` repaired.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'sales_economics_guard'
           AND tgrelid = 'sales'::regclass
           AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION 'sales_economics_guard is missing after the lifecycle drop';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0017_money_entries.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S1: the money book is a table.
--
-- `odd/tasks/v4-settlement.md` S1-S3. Until now the only answer to "where did the money
-- move" was `money_movements` — a `VIEW` over four document tables, joined to
-- `payment_methods` with an `INNER JOIN`. Two consequences, both measured before this
-- file existed:
--
--   * a misaligned row (a method that no longer resolves, a null future column) simply
--     vanishes from every balance, with no error, and nothing records that it should
--     have been there;
--   * a lost `UNION` branch leaves no trace, because there is no row to be missing.
--
-- `stock_movements` is the counter-example in this same schema: a real table, append-only
-- since `0004`, with a `reason` and a coherence guard. The money side has the weaker
-- design of the two, and this migration gives it the stronger one.
--
-- **The view does not disappear.** `money_movements` is re-created as a view over
-- `money_entries`, so every balance, report and test that reads it keeps working while
-- the source becomes a row.

-- ── accounts.kind: money or digital, never obligation ──────────────────────
--
-- `S3`. The values are closed on purpose: an obligation is **not** an account
-- (`v4-settlement.md`, S3 and S5), so `obligation` is not a member. v3 considered this
-- column and withdrew it because "in v3 its values would be indistinguishable" — cash
-- and the debt account were the only two rows, and one of them was not a wallet.
--
-- Default `digital` keeps the column additive: the seeded `cash` account is set below,
-- and any account a user creates is assumed to be a bank or a card until said otherwise.
ALTER TABLE accounts
    ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'digital';

-- **The constraint goes in before the UPDATE, and the order is not cosmetic.**
-- `0016` installed `accounts_exactly_one_default` as `AFTER ... DEFERRABLE INITIALLY
-- DEFERRED`, so the first statement that touches a row queues an event that is checked at
-- COMMIT. A second `ALTER TABLE accounts` in the same transaction then fails with
-- `cannot ALTER TABLE "accounts" because it has pending trigger events`. Measured:
-- adding the constraint after the UPDATE aborted the whole migration. Adding it first
-- means the only row-touching statement is the last one.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'accounts_kind_known'
    ) THEN
        ALTER TABLE accounts
            ADD CONSTRAINT accounts_kind_known CHECK (kind IN ('cash', 'digital'));
    END IF;
END;
$$;

-- The seeded till: fixed id `…0003`, the account `CASH_ACCOUNT_ID` names.
UPDATE accounts SET kind = 'cash' WHERE id = '00000000-0000-0000-0000-000000000003';

-- ── The book ────────────────────────────────────────────────────────────────
--
-- `document_kind` is text rather than an enum so a new document type does not need a
-- type migration; the closed set is enforced by a CHECK, the same way
-- `stock_movements.reason` is. `document_id` is NOT NULL because **an entry is the
-- effect of a document, not a document** (`S2`): an entry with no cause is a bug, and a
-- bug should not be representable.
--
-- No foreign key on `document_id`: it points at four different tables, and a polymorphic
-- FK does not exist in PostgreSQL. The coherence guard below closes the gap instead, with
-- the same technique `0011` used for `stock_movements`.
CREATE TABLE IF NOT EXISTS money_entries (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id        UUID NOT NULL REFERENCES accounts (id),
    payment_method_id UUID NOT NULL REFERENCES payment_methods (id),
    document_kind     TEXT NOT NULL,
    document_id       UUID NOT NULL,
    -- Positive is money entering the account. Declared once, here, so every reader and
    -- every writer shares one convention instead of re-deriving a sign.
    -- **Zero is allowed, and it is not a degenerate case.** `0005` made bonus lines a
    -- legitimate sale with `total = 0`, and `sales_total_check` is `total >= 0`. A
    -- sale of nothing but bonuses moved no money; the book records that fact rather than
    -- refusing the document. Measured: `CHECK (amount <> 0)` here turned a valid bonus
    -- sale into a 500.
    amount            NUMERIC(14, 6) NOT NULL,
    occurred_at       TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        TEXT NOT NULL,

    CONSTRAINT money_entries_amount_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT money_entries_kind_known CHECK (
        document_kind IN ('sale', 'credit_note', 'purchase', 'supplier_return')
    ),
    CONSTRAINT money_entries_created_by_not_blank CHECK (btrim(created_by) <> '')
);

CREATE INDEX IF NOT EXISTS money_entries_account_id_idx ON money_entries (account_id);
CREATE INDEX IF NOT EXISTS money_entries_occurred_at_idx ON money_entries (occurred_at DESC);
CREATE INDEX IF NOT EXISTS money_entries_document_idx
    ON money_entries (document_kind, document_id);
CREATE INDEX IF NOT EXISTS money_entries_payment_method_id_idx
    ON money_entries (payment_method_id);

-- ── Append-only, all three ways ─────────────────────────────────────────────
--
-- Exactly the lesson `0004` records: a row-level guard never sees `TRUNCATE`, because it
-- is a set operation and does not fire FOR EACH ROW. `0007` shipped that mistake and
-- `0008` repaired it. Both guards go in from the start here.
CREATE OR REPLACE FUNCTION reject_money_entry_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        'money_entries are append-only: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS money_entries_append_only ON money_entries;
CREATE TRIGGER money_entries_append_only
BEFORE UPDATE OR DELETE ON money_entries
FOR EACH ROW EXECUTE FUNCTION reject_money_entry_mutation();

DROP TRIGGER IF EXISTS money_entries_no_truncate ON money_entries;
CREATE TRIGGER money_entries_no_truncate
BEFORE TRUNCATE ON money_entries
FOR EACH STATEMENT EXECUTE FUNCTION reject_money_entry_mutation();

-- ── Every entry resolves to the account its method names ────────────────────
--
-- The `INNER JOIN` that made a misaligned row invisible becomes a refusal at write time.
-- Deriving `account_id` from the method inside a trigger means the two can never
-- disagree: a writer supplies the method, the trigger supplies the account, and a wrong
-- account is not expressible rather than merely discouraged.
CREATE OR REPLACE FUNCTION money_entry_resolves_account() RETURNS trigger AS $$
DECLARE
    resolved UUID;
BEGIN
    SELECT pm.account_id INTO resolved
      FROM payment_methods pm
     WHERE pm.id = NEW.payment_method_id;

    IF resolved IS NULL THEN
        RAISE EXCEPTION
            'money entry % names payment method % which does not exist',
            NEW.id, NEW.payment_method_id;
    END IF;

    IF NEW.account_id IS DISTINCT FROM resolved THEN
        RAISE EXCEPTION
            'money entry % says account % but its method % resolves to %',
            NEW.id, NEW.account_id, NEW.payment_method_id, resolved;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS money_entries_resolve_account ON money_entries;
CREATE TRIGGER money_entries_resolve_account
BEFORE INSERT ON money_entries
FOR EACH ROW EXECUTE FUNCTION money_entry_resolves_account();

-- ── Backfill: every document that already moved money gets its row ─────────
--
-- `S1`. This is the part that makes the migration correct on a database with data, not
-- just on a fresh one. `creator` is `backfill` because no user concept existed when these
-- documents were written — inventing a name would be a lie, and `backfill` is the truth.
--
-- The sign convention is `0014`'s, copied deliberately rather than re-derived:
--   sale            +total   (money came in)
--   credit_note     -total   (money went back to the customer)
--   purchase        -total   (money went out to the supplier)
--   supplier_return +total   (the supplier gave it back)
--
-- `occurred_at` takes the document's own `created_at`, so a backfilled book sits on the
-- same timeline as the documents it explains.
INSERT INTO money_entries
    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
SELECT pm.account_id, s.payment_method_id, 'sale', s.id, s.total, s.created_at, 'backfill'
  FROM sales s
  JOIN payment_methods pm ON pm.id = s.payment_method_id
ON CONFLICT DO NOTHING;

INSERT INTO money_entries
    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
SELECT pm.account_id, cn.payment_method_id, 'credit_note', cn.id, -cn.total, cn.created_at, 'backfill'
  FROM credit_notes cn
  JOIN payment_methods pm ON pm.id = cn.payment_method_id
ON CONFLICT DO NOTHING;

INSERT INTO money_entries
    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
SELECT pm.account_id, p.payment_method_id, 'purchase', p.id, -p.total, p.created_at, 'backfill'
  FROM purchases p
  JOIN payment_methods pm ON pm.id = p.payment_method_id
ON CONFLICT DO NOTHING;

INSERT INTO money_entries
    (account_id, payment_method_id, document_kind, document_id, amount, occurred_at, created_by)
SELECT pm.account_id, sr.payment_method_id, 'supplier_return', sr.id, sr.total, sr.created_at, 'backfill'
  FROM supplier_returns sr
  JOIN payment_methods pm ON pm.id = sr.payment_method_id
ON CONFLICT DO NOTHING;

-- ── The view is re-created over the book ───────────────────────────────────
--
-- Same name, same columns, same shape as `0014` so nothing downstream changes — but now
-- the `UNION` is gone and there is nothing left to lose. The document facts a reader may
-- still need (`document_kind`, `document_id`) were already columns of the old view and
-- are columns of the table.
DROP VIEW IF EXISTS money_movements;

CREATE VIEW money_movements AS
SELECT me.payment_method_id,
       me.account_id,
       me.document_kind,
       me.document_id,
       me.amount,
       me.occurred_at
  FROM money_entries me;

COMMENT ON VIEW money_movements IS
    'Read model over money_entries: one row per effect of money on an account. Positive is money entering. The source is a table, so a balance always has a row behind it.';

-- ── Prove the guards exist ─────────────────────────────────────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('money_entries', 'money_entries_append_only'),
              ('money_entries', 'money_entries_no_truncate'),
              ('money_entries', 'money_entries_resolve_account')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 3 THEN
        RAISE EXCEPTION
            'expected 3 money_entries guards, found %; refusing to continue', present;
    END IF;

    IF (SELECT count(*) FROM information_schema.columns
         WHERE table_name = 'accounts' AND column_name = 'kind') <> 1 THEN
        RAISE EXCEPTION 'accounts.kind is missing; refusing to continue';
    END IF;

    -- The book and the documents agree on how many rows there should be. A backfill that
    -- silently lost a branch would pass every other check in this file.
    IF (SELECT count(*) FROM money_entries WHERE created_by = 'backfill')
       <> (SELECT (SELECT count(*) FROM sales)
                 + (SELECT count(*) FROM credit_notes)
                 + (SELECT count(*) FROM purchases)
                 + (SELECT count(*) FROM supplier_returns)) THEN
        RAISE EXCEPTION
            'backfill wrote % entries for four documents sets totalling %; refusing to continue',
            (SELECT count(*) FROM money_entries WHERE created_by = 'backfill'),
            (SELECT (SELECT count(*) FROM sales)
                  + (SELECT count(*) FROM credit_notes)
                  + (SELECT count(*) FROM purchases)
                  + (SELECT count(*) FROM supplier_returns));
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0018_three_way_documents.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S3: money that did not come from a sale.
--
-- `odd/tasks/v4-settlement.md` S12. Until now there were exactly two ways for money to
-- move: a document that sold something and a document that reversed it. Three ordinary
-- events had no representation at all:
--
--   * **the opening balance** — what was in the till the first morning. `stock_movements`
--     has known how to start since `0002` (`reason = 'initial'`); the money book does not,
--     so its starting point is the first sale rather than the first morning, and the
--     balance of 3,000 in the development database has no opening row behind it.
--   * **a transfer** between the shop's own accounts — the till to the bank. Today the
--     only way to express it is a fake sale.
--   * **a count adjustment** — "I counted 2,850 and the book says 3,000". An adjustment
--     document, never a rewritten number.
--
-- Each is a document with its own id, so the `L11` rule holds: corrections and additions
-- are new documents, never edits. Each writes one or two entries, and none of them
-- invents a payment method: `money_entries.payment_method_id` is `NOT NULL` because a
-- movement's account is derived from a method, so a document whose movement is not
-- "through a rail" carries a per-kind marker instead (see `0018`'s own note below).

-- ── The three documents ────────────────────────────────────────────────────
--
-- **Why these carry a marker method rather than a nullable `payment_method_id`.** The
-- book derives `account_id` from the method by trigger (`0017`), and that derivation is
-- what made the old `INNER JOIN` impossible to lose a row through. Making the column
-- nullable would reopen exactly that hole. So an opening balance is recorded as entering
-- through `cash` — the shop's own statement that the till had money in it — while a
-- transfer and an adjustment record the real method of the account they touch when there
-- is one.
--
-- A count adjustment is a **real difference in a real account**, so it carries the method
-- the account's money normally arrives by, which the caller supplies.

CREATE TABLE IF NOT EXISTS opening_balances (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts (id),
    amount     NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    opened_at  TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL,
    CONSTRAINT opening_balances_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT opening_balances_created_by_not_blank CHECK (btrim(created_by) <> '')
);

-- One per account, and this is the whole reason the document needs a UNIQUE: a second
-- opening balance for the same account is not a beginning, it is a correction wearing a
-- beginning's clothes. Measured before writing it: nothing prevented two.
CREATE UNIQUE INDEX IF NOT EXISTS opening_balances_account_unique
    ON opening_balances (account_id);

CREATE TABLE IF NOT EXISTS transfers (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_account_id UUID NOT NULL REFERENCES accounts (id),
    to_account_id   UUID NOT NULL REFERENCES accounts (id),
    amount         NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    occurred_at    TIMESTAMPTZ NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by     TEXT NOT NULL,
    CONSTRAINT transfers_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT transfers_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- Moving money to the account it is already in is not a transfer; it is a typo that
    -- would write two entries summing to zero and a document nobody can explain.
    CONSTRAINT transfers_between_two_accounts CHECK (from_account_id <> to_account_id)
);

-- Two rails per transfer: money leaves through the source account's method and arrives
-- through the destination's. They are named on the document so the book never has to
-- guess, and so a transfer is expressible between the till and a bank that have entirely
-- different methods.
ALTER TABLE transfers
    ADD COLUMN IF NOT EXISTS from_payment_method_id UUID REFERENCES payment_methods (id);
ALTER TABLE transfers
    ADD COLUMN IF NOT EXISTS to_payment_method_id UUID REFERENCES payment_methods (id);

CREATE TABLE IF NOT EXISTS count_adjustments (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id        UUID NOT NULL REFERENCES accounts (id),
    payment_method_id UUID NOT NULL REFERENCES payment_methods (id),
    -- Signed: a positive difference is money the count found that the book did not have,
    -- a negative one is money the book claims and the drawer does not. Zero is refused by
    -- the CHECK because an adjustment of nothing is not a document.
    difference        NUMERIC(14, 6) NOT NULL CHECK (difference <> 0),
    counted_at        TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        TEXT NOT NULL,
    CONSTRAINT count_adjustments_whole_cents CHECK (difference = round(difference, 2)),
    CONSTRAINT count_adjustments_created_by_not_blank CHECK (btrim(created_by) <> '')
);

CREATE INDEX IF NOT EXISTS count_adjustments_account_id_idx
    ON count_adjustments (account_id);

-- ── The book learns three new document kinds ───────────────────────────────
--
-- `0017` closed the set with a CHECK, deliberately: an unrecognised kind is a balance
-- change whose cause cannot be named. Adding to a closed set is an `ALTER`, and doing it
-- here rather than by reopening the column is what keeps the guarantee.
ALTER TABLE money_entries DROP CONSTRAINT IF EXISTS money_entries_kind_known;

ALTER TABLE money_entries
    ADD CONSTRAINT money_entries_kind_known CHECK (
        document_kind IN (
            'sale', 'credit_note', 'purchase', 'supplier_return',
            'opening_balance', 'transfer', 'count_adjustment'
        )
    );

-- ── Append-only, the same three guards ────────────────────────────────────
CREATE OR REPLACE FUNCTION reject_three_way_document_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DO $$
DECLARE
    t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['opening_balances', 'transfers', 'count_adjustments'] LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', t || '_append_only', t);
        EXECUTE format(
            'CREATE TRIGGER %I BEFORE UPDATE OR DELETE ON %I
             FOR EACH ROW EXECUTE FUNCTION reject_three_way_document_mutation()',
            t || '_append_only', t);
        EXECUTE format('DROP TRIGGER IF EXISTS %I ON %I', t || '_no_truncate', t);
        EXECUTE format(
            'CREATE TRIGGER %I BEFORE TRUNCATE ON %I
             FOR EACH STATEMENT EXECUTE FUNCTION reject_three_way_document_mutation()',
            t || '_no_truncate', t);
    END LOOP;
END;
$$;

-- ── A transfer moves money between two DIFFERENT accounts, and both must exist ──
--
-- The accounts named on the document and the methods' accounts have to agree, for the
-- same reason `0017` derives the entry's account from its method: a document that says one
-- account and a book that says another is the desync this slice exists to remove.
CREATE OR REPLACE FUNCTION transfer_methods_match_accounts() RETURNS trigger AS $$
DECLARE
    from_resolved UUID;
    to_resolved   UUID;
BEGIN
    SELECT pm.account_id INTO from_resolved
      FROM payment_methods pm WHERE pm.id = NEW.from_payment_method_id;
    SELECT pm.account_id INTO to_resolved
      FROM payment_methods pm WHERE pm.id = NEW.to_payment_method_id;

    IF from_resolved IS NULL OR to_resolved IS NULL THEN
        RAISE EXCEPTION
            'transfer % names a payment method that does not exist', NEW.id;
    END IF;
    IF from_resolved IS DISTINCT FROM NEW.from_account_id THEN
        RAISE EXCEPTION
            'transfer % leaves account % but its method resolves to %',
            NEW.id, NEW.from_account_id, from_resolved;
    END IF;
    IF to_resolved IS DISTINCT FROM NEW.to_account_id THEN
        RAISE EXCEPTION
            'transfer % arrives at account % but its method resolves to %',
            NEW.id, NEW.to_account_id, to_resolved;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS transfers_methods_match ON transfers;
CREATE TRIGGER transfers_methods_match
BEFORE INSERT ON transfers
FOR EACH ROW EXECUTE FUNCTION transfer_methods_match_accounts();

-- A count adjustment's account and method must agree too, for the same reason.
CREATE OR REPLACE FUNCTION count_adjustment_method_matches() RETURNS trigger AS $$
DECLARE
    resolved UUID;
BEGIN
    SELECT pm.account_id INTO resolved
      FROM payment_methods pm WHERE pm.id = NEW.payment_method_id;
    IF resolved IS NULL THEN
        RAISE EXCEPTION
            'count adjustment % names a payment method that does not exist', NEW.id;
    END IF;
    IF resolved IS DISTINCT FROM NEW.account_id THEN
        RAISE EXCEPTION
            'count adjustment % says account % but its method resolves to %',
            NEW.id, NEW.account_id, resolved;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS count_adjustments_method_matches ON count_adjustments;
CREATE TRIGGER count_adjustments_method_matches
BEFORE INSERT ON count_adjustments
FOR EACH ROW EXECUTE FUNCTION count_adjustment_method_matches();

-- ── Prove the guards exist ─────────────────────────────────────────────────
DO $$
DECLARE
    present bigint;
    expected bigint := 6; -- two per table, three tables
BEGIN
    SELECT count(*) INTO present
      FROM pg_trigger t
     WHERE NOT t.tgisinternal
       AND t.tgrelid IN ('opening_balances'::regclass,
                         'transfers'::regclass,
                         'count_adjustments'::regclass)
       AND t.tgname LIKE '%_append_only' OR t.tgname LIKE '%_no_truncate';
    -- The predicate above cannot be written as two ORs without parentheses and still be
    -- correct, so the count is taken again explicitly rather than trusting it.
    SELECT count(*) INTO present
      FROM (VALUES
              ('opening_balances', 'opening_balances_append_only'),
              ('opening_balances', 'opening_balances_no_truncate'),
              ('transfers', 'transfers_append_only'),
              ('transfers', 'transfers_no_truncate'),
              ('count_adjustments', 'count_adjustments_append_only'),
              ('count_adjustments', 'count_adjustments_no_truncate'),
              ('transfers', 'transfers_methods_match'),
              ('count_adjustments', 'count_adjustments_method_matches')
           ) AS wanted(tbl, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.tbl::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 8 THEN
        RAISE EXCEPTION
            'expected 8 guards on the three new documents, found %; refusing to continue',
            present;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'transfers_between_two_accounts'
    ) THEN
        RAISE EXCEPTION 'transfers must be between two different accounts; refusing to continue';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'opening_balances_account_unique'
    ) THEN
        RAISE EXCEPTION 'an account may have only one opening balance; refusing to continue';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0019_reversal_budget.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S4: the reversal budget lives on the sale, not on the credit-note table.
--
-- `odd/tasks/v4-settlement.md` S9. Until now the rule "a line may not be reversed more
-- than it was sold" lived in **one place and was enforced in another**:
--
--   * the handler computed `SUM(credit_note_items.quantity)` against
--     `sale_items.quantity`, inside the sale's row lock (`sales.rs`);
--   * the schema enforced **nothing at all**.
--
-- Measured before this file existed: a `psql` session inserted 5 units against a line of
-- 1 and nothing objected. The comment in `0007` says the guard is "enforced in the write
-- path", which was true and was the problem — a guard one route applies is a guard the
-- next route forgets.
--
-- **Why this must move now rather than later.** The annulment (`S10`) is a *second*
-- document that reverses stock. If the budget stays a `SUM` over one document table, an
-- annulment either does not count (and the line can be reversed twice: once by an
-- annulment, once by a credit note) or it counts in a place only it knows about. The
-- budget belongs to the resource both documents act on: the sale line.

-- ── `sale_items` stops being blanket-immutable, for exactly one column ─────
--
-- **This is the second thing `0019` was going to break, and it was found by running it.**
-- `0003` installed `sale_items_immutable` as `BEFORE UPDATE OR DELETE` with an
-- unconditional `RAISE`. That is right for every column on the line — name, price,
-- quantity and total are history — but it also refuses the counter this slice needs, so
-- the backfill below aborted with
-- *"sale_items are immutable: corrections are compensating rows, not edits"*.
--
-- The repo already has the pattern for exactly this: `guard_sale_economics` (`0003`, and
-- re-cut in `0013`) refuses the sale's money columns and permits the one column whose
-- whole purpose is to change. This is the same technique one table deeper. The guard is
-- **narrowed, not removed**: every column except `reversed_quantity` is still refused,
-- and the budget has its own guard (`sale_items_reversal_budget`) enforcing that it may
-- only grow toward — never past — the quantity sold.
CREATE OR REPLACE FUNCTION reject_sale_item_mutation() RETURNS trigger AS $$
BEGIN
    -- **Every column is compared, `id` included.** The first version of this guard left
    -- `id` out of the list, so an `UPDATE` that rewrote only the primary key fell into the
    -- "only the budget changed" branch and was allowed. Found by review (`R3-001`,
    -- CRITICAL) and reproduced against a live database before this fix.
    IF TG_OP = 'UPDATE'
       AND NEW.id                    = OLD.id
       AND NEW.sale_id               = OLD.sale_id
       AND NEW.product_id            = OLD.product_id
       AND NEW.product_name          = OLD.product_name
       AND NEW.quantity              = OLD.quantity
       AND NEW.unit_price            = OLD.unit_price
       AND NEW.line_total            = OLD.line_total
       AND NEW.unit                  = OLD.unit
    THEN
        -- **A budget increase is allowed only from the consumer.** Everything privileged
        -- inside this transaction sets `tpox.budget_writer`, and nothing else does; a bare
        -- `UPDATE sale_items SET reversed_quantity = ...` from `psql` is refused. Without
        -- this the counter could be inflated with no credit note at all, and the handler
        -- — which reads it — would then refuse a refund that was legitimate (`R3-002`,
        -- CRITICAL, also reproduced).
        IF NEW.reversed_quantity = OLD.reversed_quantity
           OR current_setting('tpox.budget_writer', true) = 'on'
        THEN
            RETURN NEW;
        END IF;
        RAISE EXCEPTION
            'sale item % reversal budget may only move through the refund path', NEW.id;
    END IF;
    RAISE EXCEPTION 'sale_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

-- ── The column ─────────────────────────────────────────────────────────────
--
-- `reversed_quantity` is a running total rather than a derived sum, which is a deliberate
-- departure from this schema's usual "derive, never store". The reason is the one S9
-- gives: two different documents consume the same budget, and a derived sum over one of
-- them is exactly how the second document bypassed it. A stored counter with a guard is
-- the only shape in which "consumed by any document" is expressible.
ALTER TABLE sale_items
    ADD COLUMN IF NOT EXISTS reversed_quantity INTEGER NOT NULL DEFAULT 0;

-- ── Backfill, then prove it ────────────────────────────────────────────────
--
-- A migration that refuses to run on a database with refunds is not a migration: the
-- development database has one already, and every delivered one will. The figure comes
-- from the documents that exist, and the verification below compares back to its own
-- source rather than trusting the `UPDATE`.
UPDATE sale_items si
   SET reversed_quantity = COALESCE(refunded.total, 0)
  FROM (
        SELECT sale_item_id, SUM(quantity)::int AS total
          FROM credit_note_items
         GROUP BY sale_item_id
       ) AS refunded
 WHERE refunded.sale_item_id = si.id;

-- ── The guards ─────────────────────────────────────────────────────────────
--
-- Three ways to break the budget, and all three are refused:
--   * a credit-note item that would take the total past what was sold;
--   * a lower `reversed_quantity` than the ledger already accounts for (a rewrite of the
--     counter that hides history);
--   * a negative counter, or one above the quantity sold, from any other path.
CREATE OR REPLACE FUNCTION guard_sale_item_reversal_budget() RETURNS trigger AS $$
DECLARE
    sold integer;
BEGIN
    IF NEW.reversed_quantity < 0 THEN
        RAISE EXCEPTION
            'sale item % cannot have reversed a negative quantity', NEW.id;
    END IF;

    SELECT quantity INTO sold FROM sale_items WHERE id = NEW.id;
    IF NEW.reversed_quantity > sold THEN
        RAISE EXCEPTION
            'sale item % reversed % of % sold: a line cannot be reversed more than it was sold',
            NEW.id, NEW.reversed_quantity, sold;
    END IF;

    -- A counter may only grow. The reversal documents are append-only, so a shrinking
    -- counter is the one edit that would let the same unit be reversed twice.
    IF TG_OP = 'UPDATE' AND NEW.reversed_quantity < OLD.reversed_quantity THEN
        RAISE EXCEPTION
            'sale item % reversal budget cannot shrink: % -> %',
            NEW.id, OLD.reversed_quantity, NEW.reversed_quantity;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sale_items_reversal_budget ON sale_items;
CREATE TRIGGER sale_items_reversal_budget
BEFORE INSERT OR UPDATE OF reversed_quantity ON sale_items
FOR EACH ROW EXECUTE FUNCTION guard_sale_item_reversal_budget();

-- The document side: inserting a credit-note line consumes the budget, and is refused
-- when the budget is gone. This is the guard that was missing, and it is a trigger rather
-- than a CHECK because a row-level CHECK cannot see the sibling rows it would have to sum.
CREATE OR REPLACE FUNCTION consume_reversal_budget() RETURNS trigger AS $$
DECLARE
    remaining integer;
BEGIN
    -- Lock the line before reading, so two concurrent refunds cannot both see the same
    -- remaining balance. `0007` relied on the handler's `FOR UPDATE` on the sale; the
    -- schema cannot rely on a caller's lock discipline.
    SELECT quantity - reversed_quantity INTO remaining
      FROM sale_items WHERE id = NEW.sale_item_id
       FOR UPDATE;

    IF remaining IS NULL THEN
        RAISE EXCEPTION
            'credit note item names sale item % which does not exist', NEW.sale_item_id;
    END IF;

    IF NEW.quantity > remaining THEN
        RAISE EXCEPTION
            'sale item % has % left to reverse, but % were requested',
            NEW.sale_item_id, remaining, NEW.quantity;
    END IF;

    -- The only writer allowed to move the counter, announced in the transaction's own
    -- session settings so a direct `UPDATE` cannot impersonate it.
    PERFORM set_config('tpox.budget_writer', 'on', true);
    UPDATE sale_items
       SET reversed_quantity = reversed_quantity + NEW.quantity
     WHERE id = NEW.sale_item_id;
    PERFORM set_config('tpox.budget_writer', 'off', true);

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_note_items_consume_budget ON credit_note_items;
CREATE TRIGGER credit_note_items_consume_budget
BEFORE INSERT ON credit_note_items
FOR EACH ROW EXECUTE FUNCTION consume_reversal_budget();

-- ── Prove the guards exist, and that the backfill matches its source ───────
--
-- LAST, for the reason `0017` records: a check placed earlier names whatever fails next
-- instead of the guard that is actually absent.
DO $$
DECLARE
    present bigint;
    drifted bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('sale_items', 'sale_items_reversal_budget'),
              ('credit_note_items', 'credit_note_items_consume_budget')
           ) AS wanted(tbl, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.tbl::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 2 THEN
        RAISE EXCEPTION
            'expected 2 reversal-budget guards, found %; refusing to continue', present;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_name = 'sale_items' AND column_name = 'reversed_quantity'
    ) THEN
        RAISE EXCEPTION 'sale_items.reversed_quantity is missing; refusing to continue';
    END IF;

    -- The backfill against its own source. A migration that silently under-counted would
    -- pass every other check in this file and leave budget that does not exist.
    SELECT count(*) INTO drifted
      FROM sale_items si
      LEFT JOIN (
            SELECT sale_item_id, SUM(quantity)::int AS total
              FROM credit_note_items
             GROUP BY sale_item_id
           ) r ON r.sale_item_id = si.id
     WHERE si.reversed_quantity <> COALESCE(r.total, 0);
    IF drifted <> 0 THEN
        RAISE EXCEPTION
            '% sale items have a reversal budget that disagrees with the credit notes on record; refusing to continue',
            drifted;
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0020_annulments.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S5: a sale reported wrongly can be annulled.
--
-- `odd/tasks/v4-settlement.md` S10, S11. Measured before this file existed: `sales` has
-- no `status` and no `reversed_at` (v3 dropped them), the routes are create, list and
-- `POST /sales/{id}/credit-notes`, and **there is no way to correct a sale that was
-- loaded wrong**.
--
-- The only exit today is a full credit note, which means "the customer returned the
-- goods". So the stock moves as if merchandise came back, and it never did — the
-- correction is worse than the original mistake, and it was already happening.
--
-- An annulment is a different document for a different fact: *this sale was reported
-- wrongly*. It reverses money and stock, consumes the reversal budget, carries a reason,
-- and never edits or deletes the sale itself (`S11`: corrections are new documents).

-- ── The document ───────────────────────────────────────────────────────────
--
-- The reason is a closed set, like `stock_movements.reason` and
-- `money_entries.document_kind`: an unrecognised reason is a correction whose cause
-- cannot be named.
CREATE TABLE IF NOT EXISTS annulments (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sale_id    UUID NOT NULL REFERENCES sales (id),
    reason     TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL,
    CONSTRAINT annulments_reason_known CHECK (
        reason IN ('wrong_amount', 'wrong_method', 'wrong_customer', 'wrong_document')
    ),
    CONSTRAINT annulments_created_by_not_blank CHECK (btrim(created_by) <> '')
);

-- **One annulment per sale.** A second one is not a second correction; it is the same
-- correction asked twice, and it would reverse money twice.
CREATE UNIQUE INDEX IF NOT EXISTS annulments_sale_unique ON annulments (sale_id);

-- ── The stock ledger learns the reason too ─────────────────────────────────
--
-- Found by running the route, not by reading: the first version wrote
-- `reason = 'sale_annulled'` and the insert failed, because `0004` closed the set with
-- `stock_movements_reason_known` and `0010` extended it once already. An annulment returns
-- stock for a cause none of the existing reasons name — the goods never left — so the set
-- grows the same way `0010` grew it.
--
-- **This is not a refund.** A credit note puts stock back because the customer brought it;
-- an annulment puts it back because the sale was never real. Two different facts, two
-- different reasons, and the stock ledger can now tell them apart.
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known CHECK (
        reason IN (
            'initial', 'sale', 'credit_note', 'purchase', 'supplier_return',
            'sale_annulled'
        )
    );

-- ── The book learns the kind ───────────────────────────────────────────────
ALTER TABLE money_entries DROP CONSTRAINT IF EXISTS money_entries_kind_known;
ALTER TABLE money_entries
    ADD CONSTRAINT money_entries_kind_known CHECK (
        document_kind IN (
            'sale', 'credit_note', 'purchase', 'supplier_return',
            'opening_balance', 'transfer', 'count_adjustment', 'annulment'
        )
    );

-- ── Append-only, the same three ways ──────────────────────────────────────
CREATE OR REPLACE FUNCTION reject_annulment_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'annulments are append-only: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS annulments_append_only ON annulments;
CREATE TRIGGER annulments_append_only
BEFORE UPDATE OR DELETE ON annulments
FOR EACH ROW EXECUTE FUNCTION reject_annulment_mutation();

DROP TRIGGER IF EXISTS annulments_no_truncate ON annulments;
CREATE TRIGGER annulments_no_truncate
BEFORE TRUNCATE ON annulments
FOR EACH STATEMENT EXECUTE FUNCTION reject_annulment_mutation();

-- ── Annulling consumes the same budget a refund does ──────────────────────
--
-- **This is why `0019` moved the budget onto the sale line.** The credit-note consumer
-- locks the line, checks what remains and increments the counter; an annulment reverses
-- *whatever is left*, so it must see the same counter or the same unit gets reversed
-- twice — once by a credit note, once by an annulment.
--
-- The writer flag is the one `0019` established: the narrowed immutability guard on
-- `sale_items` accepts a budget increase only when the transaction announces itself.
CREATE OR REPLACE FUNCTION annulment_consumes_reversal_budget() RETURNS trigger AS $$
DECLARE
    touched integer;
BEGIN
    PERFORM set_config('tpox.budget_writer', 'on', true);

    -- Reverse every line that still has budget.
    UPDATE sale_items
       SET reversed_quantity = quantity
     WHERE sale_id = NEW.sale_id
       AND quantity > reversed_quantity;

    -- **`R3-002`, BLOCKER, and it was reachable.** The first version ran that UPDATE and
    -- never asked whether it touched anything, so a sale whose lines were already fully
    -- reversed still got an annulment row — and the route then wrote another full money
    -- and stock reversal on top of a credit note that had already returned everything.
    -- Measured: sale of 3, refund of 1, annulment → till −100 and stock 11 of 10.
    GET DIAGNOSTICS touched = ROW_COUNT;
    IF touched = 0 THEN
        RAISE EXCEPTION
            'sale % has nothing left to annul: every line is already reversed', NEW.sale_id;
    END IF;

    PERFORM set_config('tpox.budget_writer', 'off', true);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS annulments_consume_budget ON annulments;
CREATE TRIGGER annulments_consume_budget
BEFORE INSERT ON annulments
FOR EACH ROW EXECUTE FUNCTION annulment_consumes_reversal_budget();

-- ── The stock ledger learns the annulment's SHAPE, not just its name ───────
--
-- Found by running the route: `reason = 'sale_annulled'` was refused with
-- *"says reason sale_annulled but its shape says sale"*. `0011` replaced six CHECK
-- constraints with one trigger whose `CASE` derives the reason **from the row's shape**,
-- deliberately, so that adding a reason means adding a shape in one place instead of
-- editing three unrelated constraints. Adding a value to the CHECK alone, which is what
-- this file did first, cannot work.
--
-- So an annulled sale gets its own column and its own shape:
--
--     sale_id NOT NULL, annulment_id NOT NULL, quantity > 0  ->  'sale_annulled'
--
-- A credit note and an annulment both name a sale, and the difference between them —
-- "the customer brought it back" versus "the sale was never real" — is exactly the
-- `annulment_id`. Two facts that were indistinguishable in the stock ledger are now
-- distinguishable.
ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS annulment_id UUID REFERENCES annulments (id);

CREATE INDEX IF NOT EXISTS stock_movements_annulment_id_idx
    ON stock_movements (annulment_id);

-- The shape rules: an annulment names a sale and never a credit note, and a sale row with
-- an annulment is not a sale row. Both directions are checked, so neither can be faked by
-- setting only one column.
CREATE OR REPLACE FUNCTION ledger_movement_is_coherent() RETURNS trigger AS $$
DECLARE
    expected_reason   text;
    expected_positive boolean;
BEGIN
    IF NEW.sale_id IS NOT NULL AND NEW.purchase_id IS NOT NULL THEN
        RAISE EXCEPTION
            'stock movement % names both a sale and a purchase: a movement belongs to one document',
            NEW.id;
    END IF;
    IF NEW.annulment_id IS NOT NULL AND NEW.sale_id IS NULL THEN
        RAISE EXCEPTION
            'stock movement % carries an annulment but no sale: the annulment corrects a sale',
            NEW.id;
    END IF;
    IF NEW.annulment_id IS NOT NULL AND NEW.credit_note_id IS NOT NULL THEN
        RAISE EXCEPTION
            'stock movement % carries both an annulment and a credit note: one movement, one cause',
            NEW.id;
    END IF;
    IF NEW.credit_note_id IS NOT NULL AND NEW.sale_id IS NULL THEN
        RAISE EXCEPTION
            'stock movement % carries a credit note but no sale: the note corrects a sale',
            NEW.id;
    END IF;
    IF NEW.supplier_return_id IS NOT NULL AND NEW.purchase_id IS NULL THEN
        RAISE EXCEPTION
            'stock movement % carries a supplier return but no purchase: the return corrects a purchase',
            NEW.id;
    END IF;

    -- ── The shape implies the reason and the direction ─────────────────────
    --
    -- The annulment branch goes FIRST, because an annulment row also satisfies
    -- "sale_id is not null and credit_note_id is null" — the condition that used to mean
    -- 'sale'. Order is the whole correctness argument here.
    IF NEW.sale_id IS NULL AND NEW.purchase_id IS NULL THEN
        expected_reason := 'initial';
        expected_positive := NULL;
    ELSIF NEW.annulment_id IS NOT NULL THEN
        expected_reason := 'sale_annulled';
        expected_positive := true;
    ELSIF NEW.sale_id IS NOT NULL AND NEW.credit_note_id IS NULL THEN
        expected_reason := 'sale';
        expected_positive := false;
    ELSIF NEW.sale_id IS NOT NULL THEN
        expected_reason := 'credit_note';
        expected_positive := true;
    ELSIF NEW.purchase_id IS NOT NULL AND NEW.supplier_return_id IS NULL THEN
        expected_reason := 'purchase';
        expected_positive := true;
    ELSE
        expected_reason := 'supplier_return';
        expected_positive := false;
    END IF;

    IF NEW.reason <> expected_reason THEN
        RAISE EXCEPTION
            'stock movement % says reason % but its shape says %',
            NEW.id, NEW.reason, expected_reason;
    END IF;

    IF expected_positive IS NOT NULL AND (NEW.quantity > 0) <> expected_positive THEN
        RAISE EXCEPTION
            'stock movement % is a % and must be %signed',
            NEW.id, expected_reason,
            CASE WHEN expected_positive THEN 'positive-' ELSE 'negative-' END;
    END IF;

    -- Belonging and parent-child checks, unchanged from `0011`: the product must be on
    -- the sale, and a credit note must correct THIS sale.
    IF NEW.sale_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM sale_items si
         WHERE si.sale_id = NEW.sale_id AND si.product_id = NEW.product_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % names product % which is not on sale %',
            NEW.id, NEW.product_id, NEW.sale_id;
    END IF;

    IF NEW.annulment_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM annulments a
         WHERE a.id = NEW.annulment_id AND a.sale_id = NEW.sale_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % carries annulment % which does not correct sale %',
            NEW.id, NEW.annulment_id, NEW.sale_id;
    END IF;

    IF NEW.credit_note_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM credit_notes cn
         WHERE cn.id = NEW.credit_note_id AND cn.sale_id = NEW.sale_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % carries credit note % which does not correct sale %',
            NEW.id, NEW.credit_note_id, NEW.sale_id;
    END IF;

    IF NEW.purchase_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM purchase_items pi
         WHERE pi.purchase_id = NEW.purchase_id AND pi.product_id = NEW.product_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % names product % which is not on purchase %',
            NEW.id, NEW.product_id, NEW.purchase_id;
    END IF;

    IF NEW.supplier_return_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM supplier_returns sr
         WHERE sr.id = NEW.supplier_return_id AND sr.purchase_id = NEW.purchase_id
    ) THEN
        RAISE EXCEPTION
            'stock movement % carries supplier return % which does not correct purchase %',
            NEW.id, NEW.supplier_return_id, NEW.purchase_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── Prove the guards exist ─────────────────────────────────────────────────
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('annulments', 'annulments_append_only'),
              ('annulments', 'annulments_no_truncate'),
              ('annulments', 'annulments_consume_budget')
           ) AS wanted(tbl, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.tbl::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 3 THEN
        RAISE EXCEPTION
            'expected 3 annulment guards, found %; refusing to continue', present;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes WHERE indexname = 'annulments_sale_unique'
    ) THEN
        RAISE EXCEPTION 'a sale may be annulled only once; refusing to continue';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0021_obligations.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S6a: what we are owed is a document, not a wallet.
--
-- `odd/tasks/v4-receivables.md` S1-S3, S7, S8. This is the slice where `credit` stops
-- being an account, and the first half of it is a **subtraction from the book**.
--
-- The defect, restated because the schema below is the answer to it: `credit` is not a
-- wallet. It is an obligation wearing an account's clothes — it holds nothing, and it
-- answers "what am I owed" while pretending to answer "where is the money"
-- (`v4-settlement.md`, the diagnosis). Until now a sale on account wrote
-- `credit +total` into `money_entries`, which means the debt was recorded as a money
-- movement into an account that never received money.
--
-- Two consequences shape everything here:
--
--   * **A sale on account moves no money.** It writes an obligation and **zero** ledger
--     entries, so `sales.payment_method_id` becomes NULL for it — and the NULL is a
--     claim that a constraint trigger holds, never an arbitrary null.
--   * **Lowering what is owed is a document too.** A credit note on an unpaid sale used
--     to inherit `credit` and write `-total`, which is how a refund of a sale nobody paid
--     took real money out of the till (`v3-accounts.md` L107). Now it inherits the
--     sale's NULL, writes no entry, and lowers the debt through an
--     `obligations_adjustments` row.

-- ── obligations: the debt itself ───────────────────────────────────────────
--
-- `source_document_kind` is closed to `sale` for now, deliberately: an unrecognised
-- source is a debt whose cause cannot be named, exactly as an unrecognised
-- `money_entries.document_kind` is (`0017`). T7 adds `purchase` by `ALTER`, the same way
-- `0018` and `0020` extended their sets — reopening the column would lose the guarantee.
--
-- **No foreign key on `source_document_id`, and this is not laziness.** It points at
-- `sales` today and at `purchases` after T7, and PostgreSQL has no polymorphic FK. The
-- trigger below closes the gap by deriving the party from the document it names, the
-- technique `0011` established for `stock_movements` and `0017` for the book.
--
-- **No `amount > 0` alone.** A sale of nothing but bonus lines has `total = 0` (`0005`),
-- and `0017` learned this the expensive way: `CHECK (amount <> 0)` on the book turned a
-- valid bonus sale into a 500. A zero obligation is a real obligation that is settled the
-- moment it exists, so the CHECK is `>= 0`.
CREATE TABLE IF NOT EXISTS obligations (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_document_kind TEXT NOT NULL,
    source_document_id   UUID NOT NULL,
    party_kind           TEXT NOT NULL,
    party_id             UUID NOT NULL,
    amount               NUMERIC(14, 6) NOT NULL CHECK (amount >= 0),
    occurred_at          TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by           TEXT NOT NULL,

    CONSTRAINT obligations_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT obligations_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT obligations_source_known CHECK (source_document_kind IN ('sale')),
    CONSTRAINT obligations_party_known CHECK (party_kind IN ('customer', 'supplier'))
);

-- **One debt per document, and the UNIQUE is the rule rather than a convention.** A
-- second obligation naming the same sale would make "what is owed" a sum of two rows
-- nobody can tell apart, which is the same defect as two records of one debt.
CREATE UNIQUE INDEX IF NOT EXISTS obligations_source_unique
    ON obligations (source_document_kind, source_document_id);

CREATE INDEX IF NOT EXISTS obligations_party_idx
    ON obligations (party_kind, party_id);

-- ── obligations_adjustments: the debt shrinking ────────────────────────────
--
-- `S8`. A credit note that inherits the sale's NULL method moves no money and lowers
-- what is owed, and a cancellation lowers it too. **The alternative was a mutable
-- `obligations.amount` behind a `tpox.*_writer` flag** — the pattern `0019` used for
-- `sale_items.reversed_quantity`, which then cost a CRITICAL finding in the review of
-- `4ffb95e` (`v4-settlement.md` V7, `R3-002`). The honest shape exists here, so the flag
-- is not bought again: the debt drops through a row, and the balance stays a subtraction
-- of stored rows.
--
-- So the outstanding figure is
--
--     obligations.amount - SUM(settlements) - SUM(obligations_adjustments)
--
-- and a partial drop is the normal case instead of a special one.
CREATE TABLE IF NOT EXISTS obligations_adjustments (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    obligation_id UUID NOT NULL REFERENCES obligations (id),
    -- Positive, with the sign carried by the table: an adjustment always lowers the
    -- debt, and a column that sometimes adds is a column every reader must re-derive.
    amount        NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    document_kind TEXT NOT NULL,
    document_id   UUID NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by    TEXT NOT NULL,

    CONSTRAINT obligations_adjustments_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT obligations_adjustments_created_by_not_blank
        CHECK (btrim(created_by) <> ''),
    CONSTRAINT obligations_adjustments_kind_known
        CHECK (document_kind IN ('credit_note', 'cancellation'))
);

-- One adjustment per document: a credit note lowers the debt exactly once, and a second
-- row naming the same note would be the same correction counted twice.
CREATE UNIQUE INDEX IF NOT EXISTS obligations_adjustments_document_unique
    ON obligations_adjustments (document_kind, document_id);

CREATE INDEX IF NOT EXISTS obligations_adjustments_obligation_idx
    ON obligations_adjustments (obligation_id);

-- **The ceiling, adjusted side only.** `SUM(adjustments) <= amount`, checked at write
-- time. `0022` re-cuts this function to include settlements, because the combined ceiling
-- is the rule and the two halves cannot be judged apart — an obligation is not
-- oversettled or over-adjusted, it is over-*resolved*. Until settlements exist, the
-- adjustment half is the whole rule.
CREATE OR REPLACE FUNCTION obligation_adjustments_within_amount() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
BEGIN
    -- **`FOR UPDATE` on the obligation row, and it is the whole guarantee.** Without it two
    -- concurrent adjustments of the same obligation each read the same committed sum, each
    -- pass, and together lower the debt past zero — leaving a negative outstanding figure
    -- that no route can produce but `psql` can (review `R1-ceiling-race` / `R4-ceiling-trigger-reads-without-locking`).
    -- Measured reachable: two sessions, no lock, both inserts accepted.
    --
    -- The row lock is taken on `obligations`, which every adjustment of that obligation
    -- must therefore pass through in order. The HTTP routes already serialise their own
    -- work, so this is the defence for the surface this trigger exists to govern at all.
    SELECT amount INTO owed FROM obligations WHERE id = NEW.obligation_id FOR UPDATE;

    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM obligations_adjustments
     WHERE obligation_id = NEW.obligation_id
       AND id <> NEW.id;

    IF lowered + NEW.amount > owed THEN
        RAISE EXCEPTION
            'obligation % is for %, but % + % would lower it past zero',
            NEW.obligation_id, owed, lowered, NEW.amount;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_adjustments_ceiling ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_ceiling
BEFORE INSERT ON obligations_adjustments
FOR EACH ROW EXECUTE FUNCTION obligation_adjustments_within_amount();

-- ── Append-only, the same three guards every book keeps ────────────────────
--
-- A row-level guard never sees `TRUNCATE`, because it is a set operation and does not
-- fire FOR EACH ROW. `0007` shipped that mistake and `0008` repaired it; both guards go
-- in from the start, as `0017` and `0018` do.
CREATE OR REPLACE FUNCTION reject_obligation_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_append_only ON obligations;
CREATE TRIGGER obligations_append_only
BEFORE UPDATE OR DELETE ON obligations
FOR EACH ROW EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_no_truncate ON obligations;
CREATE TRIGGER obligations_no_truncate
BEFORE TRUNCATE ON obligations
FOR EACH STATEMENT EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_adjustments_append_only ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_append_only
BEFORE UPDATE OR DELETE ON obligations_adjustments
FOR EACH ROW EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_adjustments_no_truncate ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_no_truncate
BEFORE TRUNCATE ON obligations_adjustments
FOR EACH STATEMENT EXECUTE FUNCTION reject_obligation_mutation();

-- ── The sale learns to say "this moved no money" ───────────────────────────
--
-- `S2`, and this is `v4-settlement.md` S4 pulled forward from T8. **The reason is
-- measured, not stylistic.** `0017` put a plain FK from `money_entries.payment_method_id`
-- to `payment_methods.id`, `0012` refuses `DELETE` on both tables, and `0013`'s
-- `payment_methods_account_guard` refuses a real change of `account_id` once a document
-- names the method. S7 promises to retire `credit`; that delete is clean **only while no
-- ledger row names it**, and today none does — the method is seeded `is_active = false`
-- and `sales.rs` reaches it only when a caller asks for it explicitly. A sale on account
-- that wrote `credit +total` would turn S7 into a rewrite of an append-only book.
--
-- **The NULL is a claim, and the trigger below is what holds it.** A nullable column with
-- no rule is an arbitrary null, which is why the two states are enforced together.
ALTER TABLE sales ALTER COLUMN payment_method_id DROP NOT NULL;

-- ── The two states of a sale, checked at commit ────────────────────────────
--
-- `S2`:
--
--     payment_method_id IS NULL      ->  exactly one obligation names this sale
--     payment_method_id IS NOT NULL  ->  zero obligations, and at least one ledger entry
--
-- **A constraint trigger, and `DEFERRABLE INITIALLY DEFERRED` is not optional.** The sale
-- is inserted before the obligation that names it (an obligation needs the sale's id) and
-- the ledger entry is written after the sale is read back, so a plain `AFTER INSERT` would
-- refuse the correct order. Deferring to COMMIT is what lets one transaction build a
-- coherent set of rows and judge it once. That is the same mechanism `0016` used for
-- `accounts_exactly_one_default`, and it carries the same trap, recorded there: **no
-- `ALTER TABLE` on this table may follow a row-touching statement in the same
-- transaction**, because a pending trigger event blocks it.
--
-- `IS DISTINCT FROM` over the pair, not a chain of `<>`: the guard's job is to keep
-- holding when a future migration relaxes something else.
CREATE OR REPLACE FUNCTION sale_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    owed  bigint;
    moved bigint;
BEGIN
    SELECT count(*) INTO owed
      FROM obligations
     WHERE source_document_kind = 'sale' AND source_document_id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'sale' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF owed <> 1 THEN
            RAISE EXCEPTION
                'sale % says it moved no money, but % obligation(s) name it; a sale on account has exactly one',
                NEW.id, owed;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'sale % says it moved no money, but % ledger entr(ies) name it',
                NEW.id, moved;
        END IF;
    ELSE
        IF owed <> 0 THEN
            RAISE EXCEPTION
                'sale % names payment method %, but % obligation(s) also name it; money moved or debt arose, never both',
                NEW.id, NEW.payment_method_id, owed;
        END IF;
        IF moved = 0 THEN
            RAISE EXCEPTION
                'sale % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sales_payment_method_claimed ON sales;
CREATE CONSTRAINT TRIGGER sales_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON sales
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION sale_payment_method_is_claimed();

-- ── The obligation can only name a sale that claims it ────────────────────
--
-- The other half of the pair. Without it the sale-side trigger could be satisfied by an
-- obligation that names a *different* sale, and the party would be free to disagree with
-- the document: a debt attributed to a customer who never bought anything.
--
-- `S5` of `v4-settlement.md`: the party is derived from the document, never trusted from
-- the caller. A sale's debt belongs to the sale's customer, full stop.
CREATE OR REPLACE FUNCTION obligation_matches_its_sale() RETURNS trigger AS $$
DECLARE
    claim_method UUID;
    sale_party   UUID;
BEGIN
    IF NEW.source_document_kind <> 'sale' THEN
        -- Nothing else is reachable yet; T7 adds the purchase branch beside this one.
        RAISE EXCEPTION
            'obligation source kind % has no rule yet', NEW.source_document_kind;
    END IF;

    SELECT s.payment_method_id, s.customer_id
      INTO claim_method, sale_party
      FROM sales s
     WHERE s.id = NEW.source_document_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'obligation names sale % which does not exist', NEW.source_document_id;
    END IF;

    IF claim_method IS NOT NULL THEN
        RAISE EXCEPTION
            'sale % names payment method %: money moved, so it owes nothing',
            NEW.source_document_id, claim_method;
    END IF;

    IF NEW.party_kind <> 'customer' OR NEW.party_id IS DISTINCT FROM sale_party THEN
        RAISE EXCEPTION
            'obligation on sale % must belong to that sale''s customer %, not % %',
            NEW.source_document_id, sale_party, NEW.party_kind, NEW.party_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_match_their_sale ON obligations;
CREATE CONSTRAINT TRIGGER obligations_match_their_sale
AFTER INSERT ON obligations
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION obligation_matches_its_sale();

-- ── A credit note on an unpaid sale moves no money ────────────────────────
--
-- `S3`, and the sign is the whole point. A note that inherits the sale's NULL method must
-- write **no** ledger entry and exactly one adjustment; a note that names a method must
-- write a ledger entry and no adjustment. Before this, the inherited path wrote `-total`
-- against `credit`, which is how `L107`'s refund of a sale nobody paid took real money out
-- of the till.
--
-- **A note may be NULL only when its sale is NULL.** A note on a paid sale that moved no
-- money would be a credit balance with no payment behind it — money from nowhere. The
-- refund-a-balance case is a `payments` row with `direction = out` (`S4`), not a note.
ALTER TABLE credit_notes ALTER COLUMN payment_method_id DROP NOT NULL;

CREATE OR REPLACE FUNCTION credit_note_moves_money_or_lowers_debt()
RETURNS trigger AS $$
DECLARE
    sale_method UUID;
    moved       bigint;
    lowered     bigint;
BEGIN
    SELECT s.payment_method_id INTO sale_method
      FROM credit_notes cn JOIN sales s ON s.id = cn.sale_id
     WHERE cn.id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'credit_note' AND document_id = NEW.id;

    SELECT count(*) INTO lowered
      FROM obligations_adjustments
     WHERE document_kind = 'credit_note' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF sale_method IS NOT NULL THEN
            RAISE EXCEPTION
                'credit note % says it moved no money, but its sale % names payment method %',
                NEW.id, NEW.sale_id, sale_method;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'credit note % says it moved no money but wrote % ledger entr(ies)',
                NEW.id, moved;
        END IF;
        IF lowered <> 1 THEN
            RAISE EXCEPTION
                'credit note % moves no money, so it must lower exactly one obligation; % found',
                NEW.id, lowered;
        END IF;
    ELSE
        IF moved = 0 THEN
            RAISE EXCEPTION
                'credit note % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
        IF lowered <> 0 THEN
            RAISE EXCEPTION
                'credit note % names payment method %: money went back, so it lowers no debt',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_claim_their_effect ON credit_notes;
CREATE CONSTRAINT TRIGGER credit_notes_claim_their_effect
AFTER INSERT ON credit_notes
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION credit_note_moves_money_or_lowers_debt();

-- ── The book refuses a destination that is not a wallet ───────────────────
--
-- `S1` read backwards: since no sale on account writes a ledger entry, nothing in the book
-- may point at a destination that holds nothing. `0017`'s `money_entry_resolves_account`
-- already refuses a row whose method resolves to no account; this refuses the other shape,
-- a method that **does** resolve but to an account that is not a wallet.
--
-- **`credit` is gone from the schema entirely** (`S7` of `v4-settlement.md`, and the seed is
-- removed from `0012` rather than deleted by a migration — the owner's decision, because
-- nothing has shipped). What remains here is the general rule that made its removal safe,
-- stated without naming a row that no longer exists: debt is a document, not a balance, so a
-- ledger row can never be the record of somebody owing something.
--
-- The rule is expressed as the closed set `accounts.kind` already declares (`0017`: `cash` or
-- `digital`). A future account kind that is not a wallet would have to be added to that CHECK
-- first, which is the review this repository wants for that change.
CREATE OR REPLACE FUNCTION money_entry_names_a_real_account() RETURNS trigger AS $$
DECLARE
    kind TEXT;
BEGIN
    SELECT a.kind INTO kind
      FROM payment_methods pm JOIN accounts a ON a.id = pm.account_id
     WHERE pm.id = NEW.payment_method_id;

    IF kind IS NULL OR kind NOT IN ('cash', 'digital') THEN
        RAISE EXCEPTION
            'money entry % names a method on account kind %, which is not a wallet: debt is a document (obligations, payables), not a balance',
            NEW.id, kind;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS money_entries_never_name_the_debt ON money_entries;
DROP TRIGGER IF EXISTS money_entries_name_a_real_account ON money_entries;
CREATE TRIGGER money_entries_name_a_real_account
BEFORE INSERT ON money_entries
FOR EACH ROW EXECUTE FUNCTION money_entry_names_a_real_account();

-- ── No backfill, and that is a decision rather than an omission ──────────
--
-- **This migration does not migrate pre-existing credit sales, and the owner decided why:**
-- the application has never shipped, there is no production database, and the only databases
-- that exist are the owner's own test data, droppable on request.
--
-- The first version of this file tried to migrate them anyway, and it was **broken in two
-- ways at once**, measured on a database at `0020` holding one legacy credit sale:
--
--   1. `UPDATE sales SET payment_method_id = NULL` collided with `0013`'s
--      `guard_sale_economics`, which refuses **any** change to that column:
--      *"sale … economics are immutable: a correction is a credit note, never an edit"*.
--      The migration aborted before reaching the check below.
--   2. Even with the guard relaxed, the ledger rows it leaves behind cannot be removed —
--      `money_entries` is append-only — so the check at the end would refuse the whole
--      migration anyway, asking a human to decide the fate of rows no route can delete.
--
-- An empty database never reaches either failure, which is why 119 tests and every CI run
-- passed: **they all start from nothing.** That is `v4-settlement.md` V1b's own warning —
-- "doing it on an empty database would have proven nothing" — and this slice ignored it.
--
-- **Removing the backfill is the honest fix, not a shortcut.** A backfill carries live data
-- across a schema change; there is none. Keeping a broken one and disabling the check that
-- exposes it would leave two generations of credit history in any database someone did keep.
-- What replaces it is a refusal naming the count and the remedy — a `DROP`, the owner's
-- choice. Re-measured: an empty database applies in 17ms, and one holding a legacy credit
-- sale refuses once, by design.

-- ── Prove the guards exist, and that no legacy data is in the way ─────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
    legacy  bigint;
    unclaimed bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('obligations', 'obligations_append_only'),
              ('obligations', 'obligations_no_truncate'),
              ('obligations', 'obligations_match_their_sale'),
              ('obligations_adjustments', 'obligations_adjustments_append_only'),
              ('obligations_adjustments', 'obligations_adjustments_no_truncate'),
              ('obligations_adjustments', 'obligations_adjustments_ceiling'),
              ('sales', 'sales_payment_method_claimed'),
              ('credit_notes', 'credit_notes_claim_their_effect'),
              ('money_entries', 'money_entries_name_a_real_account')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 9 THEN
        RAISE EXCEPTION
            'expected 9 v4-receivables guards, found %; refusing to continue', present;
    END IF;

    -- The columns really are nullable, which no trigger would reveal.
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'sales' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'sales.payment_method_id is still NOT NULL; refusing to continue';
    END IF;
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'credit_notes' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'credit_notes.payment_method_id is still NOT NULL; refusing to continue';
    END IF;

    -- **No legacy data may be in the way, and the remedy is a DROP** — the owner's
    -- decision: nothing has shipped, so no database holds history worth carrying.
    --
    -- The check runs against **any** ledger row whose method resolves to no account, which
    -- is the shape the retired `credit` account used to have: a book row pointing at a
    -- destination that is not a wallet. With `credit` removed from the initial schema (`S7`
    -- of `v4-settlement.md`, and the seed is gone from `0012`) this can only be violated by
    -- rows written before that rewrite, which is exactly the case the refusal exists for.
    SELECT count(*) INTO legacy
      FROM money_entries me
      LEFT JOIN payment_methods pm ON pm.id = me.payment_method_id
      LEFT JOIN accounts a         ON a.id = pm.account_id
     WHERE a.id IS NULL;
    IF legacy > 0 THEN
        RAISE EXCEPTION
            '% money_entries row(s) name a payment method that resolves to no account. This migration does not migrate them: the application has never shipped, so the remedy is to drop the development database and start from empty ("docker compose down -v").',
            legacy;
    END IF;

    -- **Both halves of the two-state rule, because the comment promised both** (`R2-002`
    -- of the review: the first version checked only the obligation half, so a sale with a
    -- non-NULL method and no ledger entry — or a NULL sale with an entry elsewhere — would
    -- pass here and be refused at its next write).
    SELECT count(*) INTO unclaimed
      FROM sales s
     WHERE (s.payment_method_id IS NULL)
        <> (EXISTS (SELECT 1 FROM obligations o
                     WHERE o.source_document_kind = 'sale' AND o.source_document_id = s.id))
        OR (s.payment_method_id IS NULL AND EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'sale' AND me.document_id = s.id))
        OR (s.payment_method_id IS NOT NULL AND NOT EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'sale' AND me.document_id = s.id));
    IF unclaimed > 0 THEN
        RAISE EXCEPTION
            '% sale(s) disagree with their obligations or their ledger entries about whether money moved; refusing to continue',
            unclaimed;
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0022_payments_settlements.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S6a: a collection is a document, and settlement is a table.
--
-- `odd/tasks/v4-receivables.md` S4-S6. `0021` gave a sale that moved no money a document
-- to owe with. This is the other half: the money arriving, and the rows that say which
-- debt it paid.
--
-- **Why `settlements` is a table and not a column on either side.** Two facts force it,
-- both of them the owner's own words (`v4-settlement.md` L18):
--
--   * "un cobro puede pagar varias obligaciones" — one payment covers several debts;
--   * "una venta/compra puede quedar parcialmente pagada" — one debt is paid in parts.
--
-- A column on `obligations` could express the second and not the first; a column on
-- `payments` the reverse. A table expresses both, and the two ceilings are then sums over
-- rows instead of comparisons of stored fields.
--
-- **Partial is not a state.** It is `SUM(settlements) < amount`. There is no `status`
-- column here, exactly as there is none on `sales` since `0015`: a stored state that is a
-- subtraction of other rows can disagree with them.

-- ── payments: money arriving, or leaving ───────────────────────────────────
--
-- `S4`, and `S7` of `v4-settlement.md`. `in` is a customer paying us; `out` is us paying a
-- supplier or refunding a customer's credit balance. **One document with a direction**,
-- not two tables: the difference between a collection and a refund is a sign, and two
-- tables would duplicate every rule about methods, parties and the book.
--
-- `payment_method_id` is `NOT NULL` here and stays that way. A payment **is** money moving,
-- so it always has a rail — that is the opposite of a sale on account, and the asymmetry
-- is the design rather than an oversight.
CREATE TABLE IF NOT EXISTS payments (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    party_kind        TEXT NOT NULL,
    party_id          UUID NOT NULL,
    payment_method_id UUID NOT NULL REFERENCES payment_methods (id),
    total             NUMERIC(14, 6) NOT NULL CHECK (total > 0),
    direction         TEXT NOT NULL,
    occurred_at       TIMESTAMPTZ NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        TEXT NOT NULL,

    CONSTRAINT payments_whole_cents CHECK (total = round(total, 2)),
    CONSTRAINT payments_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT payments_party_known CHECK (party_kind IN ('customer', 'supplier')),
    CONSTRAINT payments_direction_known CHECK (direction IN ('in', 'out')),
    -- A customer pays us; we pay a supplier; and we may refund a customer's credit
    -- balance. The crossed pair that is not a document this schema describes is `in`
    -- **from a supplier**, and leaving it reachable would make the direction a suggestion.
    CONSTRAINT payments_direction_matches_party CHECK (
        (direction = 'in'  AND party_kind = 'customer')
     OR (direction = 'out' AND party_kind = 'supplier')
     OR (direction = 'out' AND party_kind = 'customer')
    )
);

CREATE INDEX IF NOT EXISTS payments_party_idx ON payments (party_kind, party_id);
CREATE INDEX IF NOT EXISTS payments_occurred_at_idx ON payments (occurred_at DESC);

-- ── settlements: which debt this money paid, and how much ──────────────────
--
-- `S5`. Both ceilings are checked by the trigger below, and both are `SUM`s because
-- neither side is limited to one row.
CREATE TABLE IF NOT EXISTS settlements (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    obligation_id UUID NOT NULL REFERENCES obligations (id),
    payment_id    UUID NOT NULL REFERENCES payments (id),
    amount        NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by    TEXT NOT NULL,

    CONSTRAINT settlements_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT settlements_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- **Two rows from the same payment to the same debt are one row written twice.** They
    -- would pass both ceilings while double-counting, and their sum says exactly what a
    -- single row of that sum would say — so there is nothing to express. Measured by
    -- writing one: the first attempt at the multi-debt test tried 40 then 60 against the
    -- same pair, and this index is what corrected the test.
    --
    -- "A debt paid in parts" therefore means across **payments**, which is what
    -- `v4-settlement.md` L18 describes: different collections over time.
    CONSTRAINT settlements_one_row_per_pair UNIQUE (obligation_id, payment_id)
);

CREATE INDEX IF NOT EXISTS settlements_obligation_idx ON settlements (obligation_id);
CREATE INDEX IF NOT EXISTS settlements_payment_idx ON settlements (payment_id);

-- ── The two ceilings, in one function and one place ────────────────────────
--
-- **Both are judged against the committed state plus this row**, and both are stated as
-- the rule they are: the sum of settlements must not exceed the obligation, and must not
-- exceed the payment. The alternative — trusting the handler — is what `0019` exists to
-- correct: a guard one route applies is a guard the next route forgets.
--
-- The message names both numbers because the caller needs to know **which** ceiling it
-- hit: an over-settled debt is a different correction from an over-applied payment.
CREATE OR REPLACE FUNCTION settlement_within_both_ceilings() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
    paid    NUMERIC(14, 6);
    applied NUMERIC(14, 6);
BEGIN
    -- **Both rows are locked, in ONE global order: payment first, then obligation**
    -- (`R1-settlement-ceiling-unlocked` / `R3-004`, and the order is the fix for
    -- `R1-settlement-lock-order-inversion` / `R3-001`).
    --
    -- Without the locks, two concurrent direct-SQL settlements — the same payment against
    -- different obligations, or the same obligation from different payments — each read the same
    -- committed sums, both pass, and commit past a ceiling.
    --
    -- **The order is not free to choose.** The route takes the party's payment rows first (a
    -- superset that includes the row being settled) and the obligation second. This trigger
    -- originally took them obligation-first, which cycled against the route: a route settlement
    -- held the payment rows and waited for the obligation while a direct insert held the
    -- obligation and waited for the payment — a deadlock where the base could not have one.
    -- Payment before obligation, everywhere, is what makes the cycle inexpressible.
    SELECT p.total  INTO paid FROM payments p    WHERE p.id = NEW.payment_id FOR UPDATE;
    SELECT o.amount INTO owed FROM obligations o WHERE o.id = NEW.obligation_id FOR UPDATE;

    IF owed IS NULL OR paid IS NULL THEN
        RAISE EXCEPTION
            'settlement names obligation % and payment %; at least one does not exist',
            NEW.obligation_id, NEW.payment_id;
    END IF;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM settlements WHERE obligation_id = NEW.obligation_id;
    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM obligations_adjustments WHERE obligation_id = NEW.obligation_id;
    SELECT COALESCE(SUM(amount), 0) INTO applied
      FROM settlements WHERE payment_id = NEW.payment_id;

    -- **The two ceilings compose here, and this is the only place they do:** a credit note
    -- lowers what a payment may still cover, so `settlements + adjustments <= amount`. A
    -- debt cannot be resolved twice — once by a document, once by money.
    IF settled + NEW.amount + lowered > owed THEN
        RAISE EXCEPTION
            'obligation % is for %, with % already lowered by document(s); % + % would settle more than is owed',
            NEW.obligation_id, owed, lowered, settled, NEW.amount;
    END IF;

    IF applied + NEW.amount > paid THEN
        RAISE EXCEPTION
            'payment % is for %, with % already applied; % + % would apply more than was paid',
            NEW.payment_id, paid, applied, applied, NEW.amount;
    END IF;

    -- **Direction, and the party on both sides** (`R3-005`). These two invariants lived only in
    -- `create_settlement`, so a direct SQL insert — or any future route — could settle a debt
    -- with money that left the till, or with another party's money. A ceiling that is only
    -- enforced above the database is a suggestion to everyone who reaches below it.
    IF NOT EXISTS (
        SELECT 1 FROM payments WHERE id = NEW.payment_id AND direction = 'in'
    ) THEN
        RAISE EXCEPTION
            'settlement % names payment % which is not money coming in; money that left cannot settle a debt',
            NEW.id, NEW.payment_id;
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM obligations o
          JOIN payments p ON p.id = NEW.payment_id
         WHERE o.id = NEW.obligation_id
           AND o.party_kind = p.party_kind
           AND o.party_id = p.party_id
    ) THEN
        RAISE EXCEPTION
            'settlement % pays obligation % with payment % belonging to a different party',
            NEW.id, NEW.obligation_id, NEW.payment_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS settlements_within_both_ceilings ON settlements;
CREATE TRIGGER settlements_within_both_ceilings
BEFORE INSERT ON settlements
FOR EACH ROW EXECUTE FUNCTION settlement_within_both_ceilings();

-- ── The adjustment ceiling learns about settlements ───────────────────────
--
-- `0021` cut `obligation_adjustments_ceiling` to compare against `obligations.amount`
-- alone, and said so: the combined ceiling is the rule and the two halves cannot be judged
-- apart. This is the other half arriving.
--
-- **Re-cut rather than duplicated.** A second trigger checking the joined total would
-- answer whichever message happened to fire first, which is the ambiguity `0012` records
-- about checks placed in the wrong order.
CREATE OR REPLACE FUNCTION obligation_adjustments_within_amount() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
BEGIN
    -- **`FOR UPDATE`, and this re-cut DROPPED it** (`R1-adjustment-ceiling-lock-dropped` /
    -- `R3-004`). `0021` added the lock to this very function and called it "the whole
    -- guarantee"; `CREATE OR REPLACE` replaced the body without it, so over the full chain the
    -- trigger-level protection silently disappeared while both files still read as if they had
    -- it. The HTTP paths take their own locks, which is why no test noticed — the guard that
    -- covers the `psql` surface was the one that vanished.
    --
    -- A function redefinition is not additive: everything the previous body guaranteed has to
    -- be written again. That is the cost of `CREATE OR REPLACE`, and it is worth stating in the
    -- file that paid it.
    SELECT amount INTO owed FROM obligations WHERE id = NEW.obligation_id FOR UPDATE;

    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM obligations_adjustments
     WHERE obligation_id = NEW.obligation_id AND id <> NEW.id;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM settlements WHERE obligation_id = NEW.obligation_id;

    IF lowered + settled + NEW.amount > owed THEN
        RAISE EXCEPTION
            'obligation % is for %, with % already settled; % + % would lower it past zero',
            NEW.obligation_id, owed, settled, lowered, NEW.amount;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── Append-only, the same three guards every book keeps ────────────────────
CREATE OR REPLACE FUNCTION reject_payment_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payments_append_only ON payments;
CREATE TRIGGER payments_append_only
BEFORE UPDATE OR DELETE ON payments
FOR EACH ROW EXECUTE FUNCTION reject_payment_mutation();

DROP TRIGGER IF EXISTS payments_no_truncate ON payments;
CREATE TRIGGER payments_no_truncate
BEFORE TRUNCATE ON payments
FOR EACH STATEMENT EXECUTE FUNCTION reject_payment_mutation();

DROP TRIGGER IF EXISTS settlements_append_only ON settlements;
CREATE TRIGGER settlements_append_only
BEFORE UPDATE OR DELETE ON settlements
FOR EACH ROW EXECUTE FUNCTION reject_payment_mutation();

DROP TRIGGER IF EXISTS settlements_no_truncate ON settlements;
CREATE TRIGGER settlements_no_truncate
BEFORE TRUNCATE ON settlements
FOR EACH STATEMENT EXECUTE FUNCTION reject_payment_mutation();

-- ── A payment writes its ledger entry, once ───────────────────────────────
--
-- `S4`. Money in is a positive entry, money out negative — `0014`'s convention, unchanged.
-- The entry is written by the route inside the same transaction as the document, exactly
-- as a sale's is; this trigger is the counterpart of `0021`'s claim guard and refuses a
-- payment that wrote none.
--
-- **Nothing here names the debt account, and matter-of-factly:** a collection is money
-- arriving, so it names a real rail. The debt account stopped being writable in `0021` and
-- this does not reopen it.
CREATE OR REPLACE FUNCTION payment_claims_its_entry() RETURNS trigger AS $$
DECLARE
    moved bigint;
BEGIN
    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'payment' AND document_id = NEW.id;

    IF moved <> 1 THEN
        RAISE EXCEPTION
            'payment % wrote % ledger entr(ies); a payment moves money exactly once',
            NEW.id, moved;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payments_claim_their_entry ON payments;
CREATE CONSTRAINT TRIGGER payments_claim_their_entry
AFTER INSERT ON payments
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION payment_claims_its_entry();

-- ── The book learns `payment`, and keeps every kind it already knew ───────
--
-- `0020` extended this closed set, and extending it is an `ALTER` for the reason that file
-- records: an unrecognised kind is a balance change whose cause cannot be named.
--
-- **The list below is the whole set, not the new members.** The first version of this
-- statement wrote the `0017` set plus `payment` and **silently dropped `annulment`**, which
-- turned every annulment into a 500. Two `v4_an_annulment_*` tests caught it on a full
-- `make test`, not on the focused runs. The `DO` block at the end of this file now asserts
-- every kind the migrations before it introduced is still accepted, so the next copy of
-- this `CHECK` cannot fail the same way. **Copying a closed set's members by hand is how
-- the set loses one** — `0020`'s own lesson, relearned here at cost.
ALTER TABLE money_entries DROP CONSTRAINT IF EXISTS money_entries_kind_known;

ALTER TABLE money_entries
    ADD CONSTRAINT money_entries_kind_known CHECK (
        document_kind IN (
            'sale', 'credit_note', 'purchase', 'supplier_return',
            'opening_balance', 'transfer', 'count_adjustment',
            'annulment',
            'payment'
        )
    );

-- ── Prove the guards exist ─────────────────────────────────────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payments', 'payments_append_only'),
              ('payments', 'payments_no_truncate'),
              ('payments', 'payments_claim_their_entry'),
              ('settlements', 'settlements_append_only'),
              ('settlements', 'settlements_no_truncate'),
              ('settlements', 'settlements_within_both_ceilings')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 6 THEN
        RAISE EXCEPTION
            'expected 6 settlement guards, found %; refusing to continue', present;
    END IF;

    -- The book accepts the new kind **and every kind the migrations before this one
    -- introduced**. Asserting only `payment` is what let the first version drop
    -- `annulment`, so the check enumerates the members rather than the newest one.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'money_entries_kind_known'
           AND pg_get_constraintdef(oid) LIKE '%payment%'
           AND pg_get_constraintdef(oid) LIKE '%annulment%'
           AND pg_get_constraintdef(oid) LIKE '%count_adjustment%'
           AND pg_get_constraintdef(oid) LIKE '%opening_balance%'
           AND pg_get_constraintdef(oid) LIKE '%supplier_return%'
    ) THEN
        RAISE EXCEPTION
            'money_entries_kind_known lost a document kind it already knew; refusing to continue';
    END IF;
END;
$$;

-- ═══════════════════════════════════════════════════════════════════════════
-- From 0023_payables.sql
-- ═══════════════════════════════════════════════════════════════════════════

-- v4 S6b: what we owe is a document, and paying a supplier settles it.
--
-- `odd/tasks/v4-payables.md` S1-S13, the mirror of `0021`/`0022` on the purchase side.
--
-- The defect it closes is `v4-settlement.md` L19, and it is `v3-accounts.md` L107 mirrored:
-- `supplier_returns` inherits the purchase's method with no override, and **that inheritance
-- is only true for a purchase that was paid**. Returning goods on an unpaid purchase credits
-- the debt; it does not hand money back. Until this file there was no way to say "we owe the
-- supplier" at all — `purchases.rs` refuses the debt method outright (`S9b` of
-- `v3-accounts.md`: *"there is no purchase-side counterpart to a credit sale until the
-- payable document exists"*).
--
-- ── Why three new tables instead of extending `obligations` ─────────────────
--
-- `v4-settlement.md` S5/S6 describe the supplier side as an extension of `obligations` and
-- `settlements`, and the owner decided otherwise (`v4-settlement.md` L25, `v4-payables.md`
-- L2-L4). **This file therefore contains no statement whose target is an object `0021` or
-- `0022` created**: no `ALTER` over a closed set, no `CREATE OR REPLACE` of either ceiling
-- function. Those two operations are exactly what has already failed twice in this feature —
-- `0022` re-cut `obligation_adjustments_within_amount` and silently dropped the `FOR UPDATE`
-- `0021` had added (`R3-004`, CRITICAL), and its `money_entries_kind_known` rewrite silently
-- dropped `annulment`, which turned every annulment into a 500.
--
-- **What that costs, stated rather than discovered later:** the ceiling below is a second copy
-- of `settlement_within_both_ceilings`. Same rule, two functions, so the two must move
-- together if the rule ever changes. `obligations.party_kind = 'supplier'` stays declared and
-- unused, because removing it would be the rewrite being avoided. `S13` of the feature
-- document makes "no edits to `0021`/`0022`" testable rather than aspirational.

-- ── payables: the debt itself ──────────────────────────────────────────────
--
-- `source_document_kind` is closed to `purchase`, deliberately and by the same argument
-- `0021` gives for `sale`: an unrecognised source is a debt whose cause cannot be named.
--
-- **No foreign key on `source_document_id`**, for `0021`'s reason: it is polymorphic by
-- design, PostgreSQL has none, and the trigger below closes the gap by deriving the party
-- from the document — the technique `0011` established for `stock_movements`.
--
-- **`amount >= 0`, not `> 0`.** `0009` allows `total = 0` (free stock in, a bonus line), and
-- `0017` learned this the expensive way: `CHECK (amount <> 0)` on the book turned a valid
-- bonus sale into a 500. A zero payable is a real payable, settled the moment it exists.
CREATE TABLE IF NOT EXISTS payables (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_document_kind TEXT NOT NULL,
    source_document_id   UUID NOT NULL,
    party_kind           TEXT NOT NULL,
    party_id             UUID NOT NULL,
    amount               NUMERIC(14, 6) NOT NULL CHECK (amount >= 0),
    occurred_at          TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by           TEXT NOT NULL,

    CONSTRAINT payables_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payables_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT payables_source_known CHECK (source_document_kind IN ('purchase')),
    CONSTRAINT payables_party_known CHECK (party_kind IN ('supplier'))
);

-- **One debt per document, and the UNIQUE is the rule.** A second payable naming the same
-- purchase would make "what is owed" a sum of two rows nobody can tell apart.
CREATE UNIQUE INDEX IF NOT EXISTS payables_source_unique
    ON payables (source_document_kind, source_document_id);

CREATE INDEX IF NOT EXISTS payables_party_idx
    ON payables (party_kind, party_id);

-- ── payables_adjustments: the debt shrinking ───────────────────────────────
--
-- `S5`. A supplier return that inherits the purchase's NULL method moves no money and
-- lowers what is owed.
--
-- **The alternative was a mutable `payables.amount` behind a `tpox.*_writer` flag** — the
-- pattern `0019` used for `sale_items.reversed_quantity`, which cost the CRITICAL finding
-- `R3-002` in the review of `4ffb95e` (`v4-settlement.md` V7). The honest shape exists here,
-- so the flag is not bought a second time: the debt drops through a row.
--
-- So the outstanding figure is
--
--     payables.amount - SUM(payable_settlements) - SUM(payables_adjustments)
--
-- and a partial drop is the normal case rather than a special one.
CREATE TABLE IF NOT EXISTS payables_adjustments (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payable_id    UUID NOT NULL REFERENCES payables (id),
    -- Positive, with the sign carried by the table: an adjustment always lowers the debt, and
    -- a column that sometimes adds is a column every reader must re-derive.
    amount        NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    document_kind TEXT NOT NULL,
    document_id   UUID NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by    TEXT NOT NULL,

    CONSTRAINT payables_adjustments_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payables_adjustments_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- `cancellation` is accepted from the start and written by nothing yet: it is the member
    -- `v4-settlement.md` S8 names for the credit-note path's mirror, and accepting it costs one
    -- word while adding it later costs an `ALTER` over a closed set — the operation this file
    -- exists to avoid.
    CONSTRAINT payables_adjustments_kind_known
        CHECK (document_kind IN ('supplier_return', 'cancellation'))
);

-- One adjustment per document: a return lowers the debt exactly once, and a second row naming
-- the same return would be the same correction counted twice.
CREATE UNIQUE INDEX IF NOT EXISTS payables_adjustments_document_unique
    ON payables_adjustments (document_kind, document_id);

CREATE INDEX IF NOT EXISTS payables_adjustments_payable_idx
    ON payables_adjustments (payable_id);

-- ── payable_settlements: which debt this money paid, and how much ──────────
--
-- `S6`. Both ceilings are sums over rows because neither side is limited to one row, exactly
-- as `0022` argues for the customer side (`v4-settlement.md` L18, the owner's own words).
CREATE TABLE IF NOT EXISTS payable_settlements (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payable_id UUID NOT NULL REFERENCES payables (id),
    payment_id UUID NOT NULL REFERENCES payments (id),
    amount     NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL,

    CONSTRAINT payable_settlements_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payable_settlements_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- **Two rows from the same payment to the same debt are one row written twice.** They
    -- would pass both ceilings while double-counting, and their sum says exactly what a single
    -- row of that sum would say. `0022` learned this by writing one and being refused, and the
    -- test that did it was wrong rather than the index. "Paid in parts" means across
    -- **payments**.
    CONSTRAINT payable_settlements_one_row_per_pair UNIQUE (payable_id, payment_id)
);

CREATE INDEX IF NOT EXISTS payable_settlements_payable_idx
    ON payable_settlements (payable_id);
CREATE INDEX IF NOT EXISTS payable_settlements_payment_idx
    ON payable_settlements (payment_id);

-- ── The two ceilings, in one function and one place ────────────────────────
--
-- The mirror of `settlement_within_both_ceilings`, written once with both terms present.
-- **That is the whole argument for shipping the three tables in one file** (`S12`): `0022`
-- re-cut its equivalent to add the second term and lost a clause doing it.
--
-- The message names both numbers because the caller needs to know **which** ceiling it hit:
-- an over-settled debt is a different correction from an over-applied payment.
CREATE OR REPLACE FUNCTION payable_settlement_within_both_ceilings() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
    paid    NUMERIC(14, 6);
    applied NUMERIC(14, 6);
BEGIN
    -- **Both rows are locked, in ONE order: payment first, then payable.** This is the order
    -- `aa95665` established after a correction inverted it and created the deadlock it was
    -- closing (`R1-settlement-lock-order-inversion` / `R3-001` of
    -- `review-52d3b2c6c4f09589`). The order is not free to choose; it is whatever every writer
    -- agrees on, and the payable settlement route takes them the same way round.
    SELECT p.total  INTO paid FROM payments p WHERE p.id = NEW.payment_id FOR UPDATE;
    SELECT o.amount INTO owed FROM payables o  WHERE o.id = NEW.payable_id FOR UPDATE;

    IF owed IS NULL OR paid IS NULL THEN
        RAISE EXCEPTION
            'payable settlement names payable % and payment %; at least one does not exist',
            NEW.payable_id, NEW.payment_id;
    END IF;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM payable_settlements WHERE payable_id = NEW.payable_id;
    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM payables_adjustments WHERE payable_id = NEW.payable_id;
    SELECT COALESCE(SUM(amount), 0) INTO applied
      FROM payable_settlements WHERE payment_id = NEW.payment_id;

    -- **The two ceilings compose here, and this is the only place they do:** a return lowers
    -- what a payment may still cover, so `settlements + adjustments <= amount`. A debt cannot
    -- be resolved twice — once by a document, once by money.
    IF settled + NEW.amount + lowered > owed THEN
        RAISE EXCEPTION
            'payable % is for %, with % already lowered by document(s); % + % would settle more than is owed',
            NEW.payable_id, owed, lowered, settled, NEW.amount;
    END IF;

    IF applied + NEW.amount > paid THEN
        RAISE EXCEPTION
            'payment % is for %, with % already applied; % + % would apply more than was paid',
            NEW.payment_id, paid, applied, applied, NEW.amount;
    END IF;

    -- **Direction and party, below the route as well as above it** (`R3-005` of
    -- `review-b40b15076ff2c498`). A ceiling enforced only above the database is a suggestion to
    -- everyone who reaches below it. A supplier payable is settled by money that WENT OUT, to
    -- that same supplier — the exact opposite of the customer side, where the money must come
    -- in.
    IF NOT EXISTS (
        SELECT 1 FROM payments WHERE id = NEW.payment_id AND direction = 'out'
    ) THEN
        RAISE EXCEPTION
            'payable settlement % names payment % which is not money going out; only money we sent can pay what we owe',
            NEW.id, NEW.payment_id;
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM payables o
          JOIN payments p ON p.id = NEW.payment_id
         WHERE o.id = NEW.payable_id
           AND o.party_kind = p.party_kind
           AND o.party_id = p.party_id
    ) THEN
        RAISE EXCEPTION
            'payable settlement % pays payable % with payment % belonging to a different party',
            NEW.id, NEW.payable_id, NEW.payment_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payable_settlements_within_both_ceilings ON payable_settlements;
CREATE TRIGGER payable_settlements_within_both_ceilings
BEFORE INSERT ON payable_settlements
FOR EACH ROW EXECUTE FUNCTION payable_settlement_within_both_ceilings();

-- ── The adjustment ceiling learns about settlements ────────────────────────
--
-- Unlike `0022`, this one is **written once, with both terms**, because there is no earlier
-- body to re-cut. `0021` wrote the customer version against `amount` alone and `0022` replaced
-- it; that whole sequence is what S12 avoids.
CREATE OR REPLACE FUNCTION payables_adjustments_within_amount() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
BEGIN
    -- **`FOR UPDATE` on the payable row, and it is the whole guarantee.** Without it two
    -- concurrent adjustments of the same payable each read the same committed sum, each pass,
    -- and together lower the debt past zero — the defect `0021` measured as
    -- `R1-ceiling-race` / `R4-ceiling-trigger-reads-without-locking`, and the clause `0022`
    -- then lost in a re-cut.
    SELECT amount INTO owed FROM payables WHERE id = NEW.payable_id FOR UPDATE;

    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM payables_adjustments
     WHERE payable_id = NEW.payable_id AND id <> NEW.id;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM payable_settlements WHERE payable_id = NEW.payable_id;

    IF lowered + settled + NEW.amount > owed THEN
        RAISE EXCEPTION
            'payable % is for %, with % already settled; % + % would lower it past zero',
            NEW.payable_id, owed, settled, lowered, NEW.amount;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_adjustments_ceiling ON payables_adjustments;
CREATE TRIGGER payables_adjustments_ceiling
BEFORE INSERT ON payables_adjustments
FOR EACH ROW EXECUTE FUNCTION payables_adjustments_within_amount();

-- ── Append-only, the same three guards every book keeps ────────────────────
--
-- A row-level guard never sees `TRUNCATE`, because it is a set operation and does not fire
-- FOR EACH ROW. `0007` shipped that mistake and `0008` repaired it; both guards go in from
-- the start, as `0017`, `0018`, `0021` and `0022` do.
CREATE OR REPLACE FUNCTION reject_payable_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_append_only ON payables;
CREATE TRIGGER payables_append_only
BEFORE UPDATE OR DELETE ON payables
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_no_truncate ON payables;
CREATE TRIGGER payables_no_truncate
BEFORE TRUNCATE ON payables
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_adjustments_append_only ON payables_adjustments;
CREATE TRIGGER payables_adjustments_append_only
BEFORE UPDATE OR DELETE ON payables_adjustments
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_adjustments_no_truncate ON payables_adjustments;
CREATE TRIGGER payables_adjustments_no_truncate
BEFORE TRUNCATE ON payables_adjustments
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payable_settlements_append_only ON payable_settlements;
CREATE TRIGGER payable_settlements_append_only
BEFORE UPDATE OR DELETE ON payable_settlements
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payable_settlements_no_truncate ON payable_settlements;
CREATE TRIGGER payable_settlements_no_truncate
BEFORE TRUNCATE ON payable_settlements
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

-- ── The purchase learns to say "this moved no money" ───────────────────────
--
-- `S2`. The mirror of `0021`'s `ALTER TABLE sales`, and it carries the same measured trap:
-- `0016` installed `accounts_exactly_one_default` as a deferred constraint trigger, so any
-- row-touching statement on `accounts` queues an event checked at COMMIT and a later `ALTER`
-- in the same transaction aborts with *"cannot ALTER TABLE because it has pending trigger
-- events"*. `purchases` has no such trigger today; **the two `ALTER`s here are therefore
-- placed before the first statement that can touch a `purchases` row**, which in this file is
-- none — the guard is a `DO` block that only reads.
ALTER TABLE purchases ALTER COLUMN payment_method_id DROP NOT NULL;

-- ── The two states of a purchase, checked at commit ────────────────────────
--
-- `S2`:
--
--     payment_method_id IS NULL      ->  exactly one payable names this purchase
--     payment_method_id IS NOT NULL  ->  zero payables, and at least one ledger entry
--
-- **`DEFERRABLE INITIALLY DEFERRED` is not optional**, for `0021`'s reason: the purchase is
-- inserted before the payable that names it (a payable needs the purchase's id) and the
-- ledger entry is written after the purchase is read back, so a plain `AFTER INSERT` would
-- refuse the correct order. Deferring to COMMIT lets one transaction build a coherent set of
-- rows and judge it once.
--
-- `IS DISTINCT FROM` over the pair rather than a chain of `<>`: the guard's job is to keep
-- holding when a future migration relaxes something else.
CREATE OR REPLACE FUNCTION purchase_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    owed  bigint;
    moved bigint;
BEGIN
    SELECT count(*) INTO owed
      FROM payables
     WHERE source_document_kind = 'purchase' AND source_document_id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'purchase' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF owed <> 1 THEN
            RAISE EXCEPTION
                'purchase % says it moved no money, but % payable(s) name it; a purchase on account has exactly one',
                NEW.id, owed;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'purchase % says it moved no money, but % ledger entr(ies) name it',
                NEW.id, moved;
        END IF;
    ELSE
        IF owed <> 0 THEN
            RAISE EXCEPTION
                'purchase % names payment method %, but % payable(s) also name it; money moved or debt arose, never both',
                NEW.id, NEW.payment_method_id, owed;
        END IF;
        IF moved = 0 THEN
            RAISE EXCEPTION
                'purchase % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS purchases_payment_method_claimed ON purchases;
CREATE CONSTRAINT TRIGGER purchases_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON purchases
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION purchase_payment_method_is_claimed();

-- ── The payable can only name a purchase that claims it ────────────────────
--
-- The other half of the pair. Without it the purchase-side trigger could be satisfied by a
-- payable naming a *different* purchase, and the party would be free to disagree with the
-- document: a debt attributed to a supplier who never sold anything.
--
-- `S4`: the party is derived from the document, never trusted from the caller.
CREATE OR REPLACE FUNCTION payable_matches_its_purchase() RETURNS trigger AS $$
DECLARE
    claim_method   UUID;
    purchase_party UUID;
BEGIN
    IF NEW.source_document_kind <> 'purchase' THEN
        -- Closed on purpose: a second source kind arrives with its own branch, the way
        -- `0018` and `0020` extended theirs.
        RAISE EXCEPTION
            'payable source kind % has no rule yet', NEW.source_document_kind;
    END IF;

    SELECT p.payment_method_id, p.supplier_id
      INTO claim_method, purchase_party
      FROM purchases p
     WHERE p.id = NEW.source_document_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'payable names purchase % which does not exist', NEW.source_document_id;
    END IF;

    IF claim_method IS NOT NULL THEN
        RAISE EXCEPTION
            'purchase % names payment method %: money moved, so it owes nothing',
            NEW.source_document_id, claim_method;
    END IF;

    IF NEW.party_kind <> 'supplier' OR NEW.party_id IS DISTINCT FROM purchase_party THEN
        RAISE EXCEPTION
            'payable on purchase % must belong to that purchase''s supplier %, not % %',
            NEW.source_document_id, purchase_party, NEW.party_kind, NEW.party_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_match_their_purchase ON payables;
CREATE CONSTRAINT TRIGGER payables_match_their_purchase
AFTER INSERT ON payables
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION payable_matches_its_purchase();

-- ── A supplier return on an unpaid purchase moves no money ─────────────────
--
-- `S8`, and the sign is the whole point. A return that inherits the purchase's NULL method
-- must write **no** ledger entry and exactly one adjustment; a return that names a method must
-- write a ledger entry and no adjustment. Before this, the inherited path wrote `+total`
-- against the purchase's method — which for an unpaid purchase takes money out of an account
-- that never paid it, `L107`'s shape with the sign reversed.
--
-- **A return may be NULL only when its purchase is NULL.** A return on a paid purchase that
-- moved no money would be money from nowhere, the same rule `0021` states for a credit note.
ALTER TABLE supplier_returns ALTER COLUMN payment_method_id DROP NOT NULL;

CREATE OR REPLACE FUNCTION supplier_return_moves_money_or_lowers_debt()
RETURNS trigger AS $$
DECLARE
    purchase_method UUID;
    moved           bigint;
    lowered         bigint;
BEGIN
    SELECT p.payment_method_id INTO purchase_method
      FROM supplier_returns sr JOIN purchases p ON p.id = sr.purchase_id
     WHERE sr.id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'supplier_return' AND document_id = NEW.id;

    SELECT count(*) INTO lowered
      FROM payables_adjustments
     WHERE document_kind = 'supplier_return' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF purchase_method IS NOT NULL THEN
            RAISE EXCEPTION
                'supplier return % says it moved no money, but its purchase % names payment method %',
                NEW.id, NEW.purchase_id, purchase_method;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'supplier return % says it moved no money but wrote % ledger entr(ies)',
                NEW.id, moved;
        END IF;
        IF lowered <> 1 THEN
            RAISE EXCEPTION
                'supplier return % moves no money, so it must lower exactly one payable; % found',
                NEW.id, lowered;
        END IF;
    ELSE
        IF moved = 0 THEN
            RAISE EXCEPTION
                'supplier return % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
        IF lowered <> 0 THEN
            RAISE EXCEPTION
                'supplier return % names payment method %: money came back, so it lowers no debt',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS supplier_returns_claim_their_effect ON supplier_returns;
CREATE CONSTRAINT TRIGGER supplier_returns_claim_their_effect
AFTER INSERT ON supplier_returns
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION supplier_return_moves_money_or_lowers_debt();

-- ── No backfill, and the proof is a refusal rather than an assumption ──────
--
-- **Purchases never went on account before this file**, so there is no ordinary pre-existing
-- row to migrate: `purchases.rs` refused the debt method outright, and the method it names is
-- `NOT NULL` for every one of them. What could exist is a database someone hand-edited to
-- point a purchase at the debt account — the same shape `0021` refuses for a credit sale, and
-- refused here with the same remedy, because the book is append-only and rewriting it is what
-- this repository will not do.

-- ── Prove the guards exist, and that nothing is in the way ─────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present  bigint;
    legacy   bigint;
    unclaimed bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payables', 'payables_append_only'),
              ('payables', 'payables_no_truncate'),
              ('payables', 'payables_match_their_purchase'),
              ('payables_adjustments', 'payables_adjustments_append_only'),
              ('payables_adjustments', 'payables_adjustments_no_truncate'),
              ('payables_adjustments', 'payables_adjustments_ceiling'),
              ('payable_settlements', 'payable_settlements_append_only'),
              ('payable_settlements', 'payable_settlements_no_truncate'),
              ('payable_settlements', 'payable_settlements_within_both_ceilings'),
              ('purchases', 'purchases_payment_method_claimed'),
              ('supplier_returns', 'supplier_returns_claim_their_effect')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 11 THEN
        RAISE EXCEPTION
            'expected 11 v4-payables guards, found %; refusing to continue', present;
    END IF;

    -- The columns really are nullable, which no trigger would reveal.
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'purchases' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'purchases.payment_method_id is still NOT NULL; refusing to continue';
    END IF;
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'supplier_returns' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION
            'supplier_returns.payment_method_id is still NOT NULL; refusing to continue';
    END IF;

    -- **No legacy data may be in the way, and the remedy is a DROP** — the owner's standing
    -- decision: nothing has shipped, so no database holds history worth carrying.
    SELECT count(*) INTO legacy
      FROM money_entries me
      LEFT JOIN payment_methods pm ON pm.id = me.payment_method_id
      LEFT JOIN accounts a         ON a.id = pm.account_id
     WHERE a.id IS NULL;
    IF legacy > 0 THEN
        RAISE EXCEPTION
            '% money_entries row(s) name a payment method that resolves to no account. This migration does not migrate them: the application has never shipped, so the remedy is to drop the development database and start from empty ("docker compose down -v").',
            legacy;
    END IF;

    -- **Both halves of the two-state rule**, because `0021` shipped a check that asserted only
    -- one and its review named it (`R2-002`): a purchase holding a method with no ledger entry
    -- — or a NULL purchase with an entry — would pass a one-sided check and be refused at its
    -- next write.
    SELECT count(*) INTO unclaimed
      FROM purchases p
     WHERE (p.payment_method_id IS NULL)
        <> (EXISTS (SELECT 1 FROM payables o
                     WHERE o.source_document_kind = 'purchase'
                       AND o.source_document_id = p.id))
        OR (p.payment_method_id IS NULL AND EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'purchase' AND me.document_id = p.id))
        OR (p.payment_method_id IS NOT NULL AND NOT EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'purchase' AND me.document_id = p.id));
    IF unclaimed > 0 THEN
        RAISE EXCEPTION
            '% purchase(s) disagree with their payables or their ledger entries about whether money moved; refusing to continue',
            unclaimed;
    END IF;

    -- **`S13`, asserted rather than promised: nothing in `0021`/`0022` was modified.** The two
    -- functions this file *could* have re-cut are byte-identical to what those migrations left,
    -- which is what makes "a new table cannot lose a clause it never had" a property instead of
    -- a hope. A future session that reaches for the customer ceiling from here fails this check
    -- and has to say so.
    IF (SELECT prosrc FROM pg_proc WHERE proname = 'settlement_within_both_ceilings')
       NOT LIKE '%money that left cannot settle a debt%' THEN
        RAISE EXCEPTION
            'settlement_within_both_ceilings was modified; this migration must not touch the customer ceiling';
    END IF;
    IF (SELECT prosrc FROM pg_proc WHERE proname = 'obligation_adjustments_within_amount')
       NOT LIKE '%would lower it past zero%' THEN
        RAISE EXCEPTION
            'obligation_adjustments_within_amount was modified; this migration must not touch the customer ceiling';
    END IF;
END;
$$;
