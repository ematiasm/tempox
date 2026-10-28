-- v4-expenses E1-T2: an expense is money leaving that touches no stock.
--
-- `odd/tasks/v4-expenses.md` S2-S6. `0018` opened the family of documents that move money
-- without touching stock — opening balance, transfer, count adjustment — and left it pointed
-- at three one-off events. This is the first **recurring, everyday** member: a utility bill,
-- a repair, a salary. It has a category (S1, `0002`), a payee in text, and an amount.
--
-- ## What this migration deliberately does NOT do
--
-- **It does not teach `money_entries` a new kind, and that is the strongest evidence the
-- design is right** (`S5`). An expense's money moves through the `payments` row the route
-- writes, and `payments_claim_their_entry` already guarantees exactly one `'payment'` entry
-- per payment. An `'expense'` entry would record the same coin twice.
--
-- The first draft of the specification did exactly that, and it is recorded rather than
-- quietly corrected because it is the class this repository keeps paying for: two records of
-- one movement. The consequence is large and good — `money_entries_kind_known` is not
-- re-cut by this file, which is the operation that lost `annulment` in `0020`.
--
-- ## Where the debt lives
--
-- **An owed expense is a party to its own money** (`D3`), not a third debt table: several
-- advances against one salary are several `payments` rows naming the expense. `payables` would
-- have cost a third ceiling function, and `0023` records what re-cutting a money-guarding
-- function has already cost. The price — one payment answers one expense (`D4`) — is stated
-- in the document rather than left to be discovered.

CREATE TABLE IF NOT EXISTS expenses (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    category_id       UUID NOT NULL REFERENCES expense_categories (id),
    -- **The payee is text, with no foreign key** (`S2`). The payees this document needs are
    -- mostly not suppliers: "Edenor", "Ferretería X", a person's name. Forcing them into
    -- `suppliers` would fill a purchase table with rows that never sell anything and can
    -- never be deleted, and it would make "Sueldos" a supplier.
    payee             TEXT NOT NULL,
    amount            NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    -- **The day the expense belongs to, which is not necessarily the day it is paid.** This
    -- is what makes an owed expense worth recording: October's salary is entered in October
    -- and settles in November, and a later report can say both. Absent means now, exactly as
    -- `opening_balances.opened_at` does it.
    --
    -- It is an instant and not a period, which is the open question `D5d` hands to the staff
    -- slice: "one salary per employee per month" needs a period, and this column does not
    -- provide one.
    occurred_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Nullable, and **the null is a claim held at COMMIT** by the trigger below — exactly as
    -- `sales` and `purchases` do it since `0021`. A nullable column with no rule is an
    -- arbitrary null.
    payment_method_id UUID REFERENCES payment_methods (id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by        TEXT NOT NULL,

    CONSTRAINT expenses_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT expenses_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT expenses_payee_not_blank CHECK (btrim(payee) <> '')
);

CREATE INDEX IF NOT EXISTS expenses_occurred_at_idx ON expenses (occurred_at DESC);
CREATE INDEX IF NOT EXISTS expenses_category_id_idx ON expenses (category_id);

-- ── Append-only, the same three guards every document keeps ────────────────
--
-- An expense is a fact. Correcting one is a compensating document, never an edit — and the
-- `TRUNCATE` half is not pedantry: a row-level guard never sees it, because it is a set
-- operation. `0004` shipped that mistake and `0008` repaired it.
CREATE OR REPLACE FUNCTION reject_expense_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        'expenses are append-only: a correction is a compensating document, not an edit';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS expenses_append_only ON expenses;
CREATE TRIGGER expenses_append_only
BEFORE UPDATE OR DELETE ON expenses
FOR EACH ROW EXECUTE FUNCTION reject_expense_mutation();

DROP TRIGGER IF EXISTS expenses_no_truncate ON expenses;
CREATE TRIGGER expenses_no_truncate
BEFORE TRUNCATE ON expenses
FOR EACH STATEMENT EXECUTE FUNCTION reject_expense_mutation();

-- ── `payments` learns a third party, and the direction rule is extended ────
--
-- `S6`. The money direction for an expense is **`out` and only `out`**: money never arrives
-- at an expense, and admitting `in` would make the direction a suggestion — the argument
-- `0022` makes for `in`-from-a-supplier.
--
-- **Both closed sets are re-cut by their whole member list, never by adding one.** `0020`
-- copied `money_entries_kind_known`'s members by hand and silently dropped `annulment`,
-- turning every annulment into a 500. The `DO` block below asserts every member survived,
-- which is that lesson made mechanical.
ALTER TABLE payments DROP CONSTRAINT IF EXISTS payments_party_known;
ALTER TABLE payments
    ADD CONSTRAINT payments_party_known
    CHECK (party_kind IN ('customer', 'supplier', 'expense'));

ALTER TABLE payments DROP CONSTRAINT IF EXISTS payments_direction_matches_party;
ALTER TABLE payments
    ADD CONSTRAINT payments_direction_matches_party CHECK (
        (direction = 'in'  AND party_kind = 'customer')
     OR (direction = 'out' AND party_kind = 'supplier')
     OR (direction = 'out' AND party_kind = 'customer')
     OR (direction = 'out' AND party_kind = 'expense')
    );

-- ── The two states of an expense, judged at COMMIT ─────────────────────────
--
-- `S4`:
--
--     payment_method_id IS NOT NULL  ->  at least one `payments` row for this expense,
--                                        `out`, summed within `amount`
--     payment_method_id IS NULL      ->  no `payments` row for this expense yet
--
-- **`DEFERRABLE INITIALLY DEFERRED` is not optional.** The payment is written after the
-- expense row exists (it needs the expense's id), so a plain `AFTER INSERT` would refuse the
-- correct order. Deferring to COMMIT is what lets one transaction build a coherent set and
-- judge it once — the same mechanism `0021` used for `sales`, carrying the same trap: **no
-- `ALTER TABLE` on `expenses` may follow a row-touching statement in the same transaction**,
-- because a pending trigger event blocks it.
--
-- **The null case is judged at the moment the document is created, and that is why deferral
-- is right rather than clever.** A later advance (E1-T3) adds payments to an expense whose
-- claim was the null one; that is a second document making its own statement, not a
-- contradiction of the first. This trigger therefore only refuses a null expense that somehow
-- carries a payment **in the same transaction that created it** — which nothing can do, and
-- which is the point: the rule is stated where the document is born.
CREATE OR REPLACE FUNCTION expense_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    paid    bigint;
    settled NUMERIC(14, 6);
BEGIN
    SELECT count(*) INTO paid
      FROM payments
     WHERE party_kind = 'expense' AND party_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF paid <> 0 THEN
            RAISE EXCEPTION
                'expense % says it moved no money, but % payment(s) name it',
                NEW.id, paid;
        END IF;
        RETURN NEW;
    END IF;

    IF paid = 0 THEN
        RAISE EXCEPTION
            'expense % names payment method % but no payment answers it',
            NEW.id, NEW.payment_method_id;
    END IF;

    -- **The payments must name the rail the document named, and must not exceed it.** A
    -- document that says "the bank" and payments that say "the till" is the desync `0017`
    -- exists to remove, and a total paid above the declared amount is money from nowhere.
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

    -- **`R3-001` of `review-1abfbd892fed09cb`, CRITICAL, and it was reachable.** The first
    -- version checked presence, sum and direction but never that the payment names the
    -- **same rail** the document does. Measured on a migrated database: an expense naming
    -- the bank with a payment naming the till committed happily, so the document and its
    -- money disagreed about where it went — the desync `0017` exists to remove, reopened
    -- one table over.
    --
    -- The comparison is the whole rule: the document's method is the claim, and every
    -- payment answering it must be on that rail, or the claim is false.
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

DROP TRIGGER IF EXISTS expenses_payment_method_claimed ON expenses;
CREATE CONSTRAINT TRIGGER expenses_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON expenses
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION expense_payment_method_is_claimed();

-- ── Prove the guards and the closed sets ──────────────────────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
    allowed text;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('expenses', 'expenses_append_only'),
              ('expenses', 'expenses_no_truncate'),
              ('expenses', 'expenses_payment_method_claimed')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 3 THEN
        RAISE EXCEPTION
            'expected 3 expenses guards, found %; refusing to continue', present;
    END IF;

    -- **Every member of each closed set is still there.** `0020` lost one doing exactly this,
    -- so the membership is asserted rather than assumed. `pg_get_constraintdef` is read
    -- rather than a list copied into this file: the constraint is the authority, and a
    -- second copy of its members is how the first one goes stale.
    SELECT pg_get_constraintdef(oid) INTO allowed
      FROM pg_constraint WHERE conname = 'payments_party_known';
    IF allowed NOT LIKE '%customer%' OR allowed NOT LIKE '%supplier%'
       OR allowed NOT LIKE '%expense%' THEN
        RAISE EXCEPTION
            'payments_party_known lost a member: %', allowed;
    END IF;

    SELECT pg_get_constraintdef(oid) INTO allowed
      FROM pg_constraint WHERE conname = 'payments_direction_matches_party';
    IF allowed NOT LIKE '%in%' OR allowed NOT LIKE '%out%'
       OR allowed NOT LIKE '%expense%' THEN
        RAISE EXCEPTION
            'payments_direction_matches_party lost a member: %', allowed;
    END IF;

    -- The book learns **no** new kind, asserted instead of promised (`S5`). A future session
    -- that reaches for an `'expense'` entry from here fails this check and has to say so.
    SELECT pg_get_constraintdef(oid) INTO allowed
      FROM pg_constraint WHERE conname = 'money_entries_kind_known';
    IF allowed LIKE '%expense%' THEN
        RAISE EXCEPTION
            'money_entries_kind_known gained an expense kind: the money already moved in its payment, and two records of one movement is the defect 0017 killed';
    END IF;
    IF allowed NOT LIKE '%annulment%' OR allowed NOT LIKE '%payment%'
       OR allowed NOT LIKE '%opening_balance%' OR allowed NOT LIKE '%count_adjustment%' THEN
        RAISE EXCEPTION
            'money_entries_kind_known lost a member: %', allowed;
    END IF;
END;
$$;
