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
