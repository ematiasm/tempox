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
