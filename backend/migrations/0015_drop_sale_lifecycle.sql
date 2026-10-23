-- v3 (last): a sale has no lifecycle, because nothing ever had one.
--
-- **What was there.** `0002` reserved `status` (`draft | confirmed | cancel | done`)
-- and `reversed_at` for a lifecycle v1 never used. Measured over the whole life of
-- the system: **no write path in `backend/src` has ever set either column.** Their
-- only reads were `WHERE status = 'confirmed'` in two queries, and a dead
-- `status === 'cancel' || reversed_at !== null` branch in the ledger that was
-- unreachable by construction — nothing could set `'cancel'`.
--
-- **Why they were wrong, and it is not "unused".** `0007` argued it when it built
-- the credit note: *a status answers what state a document is in; the question here
-- is how much has been returned, which is a quantity that accumulates and must be
-- auditable line by line.* The same argument kills `done`: a customer owing 10.000
-- who pays 3.000 makes `done` unrepresentable without `partially_done`, and the enum
-- then grows with every document in the business. That is precisely the defect
-- `0011` was written to kill for stock — constraints enumerating a vocabulary that
-- every new document re-cuts.
--
-- **What replaces it: nothing.** "The row exists" and "it counts" become one
-- statement, exactly as they already were for `purchases`, `credit_notes` and
-- `supplier_returns`. An annulment is derived from the credit notes and from nothing
-- else, which is what the client already did.
--
-- **The risk this migration actually carries is the DROP, not the view.** A
-- `DROP COLUMN` on populated tables is the one irreversible act in the whole slice,
-- so the two pre-checks below run *before* anything is altered, and they refuse
-- rather than guess. Both were measured empty on the development database and on the
-- test database before this file was written.

-- ── Pre-checks: refuse to destroy a fact ────────────────────────────────────
--
-- If any sale says anything other than `confirmed`, then something in this database
-- *did* use the lifecycle and dropping it would erase that fact silently. If any sale
-- carries `reversed_at`, the system recorded a reversal through the column rather than
-- through a credit note, and removing it would lose the only trace. Neither can happen
-- on a schema whose write paths never touch them — which is exactly why the check is
-- worth having: it proves that claim about *this* data instead of assuming it.
DO $$
DECLARE
    not_confirmed bigint;
    reversed bigint;
BEGIN
    SELECT count(*) INTO not_confirmed FROM sales WHERE status <> 'confirmed';
    SELECT count(*) INTO reversed      FROM sales WHERE reversed_at IS NOT NULL;

    IF not_confirmed > 0 OR reversed > 0 THEN
        RAISE EXCEPTION
            'sales still use the lifecycle (% rows with status <> confirmed, % with reversed_at set); '
            'a credit note is the record of a reversal, so these must be migrated rather than dropped',
            not_confirmed, reversed;
    END IF;
END;
$$;

-- ── The full-totals snapshot, taken before the drop ─────────────────────────
--
-- The columns this file removes are read by `money_movements`, so that view has to be
-- recreated in the same transaction. To prove the recreation changed no answer, the
-- per-account totals are captured here and compared after. A migration that rewrites a
-- read model owes that comparison; without it, "the view still works" would be a claim
-- about compilation, not about arithmetic.
-- `DROP` first: a `TEMP TABLE` lives for the session, and a migrator that re-runs this
-- file within one connection — a retry after a failed attempt, which is exactly how
-- `0009`'s re-run defect was found — would otherwise fail on the name rather than on
-- anything true. `R3-001` flagged the unguarded create; this is the guard.
DROP TABLE IF EXISTS v3_totals_before_drop;
CREATE TEMP TABLE v3_totals_before_drop AS
SELECT account_id, SUM(amount) AS total
  FROM money_movements
 GROUP BY account_id;

-- ── Retire the lifecycle ────────────────────────────────────────────────────

-- The guard is re-cut BEFORE the columns go, for the ordering reason `0013` recorded:
-- a plpgsql body is not resolved when it is created, so a function still naming a
-- dropped column stays valid-looking until the next UPDATE fails at runtime with
-- `column ... does not exist`. Keeping the guard's remaining comparisons is what
-- matters — `total`, `customer_id` and `payment_method_id` are still immutable.
CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total IS DISTINCT FROM OLD.total
       OR NEW.customer_id IS DISTINCT FROM OLD.customer_id
       OR NEW.payment_method_id IS DISTINCT FROM OLD.payment_method_id THEN
        RAISE EXCEPTION 'sale % economics are immutable: a correction is a credit note, never an edit', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- **The view goes first, and the first attempt got this wrong.** `money_movements`
-- reads `s.status`, so PostgreSQL refuses the drop with
-- `cannot drop column status of table sales because other objects depend on it`.
-- Measured, not guessed: the dependency was found by creating the view on the test
-- database and asking `pg_depend` which objects referenced the column. Recreating the
-- view without the filter in the same transaction, before the `DROP`, is the only
-- order that works.
-- ── The view without a filter ───────────────────────────────────────────────
--
-- Every remaining branch reads a document whose existence IS its validity, so the
-- sale branch loses its predicate and the four branches finally agree on what counts.
-- **The aliases are not decoration.** `CREATE OR REPLACE VIEW` refuses a column
-- rename, and the first attempt omitted them: PostgreSQL took the first branch's
-- third column to be named `text` and failed with
-- `cannot change name of view column "document_kind" to "text"`. Measured, then fixed.
CREATE OR REPLACE VIEW money_movements AS
SELECT s.payment_method_id          AS payment_method_id,
       pm.account_id                AS account_id,
       'sale'::text                 AS document_kind,
       s.id                         AS document_id,
       s.total                      AS amount,
       s.created_at                 AS occurred_at
  FROM sales s
  JOIN payment_methods pm ON pm.id = s.payment_method_id

UNION ALL
SELECT cn.payment_method_id, pm.account_id, 'credit_note'::text, cn.id, -cn.total, cn.created_at
  FROM credit_notes cn
  JOIN payment_methods pm ON pm.id = cn.payment_method_id

UNION ALL
SELECT p.payment_method_id, pm.account_id, 'purchase'::text, p.id, -p.total, p.created_at
  FROM purchases p
  JOIN payment_methods pm ON pm.id = p.payment_method_id

UNION ALL
SELECT sr.payment_method_id, pm.account_id, 'supplier_return'::text, sr.id, sr.total, sr.created_at
  FROM supplier_returns sr
  JOIN payment_methods pm ON pm.id = sr.payment_method_id;

COMMENT ON VIEW money_movements IS
'Read model: one row per effect of money on an account. Positive is money entering. Never written to; balances are SUM over this view.';

ALTER TABLE sales DROP COLUMN IF EXISTS status;
ALTER TABLE sales DROP COLUMN IF EXISTS reversed_at;

-- ── The drop changed no figure ──────────────────────────────────────────────
--
-- The assertion this migration exists to make. Every account that existed before the
-- drop must hold exactly the same total after it, and no account may have appeared.
DO $$
DECLARE
    differing bigint;
BEGIN
    SELECT count(*) INTO differing
      FROM v3_totals_before_drop before
      FULL JOIN (
            SELECT account_id, SUM(amount) AS total
              FROM money_movements
             GROUP BY account_id
           ) after ON after.account_id = before.account_id
     WHERE before.total IS DISTINCT FROM after.total;

    IF differing > 0 THEN
        RAISE EXCEPTION
            'dropping the sale lifecycle changed % account total(s); the view no longer reproduces the ledger',
            differing;
    END IF;
END;
$$;

-- ── Prove the guard survived ────────────────────────────────────────────────
--
-- Last, so a failure names the guard rather than whatever statement happens to fail
-- next — the ordering `0009` shipped wrong and `0010` repaired.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'sales_economics_guard'
           AND tgrelid = 'sales'::regclass
           AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION 'sales_economics_guard is missing after the lifecycle drop';
    END IF;
END;
$$;
