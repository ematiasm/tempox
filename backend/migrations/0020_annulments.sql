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
