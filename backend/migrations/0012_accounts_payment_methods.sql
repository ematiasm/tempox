-- v3 (part 1 of 2): the vocabulary — accounts, payment methods, and the guards
-- that keep "where the money moved" meaningful.
--
-- **Why this is two migrations and not one.** The previous single file carried two
-- unrelated classes of risk: creating a small vocabulary plus its invariants, and
-- rewriting the destination of every document already in the database. Native review
-- found a defect in each half (a deadlocked default-account guard, then a race in the
-- same guard), and a file that big makes each reviewer re-read both halves. Split,
-- `0013` holds the data rewrite and this file holds the vocabulary, so each is
-- reviewed against one question.
--
-- **The split point is a dependency, not a line count.** The guards below are about
-- accounts and methods alone; the guard in `0013` (`guard_payment_method_account`)
-- queries the four documents, so it cannot live here — those tables have no
-- `payment_method_id` until `0013` adds it.
--
-- **What this file does NOT do.** No balance column: a balance is a `SUM` over
-- documents, derived by the view in `0014`. No `kind` on `accounts`: that
-- classification was argued for and withdrawn, because its three values are
-- indistinguishable in v3 and locating the till is already solved by a fixed id —
-- the mechanism `WALK_IN_CUSTOMER_ID` (`sales.rs`) and `ONE_OFF_SUPPLIER_ID`
-- (`purchases.rs`) established.
--
-- The direction is the requirement: a payment method belongs to exactly one
-- account; an account has zero or more methods; an event names the *method* and the
-- method resolves the *account*. A method shared across accounts would make every
-- per-account balance unreconstructable.

-- ── Accounts ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS accounts (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    -- Whether this is the account the shop means by "the till". Exactly one row
    -- may hold it, enforced below. It carries two real consumers — the `/sales`
    -- footer and the default selection in the sale's payment-method picker — so
    -- it is configuration with use, not a preventive classification.
    is_default BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A blank name would render as an empty row in the picker, which is worse
    -- than a missing account. Trimmed before the check, so '   ' is refused too.
    CONSTRAINT accounts_name_not_blank CHECK (btrim(name) <> '')
);

-- At most one default account. A partial unique index rather than a CHECK,
-- because "exactly one row has this value" is a statement about the table, and a
-- row-level CHECK cannot see sibling rows.
CREATE UNIQUE INDEX IF NOT EXISTS accounts_one_default_idx
    ON accounts (is_default) WHERE is_default;

-- ── Payment methods ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS payment_methods (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id UUID NOT NULL REFERENCES accounts (id),
    -- The stored name is English and is a contract: it reaches the API and any
    -- export. "Efectivo"/"Crédito" live in the frontend dictionary, which is how
    -- `walk-in` survived es-AR unchanged.
    name       TEXT NOT NULL UNIQUE,
    -- Retires a method from the picker while its history stays readable. It is a
    -- **picker** concern only: the write paths accept an inactive method, which is
    -- what keeps the `walk-in`+credit guard from passing for the wrong reason (a
    -- refusal caused by inactivity rather than by the rule under test).
    is_active  BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT payment_methods_name_not_blank CHECK (btrim(name) <> '')
);

CREATE INDEX IF NOT EXISTS payment_methods_account_id_idx
    ON payment_methods (account_id);

-- ── Seeds ───────────────────────────────────────────────────────────────────
--
-- Fixed ids, exactly as `walk-in` (`…0001`) and `one-off-supplier` (`…0002`) are.
-- A fixed id is what lets the server find a known row without a lookup and
-- without a classification column.
--
-- **`credit` is not seeded, and it never was, in this history.** v3 seeded an
-- account (`…0005`) and an inactive method (`…0006`) named `credit` so that a
-- guard about "a credit sale may not be walk-in" had a row that could make it
-- fail. That guard is gone and so is the row: debt is a document (`obligations`,
-- `payables`) and there is no account that holds a right to be paid. `S7` of
-- `v4-settlement.md` retired it, and the owner's decision was to rewrite the
-- history rather than add a migration that deletes a row the initial schema
-- should never have created — the application has never shipped, so no database
-- holds a document naming it.
--
-- **This leaves only `cash` as a seeded destination**, which is what the write
-- paths already assume: `CASH_ACCOUNT_ID` and `CASH_PAYMENT_METHOD_ID` in
-- `models/account.rs` are the only fixed ids the server resolves without a
-- lookup.
INSERT INTO accounts (id, name, is_default)
VALUES ('00000000-0000-0000-0000-000000000003', 'cash', true)
ON CONFLICT DO NOTHING;

INSERT INTO payment_methods (id, account_id, name, is_active)
VALUES ('00000000-0000-0000-0000-000000000004',
        '00000000-0000-0000-0000-000000000003', 'cash', true)
ON CONFLICT DO NOTHING;

-- ── The default account always has an active method ─────────────────────────

-- The owner's invariant. It is a statement about **other rows**, so no CHECK can
-- express it, and it has three ways to be violated:
--   * the last active method of the default account is deactivated;
--   * the last active method of the default account is moved to another account;
--   * an account is made default while having no active method.
-- A fourth path is closed by refusing DELETE on both tables (below): if a method
-- could be deleted, this invariant would be unreachable-except-by-delete.
--
-- The triggers are created **after** the seeds, because the seed inserts the
-- default account before any method exists. A trigger created earlier would fail
-- on the very rows it is meant to protect the boot of.
CREATE OR REPLACE FUNCTION guard_default_account_keeps_active_method() RETURNS trigger AS $$
DECLARE
    is_default_account boolean;
BEGIN
    -- Fires when a method stops being active, or leaves the account. Both would
    -- remove the last active method from a default account.
    IF (OLD.is_active AND NOT NEW.is_active)
       OR NEW.account_id IS DISTINCT FROM OLD.account_id THEN
        -- **The lock is the guard, not a hint.** Reproduced with two real sessions
        -- before this line existed: A deactivates m1 and B deactivates m2
        -- concurrently, each sees the other method as still active, both commit, and
        -- the default account is left with **zero** active methods. An EXISTS check
        -- alone cannot see a concurrent uncommitted deactivation, so nothing
        -- serialized the two writers. `FOR UPDATE` on the account row does: the
        -- second transaction blocks until the first commits, then sees the real
        -- state and refuses. Found by native review (`R3-001`, second occurrence) as
        -- an inferential finding and confirmed by reproduction.
        SELECT a.is_default INTO is_default_account
          FROM accounts a WHERE a.id = OLD.account_id FOR UPDATE;

        IF is_default_account THEN
            IF NOT EXISTS (
                SELECT 1 FROM payment_methods pm
                 WHERE pm.account_id = OLD.account_id
                   AND pm.is_active
                   AND pm.id <> OLD.id
            ) THEN
                RAISE EXCEPTION
                    'account % is the default and would be left with no active payment method',
                    OLD.account_id;
            END IF;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_default_account_guard ON payment_methods;
CREATE TRIGGER payment_methods_default_account_guard
BEFORE UPDATE OF is_active, account_id ON payment_methods
FOR EACH ROW EXECUTE FUNCTION guard_default_account_keeps_active_method();

CREATE OR REPLACE FUNCTION guard_new_default_account_has_active_method() RETURNS trigger AS $$
BEGIN
    -- Covers both "make an existing account default" and "create it already
    -- default". `is_default = false` is always allowed: it can only *reduce* the
    -- set of defaults, and the partial unique index already allows zero.
    IF NEW.is_default AND (TG_OP = 'INSERT' OR NOT OLD.is_default) THEN
        -- The same race as the deactivation path, in the other direction: promoting
        -- an account while its last active method is concurrently deactivated would
        -- otherwise leave a default with nothing active. Locking the candidate row
        -- makes the two writers serialize on the same object.
        IF TG_OP = 'UPDATE' THEN
            PERFORM 1 FROM accounts WHERE id = NEW.id FOR UPDATE;
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM payment_methods pm
             WHERE pm.account_id = NEW.id AND pm.is_active
        ) THEN
            RAISE EXCEPTION
                'account % cannot be the default: it has no active payment method',
                NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_default_needs_method_guard ON accounts;
CREATE TRIGGER accounts_default_needs_method_guard
BEFORE INSERT OR UPDATE OF is_default ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_new_default_account_has_active_method();

-- **Moving the default, and the deadlock this replaced.** The first version of
-- this migration refused every true→false transition here, and `_one_default_idx`
-- refuses a second true row while the current one is still true. Measured, the two
-- together made the default **permanently fixed** to whichever account was seeded:
--
--   apagar cash primero            → 'cannot stop being the default'
--   prender credit primero        → duplicate key on accounts_one_default_idx
--
-- No ordering worked. The invariant was also mis-stated: "the shop always has
-- exactly one till with an active method" is a statement about the table at the
-- end of a transaction, and a BEFORE trigger cannot see that state. It is therefore
-- a **deferrable constraint**, not a trigger: within one transaction the shop may
-- briefly have zero or two defaults, and the check runs at COMMIT.
--
-- The fix was found by native review (`R3-001`, critical) and verified by hand:
-- the tests in this slice had encoded the bug, asserting that unsetting the default
-- was an error instead of that a *transition* was legal as long as one always
-- remained.
--
-- What remains as a trigger is only the part that a constraint cannot express:
-- the account being made default must already have an active method, because that
-- is a fact about the candidate, not about the final count.
CREATE OR REPLACE FUNCTION guard_new_default_account_has_active_method() RETURNS trigger AS $$
BEGIN
    IF NEW.is_default AND (TG_OP = 'INSERT' OR NOT OLD.is_default) THEN
        IF NOT EXISTS (
            SELECT 1 FROM payment_methods pm
             WHERE pm.account_id = NEW.id AND pm.is_active
        ) THEN
            RAISE EXCEPTION
                'account % cannot be the default: it has no active payment method',
                NEW.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_default_needs_method_guard ON accounts;
CREATE TRIGGER accounts_default_needs_method_guard
BEFORE INSERT OR UPDATE OF is_default ON accounts
FOR EACH ROW EXECUTE FUNCTION guard_new_default_account_has_active_method();

-- The count invariant, deferred to COMMIT. `accounts_one_default_idx` still
-- guarantees *at most* one at any instant; this guarantees *exactly* one at the end
-- of every transaction, which is the rule the shop actually needs: the `/sales`
-- footer and the sale picker always have an account to fall back to.
--
-- **A CONSTRAINT TRIGGER, not a deferred CHECK.** Measured while writing this:
-- PostgreSQL refuses `CHECK constraints cannot be marked DEFERRABLE`, on every
-- version — only UNIQUE, PRIMARY KEY, EXCLUDE and FOREIGN KEY may be deferred, and a
-- partial unique index (which is what `accounts_one_default_idx` is) cannot be a
-- deferrable constraint at all. The mechanism designed exactly for "validate the
-- final state of a table at COMMIT" is a constraint trigger, and that is what this
-- is: AFTER, FOR EACH ROW, DEFERRABLE INITIALLY DEFERRED.
CREATE OR REPLACE FUNCTION assert_exactly_one_default_account() RETURNS trigger AS $$
DECLARE
    defaults bigint;
BEGIN
    SELECT count(*) INTO defaults FROM accounts WHERE is_default;
    IF defaults <> 1 THEN
        RAISE EXCEPTION
            'the shop must have exactly one default account at commit; found %', defaults;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_exactly_one_default ON accounts;
CREATE CONSTRAINT TRIGGER accounts_exactly_one_default
AFTER INSERT OR UPDATE OR DELETE ON accounts
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION assert_exactly_one_default_account();

-- ── No deletes ──────────────────────────────────────────────────────────────
--
-- Deletion would erase the explanation of a balance: a method's documents would
-- keep a foreign key to a row that no longer says which account the money went to.
-- `is_active` is the answer to "I do not want it in the picker any more", and that
-- is the only retirement both tables need.
--
-- Both `UPDATE` and `DELETE` are guarded where mutation is refused at all, and
-- `TRUNCATE` is a separate statement-level trigger: it does not fire FOR EACH ROW,
-- so a row-level guard never sees it. `0007` shipped exactly that mistake — a
-- blanket reject function applied only to TRUNCATE, so UPDATE and DELETE succeeded
-- while the docs claimed they did not. `0008` repaired it; that lesson is applied
-- here up front rather than repaired later.
CREATE OR REPLACE FUNCTION reject_account_delete() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'accounts cannot be deleted: is_active on its methods is how a destination is retired';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION reject_payment_method_delete() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'payment_methods cannot be deleted: is_active is how a method is retired';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS accounts_no_delete ON accounts;
CREATE TRIGGER accounts_no_delete
BEFORE DELETE ON accounts
FOR EACH ROW EXECUTE FUNCTION reject_account_delete();

DROP TRIGGER IF EXISTS accounts_no_truncate ON accounts;
CREATE TRIGGER accounts_no_truncate
BEFORE TRUNCATE ON accounts
FOR EACH STATEMENT EXECUTE FUNCTION reject_account_delete();

DROP TRIGGER IF EXISTS payment_methods_no_delete ON payment_methods;
CREATE TRIGGER payment_methods_no_delete
BEFORE DELETE ON payment_methods
FOR EACH ROW EXECUTE FUNCTION reject_payment_method_delete();

DROP TRIGGER IF EXISTS payment_methods_no_truncate ON payment_methods;
CREATE TRIGGER payment_methods_no_truncate
BEFORE TRUNCATE ON payment_methods
FOR EACH STATEMENT EXECUTE FUNCTION reject_payment_method_delete();

-- ── Prove the guards exist ──────────────────────────────────────────────────
--
-- LAST, after every CREATE TRIGGER, because that is the only position where a
-- failure names the real cause. `0009` shipped this check in the middle and a re-run
-- reported the constraint that happened to fail next instead of the guard that was
-- actually absent; a diagnostic that names the wrong cause costs more than no
-- diagnostic.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payment_methods', 'payment_methods_default_account_guard'),
              ('payment_methods', 'payment_methods_no_delete'),
              ('payment_methods', 'payment_methods_no_truncate'),
              ('accounts', 'accounts_default_needs_method_guard'),
              ('accounts', 'accounts_no_delete'),
              ('accounts', 'accounts_no_truncate'),
              ('accounts', 'accounts_exactly_one_default')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 7 THEN
        RAISE EXCEPTION
            'expected 7 v3 account/method triggers, found %; refusing to continue', present;
    END IF;

    -- The count invariant is a DEFERRABLE constraint trigger, and its deferrability
    -- is the whole point: without it the shop could end a transaction with no till
    -- and nothing else here would notice.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'accounts_exactly_one_default'
           AND condeferrable
           AND condeferred
    ) THEN
        RAISE EXCEPTION
            'accounts_exactly_one_default is missing or not DEFERRABLE INITIALLY DEFERRED; refusing to continue';
    END IF;
END;
$$;
