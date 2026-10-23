-- v3 (part 2 of 2): the four documents learn where the money went.
--
-- **This half is the data rewrite**, and it is separate because it carries a
-- different risk from `0012`: it adds a NOT NULL column to four populated tables,
-- fills it for every row already in the database, and drops the column it replaces.
-- Reviewing "did the vocabulary and its guards survive?" and "did the backfill
-- rewrite history correctly?" are two questions, so they are two files.
--
-- **The backfill cannot invent a destination.** `0002`'s `DEFAULT 'cash'` is the
-- proof that every row already in these tables means "the till", so pointing them at
-- the seeded `cash` method states a fact rather than making one up.
--
-- **Why `ADD COLUMN ... NOT NULL DEFAULT` and not a four-step backfill.** Measured:
-- `0007`, `0009` and `0010` guard `credit_notes`, `purchases` and `supplier_returns`
-- with triggers that reject `UPDATE` outright
-- (`credit_notes are immutable: corrections are compensating rows, not edits`), so
-- `ADD COLUMN nullable` -> `UPDATE` -> `SET NOT NULL` fails on three of four tables.
-- Disabling those triggers for the length of a migration is the wrong trade: they are
-- the guards most worth trusting, and switching them off while history is written is
-- exactly what this repository refuses. `ADD COLUMN ... NOT NULL DEFAULT` fills
-- existing rows as a catalog change — no row is updated, no trigger fires — and the
-- `DEFAULT` is dropped immediately afterwards so it cannot become a second way of
-- writing a document.

-- ── The four documents learn where the money went ───────────────────────────

-- Added nullable, backfilled, then set NOT NULL. The order is not stylistic: a
-- `NOT NULL` on a populated table fails immediately, and the backfill cannot
-- invent a destination — it records the only one that existed. `0002`'s
-- `DEFAULT 'cash'` is the proof that every row already in these tables means "the
-- till", so pointing them at the seeded `cash` method states a fact.
--
-- The alternative — leaving the column nullable so old history has no
-- destination — was refused: it would create two classes of document and every
-- consumer would need two code paths forever.
--
-- **Why `ADD COLUMN ... NOT NULL DEFAULT` in one statement instead of an
-- `UPDATE` backfill.** Measured while writing this file: the `UPDATE` form fails
-- on three of the four tables, because `0007`, `0009` and `0010` guard them with
-- triggers that reject `UPDATE` outright (`credit_notes are immutable: corrections
-- are compensating rows, not edits`). Disabling those triggers for the length of a
-- migration would mean the guards most worth trusting are exactly the ones switched
-- off while history is rewritten. `ADD COLUMN ... NOT NULL DEFAULT` fills existing
-- rows as a catalog change — no row is updated, no trigger fires — which is both
-- safer and the reason the guards stay untouched.
--
-- The `DEFAULT` is dropped immediately afterwards so it cannot become a second way
-- of writing a document: after this point a new row with no destination is refused,
-- and the write paths name the method explicitly.
ALTER TABLE sales
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE credit_notes
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE purchases
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);
ALTER TABLE supplier_returns
    ADD COLUMN IF NOT EXISTS payment_method_id UUID NOT NULL
        DEFAULT '00000000-0000-0000-0000-000000000004'
        REFERENCES payment_methods (id);

ALTER TABLE sales            ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE credit_notes     ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE purchases        ALTER COLUMN payment_method_id DROP DEFAULT;
ALTER TABLE supplier_returns ALTER COLUMN payment_method_id DROP DEFAULT;

-- The view joins documents to methods on these columns, and the balance of one
-- account reads through that join. Without them the per-branch `account_id`
-- predicate cannot be pushed down.
CREATE INDEX IF NOT EXISTS sales_payment_method_id_idx            ON sales (payment_method_id);
CREATE INDEX IF NOT EXISTS credit_notes_payment_method_id_idx     ON credit_notes (payment_method_id);
CREATE INDEX IF NOT EXISTS purchases_payment_method_id_idx        ON purchases (payment_method_id);
CREATE INDEX IF NOT EXISTS supplier_returns_payment_method_id_idx ON supplier_returns (payment_method_id);

-- ── Retire `sales.payment_method`, and re-cut the guard that froze it ───────

-- The old TEXT column held one value and nothing wrote it. Keeping it beside the
-- FK would be exactly the dead vocabulary this slice rejects elsewhere, and the
-- `C: hybrid` wire shape was rejected for the same reason: two sources of the
-- same name, one of which goes stale.
--
-- **The guard must be re-cut in this same file, and this is not optional.**
-- `guard_sale_economics` (`0003`) compares `NEW.payment_method <> OLD.payment_method`.
-- A plpgsql body is not resolved when it is created, so dropping the column leaves
-- the function in place and *valid-looking* until the next UPDATE, when it fails at
-- runtime with `column "payment_method" does not exist`. Worse, if it were simply
-- dropped from the comparison, a recorded sale's destination could be rewritten
-- after the fact — paying 25.00 at the till and later moving the money to the bank
-- statement, which is the history rewrite this repository refuses everywhere.
--
-- `IS DISTINCT FROM` rather than `<>`, deliberately: `NULL <> value` is `NULL`, and
-- an `IF` whose condition is `NULL` does not fire. Today both columns are NOT NULL,
-- so the two spellings behave identically — but the guard's whole job is to keep
-- holding when a future migration relaxes that, and a guard that silently stops
-- guarding is the defect `0011` was written to kill.
ALTER TABLE sales DROP COLUMN IF EXISTS payment_method;

CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total IS DISTINCT FROM OLD.total
       OR NEW.customer_id IS DISTINCT FROM OLD.customer_id
       OR NEW.payment_method_id IS DISTINCT FROM OLD.payment_method_id THEN
        RAISE EXCEPTION 'sale % economics are immutable: status may change, money may not', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── A method's account is immutable once history names it ───────────────────

-- Moving a method between accounts would rewrite yesterday's balance today: the
-- document still says "this method", the method now says "another account", and the
-- account's historical total changes without a single document being written.
--
-- The condition is **history**, not the field: a method no document names is a typo
-- and must stay correctable, or a method attached to the wrong account could never
-- be fixed. That is also why `name` and `is_active` stay editable — the identity
-- that must not move is which account the money lands in, not the spelling.
CREATE OR REPLACE FUNCTION guard_payment_method_account() RETURNS trigger AS $$
BEGIN
    IF NEW.account_id IS DISTINCT FROM OLD.account_id THEN
        IF EXISTS (SELECT 1 FROM sales            WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM credit_notes     WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM purchases        WHERE payment_method_id = OLD.id)
        OR EXISTS (SELECT 1 FROM supplier_returns WHERE payment_method_id = OLD.id) THEN
            RAISE EXCEPTION
                'payment method % cannot move to another account: documents already name it',
                OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payment_methods_account_guard ON payment_methods;
CREATE TRIGGER payment_methods_account_guard
BEFORE UPDATE OF account_id ON payment_methods
FOR EACH ROW EXECUTE FUNCTION guard_payment_method_account();

-- ── Every existing document satisfies the new rule ──────────────────────────

-- The NOT NULL above proves the column is populated; the foreign key proves it
-- points at a method; this names the rule the slice actually claims — every document
-- says where the money moved — and its failure names the data rather than a trigger.
DO $$
DECLARE
    stranded bigint;
BEGIN
    SELECT count(*) INTO stranded
      FROM (
            SELECT payment_method_id FROM sales
            UNION ALL SELECT payment_method_id FROM credit_notes
            UNION ALL SELECT payment_method_id FROM purchases
            UNION ALL SELECT payment_method_id FROM supplier_returns
           ) AS documents
      LEFT JOIN payment_methods pm ON pm.id = documents.payment_method_id
     WHERE pm.id IS NULL;

    IF stranded > 0 THEN
        RAISE EXCEPTION
            'v3: % document(s) name a payment method that does not exist; refusing to continue',
            stranded;
    END IF;
END;
$$;

-- ── Prove this half's guard exists ──────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_trigger
         WHERE tgname = 'payment_methods_account_guard'
           AND tgrelid = 'payment_methods'::regclass
           AND NOT tgisinternal
    ) THEN
        RAISE EXCEPTION
            'payment_methods_account_guard is missing; refusing to continue';
    END IF;
END;
$$;

