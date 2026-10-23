-- v3: the seeded `credit` account and method keep their identity.
--
-- `backend/src/models/account.rs` names these two rows by fixed id —
-- `CREDIT_ACCOUNT_ID` (`…0005`) and `CREDIT_PAYMENT_METHOD_ID` (`…0006`) — and the
-- sale and purchase guards compare against that id with no lookup. A constant is
-- only as good as the row it points at, and until this migration nothing stopped
-- either row from being rearranged into something else:
--
--   UPDATE payment_methods SET account_id = '…0003' WHERE id = '…0006'   -- succeeded
--   UPDATE accounts SET is_default = true    WHERE id = '…0005'          -- succeeded
--
-- The first leaves the debt account with **zero** methods and puts a method named
-- `credit` on the till's account. The second makes `getCash()` — which reads exactly
-- `is_default` — show the debt account's balance as the shop's till, while a sale
-- that omits a method still posts to `…0003`. Both were reproduced against a real
-- server before this file existed (`odd/tasks/v3-seed-identities.md`, L2).
--
-- What was already closed and is *not* re-examined here: deletion. `0012` refuses
-- `DELETE` and `TRUNCATE` on both tables and the router has no delete route.
--
-- `name` and `is_active` stay editable, deliberately. A rename corrects a typo
-- because every document stores the id, and `is_active` is how the picker is told
-- what to offer. Whether `credit` should be *offered for sale* is a different
-- question, decided — and still open — in `v3-accounts.md`; nothing here settles it.

-- ── The seeded method never changes account ─────────────────────────────────
--
-- `IS DISTINCT FROM`, modelled on `guard_product_unit` (`0003`): the guard fires on
-- a write that names the column but only refuses a real change, so an UPDATE that
-- sets the value it already holds passes. That matters because `accounts.rs` sends
-- `COALESCE($4, account_id)` on every PATCH, which rewrites the column with its own
-- value whenever the caller renames something.
CREATE OR REPLACE FUNCTION guard_credit_method_identity() RETURNS trigger AS $$
BEGIN
    IF NEW.account_id IS DISTINCT FROM OLD.account_id THEN
        RAISE EXCEPTION
            'the seeded credit payment method cannot move to another account';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_credit_identity_guard ON payment_methods;
CREATE TRIGGER payment_methods_credit_identity_guard
BEFORE UPDATE OF account_id ON payment_methods
FOR EACH ROW
WHEN (OLD.id = '00000000-0000-0000-0000-000000000006'::uuid)
EXECUTE FUNCTION guard_credit_method_identity();

-- ── Nothing is ever attached to the debt account ────────────────────────────
--
-- The other side of the same rule, and the side that was reachable in practice: a
-- method moved *onto* `…0005` would answer "a customer's debt was settled by this
-- means", which no v3 document describes. Written as a refusal on the destination
-- rather than on the source, so it also covers `INSERT` — a new method cannot be
-- created on the debt account either.
CREATE OR REPLACE FUNCTION guard_credit_account_takes_no_method() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        'no payment method may be attached to the seeded credit account';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_credit_account_guard ON payment_methods;
CREATE TRIGGER payment_methods_credit_account_guard
BEFORE INSERT OR UPDATE OF account_id ON payment_methods
FOR EACH ROW
WHEN (NEW.account_id = '00000000-0000-0000-0000-000000000005'::uuid)
EXECUTE FUNCTION guard_credit_account_takes_no_method();

-- ── The debt account is never the till ──────────────────────────────────────
--
-- `is_default` answers "which account does the shop mean by the till", and exactly
-- one row holds it. `…0005` must never be that row.
--
-- This is a BEFORE trigger on the way *in*, and it is not redundant with the
-- existing rules: `accounts_default_needs_method_guard` refuses a default account
-- with no active method, which refuses `…0005` today only by accident — the moment
-- its seeded method is activated (an operation `is_active` explicitly permits) that
-- guard stops applying and the promotion succeeds. Measured, in that order, before
-- this file existed.
CREATE OR REPLACE FUNCTION guard_credit_account_is_never_default() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'the seeded credit account can never be the shop''s till';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_credit_never_default_guard ON accounts;
CREATE TRIGGER accounts_credit_never_default_guard
BEFORE UPDATE OF is_default ON accounts
FOR EACH ROW
WHEN (NEW.id = '00000000-0000-0000-0000-000000000005'::uuid AND NEW.is_default)
EXECUTE FUNCTION guard_credit_account_is_never_default();

-- ── Renaming the account does not carry the identity away ───────────────────

COMMENT ON TRIGGER payment_methods_credit_identity_guard ON payment_methods IS
    'Seeded credit method …0006 keeps its account; see odd/tasks/v3-seed-identities.md';
COMMENT ON TRIGGER payment_methods_credit_account_guard ON payment_methods IS
    'Nothing may be attached to the seeded credit account …0005';
COMMENT ON TRIGGER accounts_credit_never_default_guard ON accounts IS
    'The seeded credit account …0005 is never the shop till';

-- ── Prove the guards exist ──────────────────────────────────────────────────
--
-- LAST, after every CREATE TRIGGER, for the reason `0012` records: a check placed
-- earlier reports whatever constraint happens to fail next instead of the guard
-- that is actually missing.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payment_methods', 'payment_methods_credit_identity_guard'),
              ('payment_methods', 'payment_methods_credit_account_guard'),
              ('accounts', 'accounts_credit_never_default_guard')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 3 THEN
        RAISE EXCEPTION
            'expected 3 seed-identity guards, found %; refusing to continue', present;
    END IF;

    -- The seeded rows are still the rows the guards talk about. A guard on an id
    -- nothing holds would look present and protect nothing.
    IF NOT EXISTS (SELECT 1 FROM accounts WHERE id = '00000000-0000-0000-0000-000000000005') THEN
        RAISE EXCEPTION 'the seeded credit account …0005 is missing; the guards protect nothing';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM payment_methods
         WHERE id = '00000000-0000-0000-0000-000000000006'
           AND account_id = '00000000-0000-0000-0000-000000000005'
    ) THEN
        RAISE EXCEPTION 'the seeded credit method …0006 is missing or already moved';
    END IF;
END;
$$;
