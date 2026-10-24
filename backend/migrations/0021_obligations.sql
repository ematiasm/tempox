-- v4 S6a: what we are owed is a document, not a wallet.
--
-- `odd/tasks/v4-receivables.md` S1-S3, S7, S8. This is the slice where `credit` stops
-- being an account, and the first half of it is a **subtraction from the book**.
--
-- The defect, restated because the schema below is the answer to it: `credit` is not a
-- wallet. It is an obligation wearing an account's clothes — it holds nothing, and it
-- answers "what am I owed" while pretending to answer "where is the money"
-- (`v4-settlement.md`, the diagnosis). Until now a sale on account wrote
-- `credit +total` into `money_entries`, which means the debt was recorded as a money
-- movement into an account that never received money.
--
-- Two consequences shape everything here:
--
--   * **A sale on account moves no money.** It writes an obligation and **zero** ledger
--     entries, so `sales.payment_method_id` becomes NULL for it — and the NULL is a
--     claim that a constraint trigger holds, never an arbitrary null.
--   * **Lowering what is owed is a document too.** A credit note on an unpaid sale used
--     to inherit `credit` and write `-total`, which is how a refund of a sale nobody paid
--     took real money out of the till (`v3-accounts.md` L107). Now it inherits the
--     sale's NULL, writes no entry, and lowers the debt through an
--     `obligations_adjustments` row.

-- ── obligations: the debt itself ───────────────────────────────────────────
--
-- `source_document_kind` is closed to `sale` for now, deliberately: an unrecognised
-- source is a debt whose cause cannot be named, exactly as an unrecognised
-- `money_entries.document_kind` is (`0017`). T7 adds `purchase` by `ALTER`, the same way
-- `0018` and `0020` extended their sets — reopening the column would lose the guarantee.
--
-- **No foreign key on `source_document_id`, and this is not laziness.** It points at
-- `sales` today and at `purchases` after T7, and PostgreSQL has no polymorphic FK. The
-- trigger below closes the gap by deriving the party from the document it names, the
-- technique `0011` established for `stock_movements` and `0017` for the book.
--
-- **No `amount > 0` alone.** A sale of nothing but bonus lines has `total = 0` (`0005`),
-- and `0017` learned this the expensive way: `CHECK (amount <> 0)` on the book turned a
-- valid bonus sale into a 500. A zero obligation is a real obligation that is settled the
-- moment it exists, so the CHECK is `>= 0`.
CREATE TABLE IF NOT EXISTS obligations (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_document_kind TEXT NOT NULL,
    source_document_id   UUID NOT NULL,
    party_kind           TEXT NOT NULL,
    party_id             UUID NOT NULL,
    amount               NUMERIC(14, 6) NOT NULL CHECK (amount >= 0),
    occurred_at          TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by           TEXT NOT NULL,

    CONSTRAINT obligations_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT obligations_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT obligations_source_known CHECK (source_document_kind IN ('sale')),
    CONSTRAINT obligations_party_known CHECK (party_kind IN ('customer', 'supplier'))
);

-- **One debt per document, and the UNIQUE is the rule rather than a convention.** A
-- second obligation naming the same sale would make "what is owed" a sum of two rows
-- nobody can tell apart, which is the same defect as two records of one debt.
CREATE UNIQUE INDEX IF NOT EXISTS obligations_source_unique
    ON obligations (source_document_kind, source_document_id);

CREATE INDEX IF NOT EXISTS obligations_party_idx
    ON obligations (party_kind, party_id);

-- ── obligations_adjustments: the debt shrinking ────────────────────────────
--
-- `S8`. A credit note that inherits the sale's NULL method moves no money and lowers
-- what is owed, and a cancellation lowers it too. **The alternative was a mutable
-- `obligations.amount` behind a `tpox.*_writer` flag** — the pattern `0019` used for
-- `sale_items.reversed_quantity`, which then cost a CRITICAL finding in the review of
-- `4ffb95e` (`v4-settlement.md` V7, `R3-002`). The honest shape exists here, so the flag
-- is not bought again: the debt drops through a row, and the balance stays a subtraction
-- of stored rows.
--
-- So the outstanding figure is
--
--     obligations.amount - SUM(settlements) - SUM(obligations_adjustments)
--
-- and a partial drop is the normal case instead of a special one.
CREATE TABLE IF NOT EXISTS obligations_adjustments (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    obligation_id UUID NOT NULL REFERENCES obligations (id),
    -- Positive, with the sign carried by the table: an adjustment always lowers the
    -- debt, and a column that sometimes adds is a column every reader must re-derive.
    amount        NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    document_kind TEXT NOT NULL,
    document_id   UUID NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by    TEXT NOT NULL,

    CONSTRAINT obligations_adjustments_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT obligations_adjustments_created_by_not_blank
        CHECK (btrim(created_by) <> ''),
    CONSTRAINT obligations_adjustments_kind_known
        CHECK (document_kind IN ('credit_note', 'cancellation'))
);

-- One adjustment per document: a credit note lowers the debt exactly once, and a second
-- row naming the same note would be the same correction counted twice.
CREATE UNIQUE INDEX IF NOT EXISTS obligations_adjustments_document_unique
    ON obligations_adjustments (document_kind, document_id);

CREATE INDEX IF NOT EXISTS obligations_adjustments_obligation_idx
    ON obligations_adjustments (obligation_id);

-- **The ceiling, adjusted side only.** `SUM(adjustments) <= amount`, checked at write
-- time. `0022` re-cuts this function to include settlements, because the combined ceiling
-- is the rule and the two halves cannot be judged apart — an obligation is not
-- oversettled or over-adjusted, it is over-*resolved*. Until settlements exist, the
-- adjustment half is the whole rule.
CREATE OR REPLACE FUNCTION obligation_adjustments_within_amount() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
BEGIN
    -- **`FOR UPDATE` on the obligation row, and it is the whole guarantee.** Without it two
    -- concurrent adjustments of the same obligation each read the same committed sum, each
    -- pass, and together lower the debt past zero — leaving a negative outstanding figure
    -- that no route can produce but `psql` can (review `R1-ceiling-race` / `R4-ceiling-trigger-reads-without-locking`).
    -- Measured reachable: two sessions, no lock, both inserts accepted.
    --
    -- The row lock is taken on `obligations`, which every adjustment of that obligation
    -- must therefore pass through in order. The HTTP routes already serialise their own
    -- work, so this is the defence for the surface this trigger exists to govern at all.
    SELECT amount INTO owed FROM obligations WHERE id = NEW.obligation_id FOR UPDATE;

    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM obligations_adjustments
     WHERE obligation_id = NEW.obligation_id
       AND id <> NEW.id;

    IF lowered + NEW.amount > owed THEN
        RAISE EXCEPTION
            'obligation % is for %, but % + % would lower it past zero',
            NEW.obligation_id, owed, lowered, NEW.amount;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_adjustments_ceiling ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_ceiling
BEFORE INSERT ON obligations_adjustments
FOR EACH ROW EXECUTE FUNCTION obligation_adjustments_within_amount();

-- ── Append-only, the same three guards every book keeps ────────────────────
--
-- A row-level guard never sees `TRUNCATE`, because it is a set operation and does not
-- fire FOR EACH ROW. `0007` shipped that mistake and `0008` repaired it; both guards go
-- in from the start, as `0017` and `0018` do.
CREATE OR REPLACE FUNCTION reject_obligation_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_append_only ON obligations;
CREATE TRIGGER obligations_append_only
BEFORE UPDATE OR DELETE ON obligations
FOR EACH ROW EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_no_truncate ON obligations;
CREATE TRIGGER obligations_no_truncate
BEFORE TRUNCATE ON obligations
FOR EACH STATEMENT EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_adjustments_append_only ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_append_only
BEFORE UPDATE OR DELETE ON obligations_adjustments
FOR EACH ROW EXECUTE FUNCTION reject_obligation_mutation();

DROP TRIGGER IF EXISTS obligations_adjustments_no_truncate ON obligations_adjustments;
CREATE TRIGGER obligations_adjustments_no_truncate
BEFORE TRUNCATE ON obligations_adjustments
FOR EACH STATEMENT EXECUTE FUNCTION reject_obligation_mutation();

-- ── The sale learns to say "this moved no money" ───────────────────────────
--
-- `S2`, and this is `v4-settlement.md` S4 pulled forward from T8. **The reason is
-- measured, not stylistic.** `0017` put a plain FK from `money_entries.payment_method_id`
-- to `payment_methods.id`, `0012` refuses `DELETE` on both tables, and `0013`'s
-- `payment_methods_account_guard` refuses a real change of `account_id` once a document
-- names the method. S7 promises to retire `credit`; that delete is clean **only while no
-- ledger row names it**, and today none does — the method is seeded `is_active = false`
-- and `sales.rs` reaches it only when a caller asks for it explicitly. A sale on account
-- that wrote `credit +total` would turn S7 into a rewrite of an append-only book.
--
-- **The NULL is a claim, and the trigger below is what holds it.** A nullable column with
-- no rule is an arbitrary null, which is why the two states are enforced together.
ALTER TABLE sales ALTER COLUMN payment_method_id DROP NOT NULL;

-- ── The two states of a sale, checked at commit ────────────────────────────
--
-- `S2`:
--
--     payment_method_id IS NULL      ->  exactly one obligation names this sale
--     payment_method_id IS NOT NULL  ->  zero obligations, and at least one ledger entry
--
-- **A constraint trigger, and `DEFERRABLE INITIALLY DEFERRED` is not optional.** The sale
-- is inserted before the obligation that names it (an obligation needs the sale's id) and
-- the ledger entry is written after the sale is read back, so a plain `AFTER INSERT` would
-- refuse the correct order. Deferring to COMMIT is what lets one transaction build a
-- coherent set of rows and judge it once. That is the same mechanism `0016` used for
-- `accounts_exactly_one_default`, and it carries the same trap, recorded there: **no
-- `ALTER TABLE` on this table may follow a row-touching statement in the same
-- transaction**, because a pending trigger event blocks it.
--
-- `IS DISTINCT FROM` over the pair, not a chain of `<>`: the guard's job is to keep
-- holding when a future migration relaxes something else.
CREATE OR REPLACE FUNCTION sale_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    owed  bigint;
    moved bigint;
BEGIN
    SELECT count(*) INTO owed
      FROM obligations
     WHERE source_document_kind = 'sale' AND source_document_id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'sale' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF owed <> 1 THEN
            RAISE EXCEPTION
                'sale % says it moved no money, but % obligation(s) name it; a sale on account has exactly one',
                NEW.id, owed;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'sale % says it moved no money, but % ledger entr(ies) name it',
                NEW.id, moved;
        END IF;
    ELSE
        IF owed <> 0 THEN
            RAISE EXCEPTION
                'sale % names payment method %, but % obligation(s) also name it; money moved or debt arose, never both',
                NEW.id, NEW.payment_method_id, owed;
        END IF;
        IF moved = 0 THEN
            RAISE EXCEPTION
                'sale % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sales_payment_method_claimed ON sales;
CREATE CONSTRAINT TRIGGER sales_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON sales
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION sale_payment_method_is_claimed();

-- ── The obligation can only name a sale that claims it ────────────────────
--
-- The other half of the pair. Without it the sale-side trigger could be satisfied by an
-- obligation that names a *different* sale, and the party would be free to disagree with
-- the document: a debt attributed to a customer who never bought anything.
--
-- `S5` of `v4-settlement.md`: the party is derived from the document, never trusted from
-- the caller. A sale's debt belongs to the sale's customer, full stop.
CREATE OR REPLACE FUNCTION obligation_matches_its_sale() RETURNS trigger AS $$
DECLARE
    claim_method UUID;
    sale_party   UUID;
BEGIN
    IF NEW.source_document_kind <> 'sale' THEN
        -- Nothing else is reachable yet; T7 adds the purchase branch beside this one.
        RAISE EXCEPTION
            'obligation source kind % has no rule yet', NEW.source_document_kind;
    END IF;

    SELECT s.payment_method_id, s.customer_id
      INTO claim_method, sale_party
      FROM sales s
     WHERE s.id = NEW.source_document_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'obligation names sale % which does not exist', NEW.source_document_id;
    END IF;

    IF claim_method IS NOT NULL THEN
        RAISE EXCEPTION
            'sale % names payment method %: money moved, so it owes nothing',
            NEW.source_document_id, claim_method;
    END IF;

    IF NEW.party_kind <> 'customer' OR NEW.party_id IS DISTINCT FROM sale_party THEN
        RAISE EXCEPTION
            'obligation on sale % must belong to that sale''s customer %, not % %',
            NEW.source_document_id, sale_party, NEW.party_kind, NEW.party_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS obligations_match_their_sale ON obligations;
CREATE CONSTRAINT TRIGGER obligations_match_their_sale
AFTER INSERT ON obligations
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION obligation_matches_its_sale();

-- ── A credit note on an unpaid sale moves no money ────────────────────────
--
-- `S3`, and the sign is the whole point. A note that inherits the sale's NULL method must
-- write **no** ledger entry and exactly one adjustment; a note that names a method must
-- write a ledger entry and no adjustment. Before this, the inherited path wrote `-total`
-- against `credit`, which is how `L107`'s refund of a sale nobody paid took real money out
-- of the till.
--
-- **A note may be NULL only when its sale is NULL.** A note on a paid sale that moved no
-- money would be a credit balance with no payment behind it — money from nowhere. The
-- refund-a-balance case is a `payments` row with `direction = out` (`S4`), not a note.
ALTER TABLE credit_notes ALTER COLUMN payment_method_id DROP NOT NULL;

CREATE OR REPLACE FUNCTION credit_note_moves_money_or_lowers_debt()
RETURNS trigger AS $$
DECLARE
    sale_method UUID;
    moved       bigint;
    lowered     bigint;
BEGIN
    SELECT s.payment_method_id INTO sale_method
      FROM credit_notes cn JOIN sales s ON s.id = cn.sale_id
     WHERE cn.id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'credit_note' AND document_id = NEW.id;

    SELECT count(*) INTO lowered
      FROM obligations_adjustments
     WHERE document_kind = 'credit_note' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF sale_method IS NOT NULL THEN
            RAISE EXCEPTION
                'credit note % says it moved no money, but its sale % names payment method %',
                NEW.id, NEW.sale_id, sale_method;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'credit note % says it moved no money but wrote % ledger entr(ies)',
                NEW.id, moved;
        END IF;
        IF lowered <> 1 THEN
            RAISE EXCEPTION
                'credit note % moves no money, so it must lower exactly one obligation; % found',
                NEW.id, lowered;
        END IF;
    ELSE
        IF moved = 0 THEN
            RAISE EXCEPTION
                'credit note % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
        IF lowered <> 0 THEN
            RAISE EXCEPTION
                'credit note % names payment method %: money went back, so it lowers no debt',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS credit_notes_claim_their_effect ON credit_notes;
CREATE CONSTRAINT TRIGGER credit_notes_claim_their_effect
AFTER INSERT ON credit_notes
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION credit_note_moves_money_or_lowers_debt();

-- ── The book keeps refusing to name the debt account ──────────────────────
--
-- `S1` read backwards: since no sale on account writes a ledger entry, nothing in the book
-- may ever point at the debt account again. This trigger is what makes S7's delete of
-- `credit` a delete rather than a data migration, and it is honest about its own limit —
-- **it does not clean up rows written before it existed**, because the book is append-only
-- and rewriting it is exactly what this repository refuses. The `DO` block below refuses to
-- continue if such rows exist, naming the remedy instead of silently producing two
-- generations of credit history.
CREATE OR REPLACE FUNCTION money_entry_never_names_the_debt_account() RETURNS trigger AS $$
DECLARE
    debt UUID;
BEGIN
    SELECT a.id INTO debt
      FROM payment_methods pm JOIN accounts a ON a.id = pm.account_id
     WHERE pm.id = NEW.payment_method_id
       AND a.name = 'credit';

    IF debt IS NOT NULL THEN
        RAISE EXCEPTION
            'money entry % names a method on the debt account: debt is a document (obligations), not a balance',
            NEW.id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS money_entries_never_name_the_debt ON money_entries;
CREATE TRIGGER money_entries_never_name_the_debt
BEFORE INSERT ON money_entries
FOR EACH ROW EXECUTE FUNCTION money_entry_never_names_the_debt_account();

-- ── No backfill, and that is a decision rather than an omission ──────────
--
-- **This migration does not migrate pre-existing credit sales, and the owner decided why:**
-- the application has never shipped, there is no production database, and the only databases
-- that exist are the owner's own test data, droppable on request.
--
-- The first version of this file tried to migrate them anyway, and it was **broken in two
-- ways at once**, measured on a database at `0020` holding one legacy credit sale:
--
--   1. `UPDATE sales SET payment_method_id = NULL` collided with `0013`'s
--      `guard_sale_economics`, which refuses **any** change to that column:
--      *"sale … economics are immutable: a correction is a credit note, never an edit"*.
--      The migration aborted before reaching the check below.
--   2. Even with the guard relaxed, the ledger rows it leaves behind cannot be removed —
--      `money_entries` is append-only — so the check at the end would refuse the whole
--      migration anyway, asking a human to decide the fate of rows no route can delete.
--
-- An empty database never reaches either failure, which is why 119 tests and every CI run
-- passed: **they all start from nothing.** That is `v4-settlement.md` V1b's own warning —
-- "doing it on an empty database would have proven nothing" — and this slice ignored it.
--
-- **Removing the backfill is the honest fix, not a shortcut.** A backfill carries live data
-- across a schema change; there is none. Keeping a broken one and disabling the check that
-- exposes it would leave two generations of credit history in any database someone did keep.
-- What replaces it is a refusal naming the count and the remedy — a `DROP`, the owner's
-- choice. Re-measured: an empty database applies in 17ms, and one holding a legacy credit
-- sale refuses once, by design.

-- ── Prove the guards exist, and that no legacy data is in the way ─────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present bigint;
    legacy  bigint;
    unclaimed bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('obligations', 'obligations_append_only'),
              ('obligations', 'obligations_no_truncate'),
              ('obligations', 'obligations_match_their_sale'),
              ('obligations_adjustments', 'obligations_adjustments_append_only'),
              ('obligations_adjustments', 'obligations_adjustments_no_truncate'),
              ('obligations_adjustments', 'obligations_adjustments_ceiling'),
              ('sales', 'sales_payment_method_claimed'),
              ('credit_notes', 'credit_notes_claim_their_effect'),
              ('money_entries', 'money_entries_never_name_the_debt')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 9 THEN
        RAISE EXCEPTION
            'expected 9 v4-receivables guards, found %; refusing to continue', present;
    END IF;

    -- The columns really are nullable, which no trigger would reveal.
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'sales' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'sales.payment_method_id is still NOT NULL; refusing to continue';
    END IF;
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'credit_notes' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'credit_notes.payment_method_id is still NOT NULL; refusing to continue';
    END IF;

    -- **No legacy data may be in the way, and the remedy is a DROP** — the owner's
    -- decision: nothing has shipped, so no database holds history worth carrying. The
    -- message says that instead of asking for a judgement call the project has already
    -- made.
    SELECT count(*) INTO legacy
      FROM money_entries me
      JOIN payment_methods pm ON pm.id = me.payment_method_id
      JOIN accounts a         ON a.id = pm.account_id
     WHERE a.name = 'credit';
    IF legacy > 0 THEN
        RAISE EXCEPTION
            '% money_entries row(s) name the debt account, from a credit sale recorded before this slice. This migration does not migrate them: the application has never shipped, so the remedy is to drop the development database and start from empty ("docker compose down -v"). If a database ever does hold history worth keeping, replace this refusal with a real backfill — and relax guard_sale_economics for exactly the payment_method_id to NULL transition, which the first attempt at this file did not.',
            legacy;
    END IF;

    -- **Both halves of the two-state rule, because the comment promised both** (`R2-002`
    -- of the review: the first version checked only the obligation half, so a sale with a
    -- non-NULL method and no ledger entry — or a NULL sale with an entry elsewhere — would
    -- pass here and be refused at its next write).
    SELECT count(*) INTO unclaimed
      FROM sales s
     WHERE (s.payment_method_id IS NULL)
        <> (EXISTS (SELECT 1 FROM obligations o
                     WHERE o.source_document_kind = 'sale' AND o.source_document_id = s.id))
        OR (s.payment_method_id IS NULL AND EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'sale' AND me.document_id = s.id))
        OR (s.payment_method_id IS NOT NULL AND NOT EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'sale' AND me.document_id = s.id));
    IF unclaimed > 0 THEN
        RAISE EXCEPTION
            '% sale(s) disagree with their obligations or their ledger entries about whether money moved; refusing to continue',
            unclaimed;
    END IF;
END;
$$;
