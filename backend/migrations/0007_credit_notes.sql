-- v1.5: reversing a sale with a credit note.
--
-- **The shape.** A sale is an immutable event (0003 locked its economics and
-- its lines). Correcting a mistake therefore cannot be an edit: it is a second
-- document — a *credit note* — that returns stock and money. The corrected sale
-- is never written: `status` stays `'confirmed'`, `reversed_at` stays NULL, its
-- `total` and `customer_id` are untouched. Reversal is a compensating row, and
-- this migration gives it somewhere to live.
--
-- **Why `status`/`reversed_at` are not used.** v1 reserved them for a lifecycle
-- (draft -> confirmed -> cancel -> done) that never happened, and 0003's
-- `guard_sale_economics` still permits those two columns to be written. A status
-- column answers "what state is this document in"; the question here is "how
-- much has been returned", which is a quantity that accumulates and must be
-- auditable line by line. A status cannot carry a partial refund, and it cannot
-- say which lines came back. The note carries both, so the note is the record
-- and the sale stays exactly as it was recorded.
--
-- **Partial and cumulative.** Several notes may be issued against one sale while
-- the quantity refunded per line never exceeds what was sold on that line. The
-- guard is enforced in the write path, inside the sale's row lock, so two
-- concurrent refunds cannot both pass it (see `credit_note_items_sale_item_id_idx`).
--
-- **Bonus lines stay coherent.** 0005 allows a line priced at zero, so a refund
-- returns what was charged: a bonus line returns `0.00` while its stock still
-- comes back. Amounts here follow that same `>= 0` rule, and money is the same
-- decimal contract 0006 established for the sale path.

-- ── The document ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS credit_notes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sale_id    UUID NOT NULL REFERENCES sales (id),
    total      NUMERIC(14,6) NOT NULL
               -- Same money contract as `sales.total` (0006): two decimals is
               -- money, the declared scale of 6 is what lets the guard refuse a
               -- finer value instead of PostgreSQL rounding it away first.
               CHECK (total >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credit_notes_total_whole_cents CHECK (total = round(total, 2))
);

-- The lines of the note. Name, unit and price are snapshots of the corrected
-- sale line, so the note reads as a self-contained receipt even if the catalog
-- changes later — the same defence-in-depth 0002 chose for `sale_items`.
CREATE TABLE IF NOT EXISTS credit_note_items (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    credit_note_id UUID NOT NULL REFERENCES credit_notes (id),
    sale_item_id   UUID NOT NULL REFERENCES sale_items (id),
    product_id     UUID NOT NULL REFERENCES products (id),
    product_name   TEXT NOT NULL,
    quantity       INTEGER NOT NULL CHECK (quantity > 0),
    unit_price     NUMERIC(14,6) NOT NULL
                   CHECK (unit_price >= 0),
    line_total     NUMERIC(14,6) NOT NULL
                   CHECK (line_total >= 0),
    unit           TEXT NOT NULL CHECK (unit IN ('unit', 'g', 'ml')),
    CONSTRAINT credit_note_items_unit_price_whole_cents
        CHECK (unit_price = round(unit_price, 2)),
    CONSTRAINT credit_note_items_line_total_whole_cents
        CHECK (line_total = round(line_total, 2))
);

CREATE INDEX IF NOT EXISTS credit_notes_sale_id_idx ON credit_notes (sale_id);
CREATE INDEX IF NOT EXISTS credit_note_items_credit_note_id_idx
    ON credit_note_items (credit_note_id);
-- The cumulative guard reads this sum inside the sale's row lock; the index is
-- what keeps that read from being a scan of the whole table per refund.
CREATE INDEX IF NOT EXISTS credit_note_items_sale_item_id_idx
    ON credit_note_items (sale_item_id);

-- `credit_notes.total = SUM(its items' line_total)` is deliberately not a CHECK.
-- It is a write-path invariant, exactly as `sales.total = SUM(sale_items.line_total)`
-- is (0006 recorded the same decision for the sale path): a row-level CHECK
-- cannot see the sibling rows, and a deferred constraint would turn a bug in one
-- handler into a commit-time failure nobody can attribute.

-- ── History guards, in the style 0003 established ───────────────────────────

-- A note's lines are pure history too. A wrong note is corrected by another
-- compensating document, never by rewriting this one.
CREATE OR REPLACE FUNCTION reject_credit_note_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'credit_note_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_note_items_immutable ON credit_note_items;
CREATE TRIGGER credit_note_items_immutable
BEFORE UPDATE OR DELETE ON credit_note_items
FOR EACH ROW EXECUTE FUNCTION reject_credit_note_item_mutation();

-- The append-only rule above covers UPDATE and DELETE; this closes the one
-- statement that can empty the table without firing it. TRUNCATE does not run
-- FOR EACH ROW triggers — the same hole 0004 closed for `stock_movements`.
DROP TRIGGER IF EXISTS credit_note_items_no_truncate ON credit_note_items;
CREATE TRIGGER credit_note_items_no_truncate
BEFORE TRUNCATE ON credit_note_items
FOR EACH STATEMENT EXECUTE FUNCTION reject_credit_note_item_mutation();

-- A note's identity and its money are immutable. Nothing here may change after
-- the row exists.
CREATE OR REPLACE FUNCTION guard_credit_note_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.sale_id <> OLD.sale_id OR NEW.total <> OLD.total THEN
        RAISE EXCEPTION 'credit note % economics are immutable', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_economics_guard ON credit_notes;
CREATE TRIGGER credit_notes_economics_guard
BEFORE UPDATE ON credit_notes
FOR EACH ROW EXECUTE FUNCTION guard_credit_note_economics();

-- A statement-level TRUNCATE trigger cannot read NEW or OLD — there is no row
-- to pass it — so the row-level guard above cannot run here. This function only
-- raises, which is what a statement-level trigger can do.
CREATE OR REPLACE FUNCTION reject_credit_note_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'credit_notes are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_no_truncate ON credit_notes;
CREATE TRIGGER credit_notes_no_truncate
BEFORE TRUNCATE ON credit_notes
FOR EACH STATEMENT EXECUTE FUNCTION reject_credit_note_mutation();

-- ── The ledger learns one more reason ───────────────────────────────────────

-- 0004 left this note for exactly this migration: "the v1.5 reversal adds its
-- own value in a new migration rather than reusing one".
ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS credit_note_id UUID REFERENCES credit_notes (id);

ALTER TABLE stock_movements
    DROP CONSTRAINT stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale', 'credit_note'));

-- The movement still points at both documents it belongs to: the sale whose
-- stock left, and the note that brought it back. The old coherence rule is
-- replaced rather than kept, because it was written for a two-reason vocabulary
-- and would now be false for a credit-note row.
--
-- `sale_id` stays NOT NULL for a credit note on purpose. A refund that cannot be
-- traced to the document it corrects is not auditable, and the sale is exactly
-- what the ledger entry explains.
ALTER TABLE stock_movements
    DROP CONSTRAINT stock_movements_sale_id_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_document_coherent
    CHECK (
        (reason IN ('sale', 'credit_note') AND sale_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL)
    );
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_credit_note_coherent
    CHECK (
        (reason = 'credit_note') = (credit_note_id IS NOT NULL)
    );

-- A guard worth having: the two constraints above are only true if every row
-- already in the ledger satisfies them, and `ADD CONSTRAINT` will happily
-- succeed against an empty table while a later INSERT lands outside them. If a
-- pre-existing row contradicts the new vocabulary — a `'sale'` row with no
-- `sale_id`, say, which 0004 allowed only in the other direction — the migration
-- says so instead of leaving a constraint that the data already violates.
DO $$
DECLARE
    incoherent bigint;
BEGIN
    SELECT count(*) INTO incoherent FROM stock_movements
     WHERE (reason IN ('sale', 'credit_note') AND sale_id IS NULL)
        OR (reason = 'initial' AND sale_id IS NOT NULL);
    IF incoherent > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that contradict the v1.5 coherence rule; fix them before migrating',
            incoherent;
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS stock_movements_credit_note_id_idx
    ON stock_movements (credit_note_id);
