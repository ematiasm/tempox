-- The claim triggers are re-created, because editing an applied migration is silent.
--
-- **`R3-001` of `review-1e9c4c74ced6d94b`, CRITICAL.** `0004` was edited twice after it had
-- been applied, and `sqlx` keys by version: a database that had recorded version 4 kept the
-- **old trigger body** with no checksum complaint, no re-run, and exit 0 -- while that body
-- refused every advance against an owed expense. Measured: `_sqlx_migrations` held 1..5 both
-- times, the trigger existed with the old body, and the second `migrate run` exited 0.
--
-- `CREATE OR REPLACE FUNCTION` plus a `DROP`/`CREATE TRIGGER` pair is idempotent, so this is
-- a no-op on a current database and a repair on a stale one. The `DO` block asserts the
-- function **bodies**, because a trigger with the right name and the wrong body is exactly
-- what a stale database holds.
--
-- The rule this file exists to make unnecessary: **an applied migration is edited only when
-- nothing has ever run it.**

-- ── The document side: an expense claims its payments ─────────────────────
CREATE OR REPLACE FUNCTION expense_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    paid    bigint;
    settled NUMERIC(14, 6);
BEGIN
    SELECT count(*) INTO paid
      FROM payments
     WHERE party_kind = 'expense' AND party_id = NEW.id;

    -- An owed expense may take advances later: judged on the payments side below.
    IF NEW.payment_method_id IS NULL THEN
        RETURN NEW;
    END IF;

    IF paid = 0 THEN
        RAISE EXCEPTION
            'expense % names payment method % but no payment answers it',
            NEW.id, NEW.payment_method_id;
    END IF;

    SELECT COALESCE(SUM(total), 0) INTO settled
      FROM payments
     WHERE party_kind = 'expense' AND party_id = NEW.id;

    IF settled > NEW.amount THEN
        RAISE EXCEPTION
            'expense % is for %, but % was paid against it',
            NEW.id, NEW.amount, settled;
    END IF;

    IF EXISTS (
        SELECT 1 FROM payments
         WHERE party_kind = 'expense' AND party_id = NEW.id
           AND direction <> 'out'
    ) THEN
        RAISE EXCEPTION
            'expense % has a payment that is not money going out', NEW.id;
    END IF;

    IF EXISTS (
        SELECT 1 FROM payments
         WHERE party_kind = 'expense' AND party_id = NEW.id
           AND payment_method_id IS DISTINCT FROM NEW.payment_method_id
    ) THEN
        RAISE EXCEPTION
            'expense % says its money moved through method %, but a payment answering it names another rail',
            NEW.id, NEW.payment_method_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── The payment side: every arrival is judged against its document ────────
CREATE OR REPLACE FUNCTION expense_payment_agrees_with_its_document() RETURNS trigger AS $$
DECLARE
    doc_method UUID;
    doc_amount NUMERIC(14, 6);
    settled    NUMERIC(14, 6);
BEGIN
    IF NEW.party_kind <> 'expense' THEN
        RETURN NEW;
    END IF;

    SELECT payment_method_id, amount INTO doc_method, doc_amount
      FROM expenses WHERE id = NEW.party_id;

    IF doc_amount IS NULL THEN
        RAISE EXCEPTION
            'payment % names expense %, which does not exist', NEW.id, NEW.party_id;
    END IF;

    -- A null-method document is the one that CAN take these payments: each advance names its
    -- own rail because the document deliberately fixed none.
    IF doc_method IS NOT NULL
       AND NEW.payment_method_id IS DISTINCT FROM doc_method THEN
        RAISE EXCEPTION
            'payment % moved money through method %, but expense % says method %',
            NEW.id, NEW.payment_method_id, NEW.party_id, doc_method;
    END IF;

    IF NEW.direction <> 'out' THEN
        RAISE EXCEPTION
            'payment % answers expense % and is not money going out', NEW.id, NEW.party_id;
    END IF;

    SELECT COALESCE(SUM(total), 0) INTO settled
      FROM payments
     WHERE party_kind = 'expense' AND party_id = NEW.party_id;

    IF settled > doc_amount THEN
        RAISE EXCEPTION
            'expense % is for %, but % was paid against it',
            NEW.party_id, doc_amount, settled;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- A constraint trigger has no `CREATE OR REPLACE`.
DROP TRIGGER IF EXISTS expenses_payment_method_claimed ON expenses;
CREATE CONSTRAINT TRIGGER expenses_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON expenses
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION expense_payment_method_is_claimed();

DROP TRIGGER IF EXISTS payments_answer_their_expense ON payments;
CREATE CONSTRAINT TRIGGER payments_answer_their_expense
AFTER INSERT ON payments
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION expense_payment_agrees_with_its_document();

-- ── Assert the bodies, not just the names ────────────────────────────────
DO $$
DECLARE
    document_side text;
    payment_side  text;
BEGIN
    SELECT prosrc INTO document_side FROM pg_proc
     WHERE proname = 'expense_payment_method_is_claimed';
    SELECT prosrc INTO payment_side FROM pg_proc
     WHERE proname = 'expense_payment_agrees_with_its_document';

    IF document_side IS NULL OR document_side NOT LIKE '%names another rail%' THEN
        RAISE EXCEPTION
            'expense_payment_method_is_claimed lacks the same-rail rule; refusing to continue';
    END IF;

    IF payment_side IS NULL OR payment_side NOT LIKE '%says method %' THEN
        RAISE EXCEPTION
            'expense_payment_agrees_with_its_document lacks the same-rail rule; refusing to continue';
    END IF;

    -- The null-method split, in both directions.
    IF document_side NOT LIKE '%IS NULL THEN%RETURN NEW%' THEN
        RAISE EXCEPTION
            'the document side no longer lets an owed expense exist; refusing to continue';
    END IF;
    IF payment_side NOT LIKE '%doc_method IS NOT NULL%' THEN
        RAISE EXCEPTION
            'the payment side would refuse every advance against an owed expense; refusing to continue';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'payments_answer_their_expense'
           AND tgrelid = 'payments'::regclass
           AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION
            'payments_answer_their_expense is missing after re-creation; refusing to continue';
    END IF;
END;
$$;
