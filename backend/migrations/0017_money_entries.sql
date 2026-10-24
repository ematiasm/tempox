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
