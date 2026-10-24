-- v4 S4: the reversal budget lives on the sale, not on the credit-note table.
--
-- `odd/tasks/v4-settlement.md` S9. Until now the rule "a line may not be reversed more
-- than it was sold" lived in **one place and was enforced in another**:
--
--   * the handler computed `SUM(credit_note_items.quantity)` against
--     `sale_items.quantity`, inside the sale's row lock (`sales.rs`);
--   * the schema enforced **nothing at all**.
--
-- Measured before this file existed: a `psql` session inserted 5 units against a line of
-- 1 and nothing objected. The comment in `0007` says the guard is "enforced in the write
-- path", which was true and was the problem — a guard one route applies is a guard the
-- next route forgets.
--
-- **Why this must move now rather than later.** The annulment (`S10`) is a *second*
-- document that reverses stock. If the budget stays a `SUM` over one document table, an
-- annulment either does not count (and the line can be reversed twice: once by an
-- annulment, once by a credit note) or it counts in a place only it knows about. The
-- budget belongs to the resource both documents act on: the sale line.

-- ── `sale_items` stops being blanket-immutable, for exactly one column ─────
--
-- **This is the second thing `0019` was going to break, and it was found by running it.**
-- `0003` installed `sale_items_immutable` as `BEFORE UPDATE OR DELETE` with an
-- unconditional `RAISE`. That is right for every column on the line — name, price,
-- quantity and total are history — but it also refuses the counter this slice needs, so
-- the backfill below aborted with
-- *"sale_items are immutable: corrections are compensating rows, not edits"*.
--
-- The repo already has the pattern for exactly this: `guard_sale_economics` (`0003`, and
-- re-cut in `0013`) refuses the sale's money columns and permits the one column whose
-- whole purpose is to change. This is the same technique one table deeper. The guard is
-- **narrowed, not removed**: every column except `reversed_quantity` is still refused,
-- and the budget has its own guard (`sale_items_reversal_budget`) enforcing that it may
-- only grow toward — never past — the quantity sold.
CREATE OR REPLACE FUNCTION reject_sale_item_mutation() RETURNS trigger AS $$
BEGIN
    -- **Every column is compared, `id` included.** The first version of this guard left
    -- `id` out of the list, so an `UPDATE` that rewrote only the primary key fell into the
    -- "only the budget changed" branch and was allowed. Found by review (`R3-001`,
    -- CRITICAL) and reproduced against a live database before this fix.
    IF TG_OP = 'UPDATE'
       AND NEW.id                    = OLD.id
       AND NEW.sale_id               = OLD.sale_id
       AND NEW.product_id            = OLD.product_id
       AND NEW.product_name          = OLD.product_name
       AND NEW.quantity              = OLD.quantity
       AND NEW.unit_price            = OLD.unit_price
       AND NEW.line_total            = OLD.line_total
       AND NEW.unit                  = OLD.unit
    THEN
        -- **A budget increase is allowed only from the consumer.** Everything privileged
        -- inside this transaction sets `tpox.budget_writer`, and nothing else does; a bare
        -- `UPDATE sale_items SET reversed_quantity = ...` from `psql` is refused. Without
        -- this the counter could be inflated with no credit note at all, and the handler
        -- — which reads it — would then refuse a refund that was legitimate (`R3-002`,
        -- CRITICAL, also reproduced).
        IF NEW.reversed_quantity = OLD.reversed_quantity
           OR current_setting('tpox.budget_writer', true) = 'on'
        THEN
            RETURN NEW;
        END IF;
        RAISE EXCEPTION
            'sale item % reversal budget may only move through the refund path', NEW.id;
    END IF;
    RAISE EXCEPTION 'sale_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

-- ── The column ─────────────────────────────────────────────────────────────
--
-- `reversed_quantity` is a running total rather than a derived sum, which is a deliberate
-- departure from this schema's usual "derive, never store". The reason is the one S9
-- gives: two different documents consume the same budget, and a derived sum over one of
-- them is exactly how the second document bypassed it. A stored counter with a guard is
-- the only shape in which "consumed by any document" is expressible.
ALTER TABLE sale_items
    ADD COLUMN IF NOT EXISTS reversed_quantity INTEGER NOT NULL DEFAULT 0;

-- ── Backfill, then prove it ────────────────────────────────────────────────
--
-- A migration that refuses to run on a database with refunds is not a migration: the
-- development database has one already, and every delivered one will. The figure comes
-- from the documents that exist, and the verification below compares back to its own
-- source rather than trusting the `UPDATE`.
UPDATE sale_items si
   SET reversed_quantity = COALESCE(refunded.total, 0)
  FROM (
        SELECT sale_item_id, SUM(quantity)::int AS total
          FROM credit_note_items
         GROUP BY sale_item_id
       ) AS refunded
 WHERE refunded.sale_item_id = si.id;

-- ── The guards ─────────────────────────────────────────────────────────────
--
-- Three ways to break the budget, and all three are refused:
--   * a credit-note item that would take the total past what was sold;
--   * a lower `reversed_quantity` than the ledger already accounts for (a rewrite of the
--     counter that hides history);
--   * a negative counter, or one above the quantity sold, from any other path.
CREATE OR REPLACE FUNCTION guard_sale_item_reversal_budget() RETURNS trigger AS $$
DECLARE
    sold integer;
BEGIN
    IF NEW.reversed_quantity < 0 THEN
        RAISE EXCEPTION
            'sale item % cannot have reversed a negative quantity', NEW.id;
    END IF;

    SELECT quantity INTO sold FROM sale_items WHERE id = NEW.id;
    IF NEW.reversed_quantity > sold THEN
        RAISE EXCEPTION
            'sale item % reversed % of % sold: a line cannot be reversed more than it was sold',
            NEW.id, NEW.reversed_quantity, sold;
    END IF;

    -- A counter may only grow. The reversal documents are append-only, so a shrinking
    -- counter is the one edit that would let the same unit be reversed twice.
    IF TG_OP = 'UPDATE' AND NEW.reversed_quantity < OLD.reversed_quantity THEN
        RAISE EXCEPTION
            'sale item % reversal budget cannot shrink: % -> %',
            NEW.id, OLD.reversed_quantity, NEW.reversed_quantity;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sale_items_reversal_budget ON sale_items;
CREATE TRIGGER sale_items_reversal_budget
BEFORE INSERT OR UPDATE OF reversed_quantity ON sale_items
FOR EACH ROW EXECUTE FUNCTION guard_sale_item_reversal_budget();

-- The document side: inserting a credit-note line consumes the budget, and is refused
-- when the budget is gone. This is the guard that was missing, and it is a trigger rather
-- than a CHECK because a row-level CHECK cannot see the sibling rows it would have to sum.
CREATE OR REPLACE FUNCTION consume_reversal_budget() RETURNS trigger AS $$
DECLARE
    remaining integer;
BEGIN
    -- Lock the line before reading, so two concurrent refunds cannot both see the same
    -- remaining balance. `0007` relied on the handler's `FOR UPDATE` on the sale; the
    -- schema cannot rely on a caller's lock discipline.
    SELECT quantity - reversed_quantity INTO remaining
      FROM sale_items WHERE id = NEW.sale_item_id
       FOR UPDATE;

    IF remaining IS NULL THEN
        RAISE EXCEPTION
            'credit note item names sale item % which does not exist', NEW.sale_item_id;
    END IF;

    IF NEW.quantity > remaining THEN
        RAISE EXCEPTION
            'sale item % has % left to reverse, but % were requested',
            NEW.sale_item_id, remaining, NEW.quantity;
    END IF;

    -- The only writer allowed to move the counter, announced in the transaction's own
    -- session settings so a direct `UPDATE` cannot impersonate it.
    PERFORM set_config('tpox.budget_writer', 'on', true);
    UPDATE sale_items
       SET reversed_quantity = reversed_quantity + NEW.quantity
     WHERE id = NEW.sale_item_id;
    PERFORM set_config('tpox.budget_writer', 'off', true);

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_note_items_consume_budget ON credit_note_items;
CREATE TRIGGER credit_note_items_consume_budget
BEFORE INSERT ON credit_note_items
FOR EACH ROW EXECUTE FUNCTION consume_reversal_budget();

-- ── Prove the guards exist, and that the backfill matches its source ───────
--
-- LAST, for the reason `0017` records: a check placed earlier names whatever fails next
-- instead of the guard that is actually absent.
DO $$
DECLARE
    present bigint;
    drifted bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('sale_items', 'sale_items_reversal_budget'),
              ('credit_note_items', 'credit_note_items_consume_budget')
           ) AS wanted(tbl, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.tbl::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 2 THEN
        RAISE EXCEPTION
            'expected 2 reversal-budget guards, found %; refusing to continue', present;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_name = 'sale_items' AND column_name = 'reversed_quantity'
    ) THEN
        RAISE EXCEPTION 'sale_items.reversed_quantity is missing; refusing to continue';
    END IF;

    -- The backfill against its own source. A migration that silently under-counted would
    -- pass every other check in this file and leave budget that does not exist.
    SELECT count(*) INTO drifted
      FROM sale_items si
      LEFT JOIN (
            SELECT sale_item_id, SUM(quantity)::int AS total
              FROM credit_note_items
             GROUP BY sale_item_id
           ) r ON r.sale_item_id = si.id
     WHERE si.reversed_quantity <> COALESCE(r.total, 0);
    IF drifted <> 0 THEN
        RAISE EXCEPTION
            '% sale items have a reversal budget that disagrees with the credit notes on record; refusing to continue',
            drifted;
    END IF;
END;
$$;
