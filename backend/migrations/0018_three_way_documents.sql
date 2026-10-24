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
