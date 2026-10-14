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
