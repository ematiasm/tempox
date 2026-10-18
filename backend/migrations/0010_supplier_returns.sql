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
