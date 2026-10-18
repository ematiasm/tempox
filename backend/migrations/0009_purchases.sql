-- v2: the purchases slice — suppliers, purchases, purchase_items.
--
-- The mirror of 0002, with one deliberate asymmetry: a purchase has no status.
-- A sale carries a reserved lifecycle because v1's reversal needed it; a purchase
-- is a confirmed fact the moment it exists, so "the row exists" and "it counts"
-- stay the same statement (plan-v2.md, "Why not one document with a status").
--
-- One file, like 0002: the tables, the ledger's new reason and the guards are one
-- atomic schema change (v1 decision 9).

-- ── Suppliers ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS suppliers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- one-off-supplier: the default supplier for purchases with no named supplier,
-- exactly as `walk-in` is the default customer. A real category, seeded with a
-- fixed id so the API can reference it without a lookup.
--
-- The stored name is English and is a contract: it reaches the API and any
-- export. The localized display string ("Proveedor ocasional") lives in the
-- frontend dictionary, which is how `walk-in` survived es-AR unchanged.
INSERT INTO suppliers (id, name)
VALUES ('00000000-0000-0000-0000-000000000002', 'one-off-supplier')
ON CONFLICT DO NOTHING;

-- ── Purchases ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS purchases (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    supplier_id UUID NOT NULL REFERENCES suppliers (id),
    total       NUMERIC(14,6) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Sign and precision, the two rules 0006 established for money. `>= 0`
    -- rather than `> 0` matches `sales` since 0005 relaxed it: a zero-total
    -- document is expressible (free stock in, a bonus line out) and contributes
    -- nothing to the derived totals.
    CONSTRAINT purchases_total_check CHECK (total >= 0),
    CONSTRAINT purchases_total_whole_cents CHECK (total = round(total, 2))
);

CREATE TABLE IF NOT EXISTS purchase_items (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    purchase_id  UUID NOT NULL REFERENCES purchases (id),
    product_id   UUID NOT NULL REFERENCES products (id),
    product_name TEXT NOT NULL,
    quantity     INTEGER NOT NULL CHECK (quantity > 0),
    unit_price   NUMERIC(14,6) NOT NULL,
    line_total   NUMERIC(14,6) NOT NULL,
    unit         TEXT NOT NULL,

    CONSTRAINT purchase_items_unit_price_check CHECK (unit_price >= 0),
    CONSTRAINT purchase_items_unit_price_whole_cents CHECK (unit_price = round(unit_price, 2)),
    CONSTRAINT purchase_items_line_total_check CHECK (line_total >= 0),
    CONSTRAINT purchase_items_line_total_whole_cents CHECK (line_total = round(line_total, 2))
);

-- No unique constraint on (purchase_id, product_id): the same product can
-- legitimately arrive twice in one purchase at two different costs, and the
-- sales side already allows repeated lines.

CREATE INDEX IF NOT EXISTS purchases_created_at_idx ON purchases (created_at DESC);
CREATE INDEX IF NOT EXISTS purchases_supplier_id_idx ON purchases (supplier_id);
CREATE INDEX IF NOT EXISTS purchase_items_purchase_id_idx ON purchase_items (purchase_id);
CREATE INDEX IF NOT EXISTS purchase_items_product_id_idx ON purchase_items (product_id);

-- ── The ledger learns one more reason ───────────────────────────────────────
--
-- 0007 did this for 'credit_note' and left the note explaining why the
-- vocabulary grows in a new migration instead of reusing an existing value.
-- Same move here, extended to `purchase_id`.

ALTER TABLE stock_movements
    ADD COLUMN IF NOT EXISTS purchase_id UUID REFERENCES purchases (id);

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_reason_known;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_reason_known
    CHECK (reason IN ('initial', 'sale', 'credit_note', 'purchase'));

-- The coherence rule is recreated rather than extended, because 0007 wrote it
-- against a three-reason vocabulary (`sale`, `credit_note` => sale_id NOT NULL;
-- everything else => sale_id NULL) and a purchase row would violate it.
--
-- `sale_id` stays NOT NULL for a credit note on purpose, as 0007 argued: a
-- refund that cannot be traced to the document it corrects is not auditable.
-- A purchase has no sale to point at, which is why the reasons split by which
-- document they carry instead of by "document or initial".
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_document_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_document_coherent
    CHECK (
        (reason IN ('sale', 'credit_note') AND sale_id IS NOT NULL)
        OR (reason = 'purchase' AND purchase_id IS NOT NULL)
        OR (reason = 'initial' AND sale_id IS NULL AND purchase_id IS NULL)
    );

ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_purchase_coherent;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_purchase_coherent
    CHECK ((reason = 'purchase') = (purchase_id IS NOT NULL));

-- The direction of a purchase movement is not a style preference: stock comes in.
-- The ledger accepts negative quantities (a sale is one), so without this a
-- purchase could silently remove stock while the purchase document says it
-- arrived — the exact class of invisible inconsistency the append-only ledger
-- exists to prevent.
ALTER TABLE stock_movements
    DROP CONSTRAINT IF EXISTS stock_movements_purchase_is_incoming;
ALTER TABLE stock_movements
    ADD CONSTRAINT stock_movements_purchase_is_incoming
    CHECK (reason <> 'purchase' OR quantity > 0);

-- 0007's defensive check, and it matters more here than it did there: this
-- development database carries real sales and credit notes, so a constraint that
-- the existing data already violates would otherwise be added and then fail on
-- the next legitimate insert, blaming the wrong writer.
DO $$
DECLARE
    incoherent bigint;
BEGIN
    SELECT count(*) INTO incoherent FROM stock_movements
     WHERE (reason IN ('sale', 'credit_note') AND sale_id IS NULL)
        OR (reason = 'initial' AND (sale_id IS NOT NULL OR purchase_id IS NOT NULL))
        OR (reason NOT IN ('sale', 'credit_note', 'initial') AND sale_id IS NULL AND purchase_id IS NULL);
    IF incoherent > 0 THEN
        RAISE EXCEPTION
            'stock_movements has % row(s) that contradict the v2 coherence rule; fix them before migrating',
            incoherent;
    END IF;
END;
$$;

CREATE INDEX IF NOT EXISTS stock_movements_purchase_id_idx
    ON stock_movements (purchase_id);

-- ── Immutability guards ─────────────────────────────────────────────────────
--
-- Written correctly the first time, which is 0008's lesson learned in advance:
-- 0007 shipped a blanket reject function applied only to TRUNCATE, so UPDATE and
-- DELETE succeeded while the docs claimed they did not. Both operations are
-- guarded here from the start.
--
-- Unlike `sales`, there is no permitted UPDATE: a purchase has no status and no
-- reversed_at, so its economics and its timestamps are equally final.

CREATE OR REPLACE FUNCTION reject_purchase_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'purchases are immutable: a correction is a compensating document, not an edit';
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION reject_purchase_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'purchase_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS purchases_immutable ON purchases;
CREATE TRIGGER purchases_immutable
BEFORE UPDATE OR DELETE ON purchases
FOR EACH ROW EXECUTE FUNCTION reject_purchase_mutation();

DROP TRIGGER IF EXISTS purchases_no_truncate ON purchases;
CREATE TRIGGER purchases_no_truncate
BEFORE TRUNCATE ON purchases
FOR EACH STATEMENT EXECUTE FUNCTION reject_purchase_mutation();

DROP TRIGGER IF EXISTS purchase_items_immutable ON purchase_items;
CREATE TRIGGER purchase_items_immutable
BEFORE UPDATE OR DELETE ON purchase_items
FOR EACH ROW EXECUTE FUNCTION reject_purchase_item_mutation();

DROP TRIGGER IF EXISTS purchase_items_no_truncate ON purchase_items;
CREATE TRIGGER purchase_items_no_truncate
BEFORE TRUNCATE ON purchase_items
FOR EACH STATEMENT EXECUTE FUNCTION reject_purchase_item_mutation();

-- 0008 ends with an assertion that its guard actually exists, because a silently
-- missing trigger is exactly the failure it was written to repair. The same check
-- belongs here, and it is cheap: four triggers, one query.
--
-- It runs LAST, after the triggers it checks are created, because that is the
-- only position where a failure names the real cause. This migration was first
-- written with the check in the middle, and the measurement was unambiguous: a
-- re-run reported the *constraint* that happened to fail next instead of the
-- guard that was actually absent. A diagnostic that names the wrong cause costs
-- more than no diagnostic, so the check sits after every CREATE TRIGGER.
DO $$
DECLARE
    present bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('purchases', 'purchases_immutable'),
              ('purchases', 'purchases_no_truncate'),
              ('purchase_items', 'purchase_items_immutable'),
              ('purchase_items', 'purchase_items_no_truncate')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 4 THEN
        RAISE EXCEPTION
            'expected 4 v2 immutability triggers, found %; refusing to continue', present;
    END IF;
END;
$$;
