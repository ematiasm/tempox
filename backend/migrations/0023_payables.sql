-- v4 S6b: what we owe is a document, and paying a supplier settles it.
--
-- `odd/tasks/v4-payables.md` S1-S13, the mirror of `0021`/`0022` on the purchase side.
--
-- The defect it closes is `v4-settlement.md` L19, and it is `v3-accounts.md` L107 mirrored:
-- `supplier_returns` inherits the purchase's method with no override, and **that inheritance
-- is only true for a purchase that was paid**. Returning goods on an unpaid purchase credits
-- the debt; it does not hand money back. Until this file there was no way to say "we owe the
-- supplier" at all — `purchases.rs` refuses the debt method outright (`S9b` of
-- `v3-accounts.md`: *"there is no purchase-side counterpart to a credit sale until the
-- payable document exists"*).
--
-- ── Why three new tables instead of extending `obligations` ─────────────────
--
-- `v4-settlement.md` S5/S6 describe the supplier side as an extension of `obligations` and
-- `settlements`, and the owner decided otherwise (`v4-settlement.md` L25, `v4-payables.md`
-- L2-L4). **This file therefore contains no statement whose target is an object `0021` or
-- `0022` created**: no `ALTER` over a closed set, no `CREATE OR REPLACE` of either ceiling
-- function. Those two operations are exactly what has already failed twice in this feature —
-- `0022` re-cut `obligation_adjustments_within_amount` and silently dropped the `FOR UPDATE`
-- `0021` had added (`R3-004`, CRITICAL), and its `money_entries_kind_known` rewrite silently
-- dropped `annulment`, which turned every annulment into a 500.
--
-- **What that costs, stated rather than discovered later:** the ceiling below is a second copy
-- of `settlement_within_both_ceilings`. Same rule, two functions, so the two must move
-- together if the rule ever changes. `obligations.party_kind = 'supplier'` stays declared and
-- unused, because removing it would be the rewrite being avoided. `S13` of the feature
-- document makes "no edits to `0021`/`0022`" testable rather than aspirational.

-- ── payables: the debt itself ──────────────────────────────────────────────
--
-- `source_document_kind` is closed to `purchase`, deliberately and by the same argument
-- `0021` gives for `sale`: an unrecognised source is a debt whose cause cannot be named.
--
-- **No foreign key on `source_document_id`**, for `0021`'s reason: it is polymorphic by
-- design, PostgreSQL has none, and the trigger below closes the gap by deriving the party
-- from the document — the technique `0011` established for `stock_movements`.
--
-- **`amount >= 0`, not `> 0`.** `0009` allows `total = 0` (free stock in, a bonus line), and
-- `0017` learned this the expensive way: `CHECK (amount <> 0)` on the book turned a valid
-- bonus sale into a 500. A zero payable is a real payable, settled the moment it exists.
CREATE TABLE IF NOT EXISTS payables (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_document_kind TEXT NOT NULL,
    source_document_id   UUID NOT NULL,
    party_kind           TEXT NOT NULL,
    party_id             UUID NOT NULL,
    amount               NUMERIC(14, 6) NOT NULL CHECK (amount >= 0),
    occurred_at          TIMESTAMPTZ NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by           TEXT NOT NULL,

    CONSTRAINT payables_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payables_created_by_not_blank CHECK (btrim(created_by) <> ''),
    CONSTRAINT payables_source_known CHECK (source_document_kind IN ('purchase')),
    CONSTRAINT payables_party_known CHECK (party_kind IN ('supplier'))
);

-- **One debt per document, and the UNIQUE is the rule.** A second payable naming the same
-- purchase would make "what is owed" a sum of two rows nobody can tell apart.
CREATE UNIQUE INDEX IF NOT EXISTS payables_source_unique
    ON payables (source_document_kind, source_document_id);

CREATE INDEX IF NOT EXISTS payables_party_idx
    ON payables (party_kind, party_id);

-- ── payables_adjustments: the debt shrinking ───────────────────────────────
--
-- `S5`. A supplier return that inherits the purchase's NULL method moves no money and
-- lowers what is owed.
--
-- **The alternative was a mutable `payables.amount` behind a `tpox.*_writer` flag** — the
-- pattern `0019` used for `sale_items.reversed_quantity`, which cost the CRITICAL finding
-- `R3-002` in the review of `4ffb95e` (`v4-settlement.md` V7). The honest shape exists here,
-- so the flag is not bought a second time: the debt drops through a row.
--
-- So the outstanding figure is
--
--     payables.amount - SUM(payable_settlements) - SUM(payables_adjustments)
--
-- and a partial drop is the normal case rather than a special one.
CREATE TABLE IF NOT EXISTS payables_adjustments (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payable_id    UUID NOT NULL REFERENCES payables (id),
    -- Positive, with the sign carried by the table: an adjustment always lowers the debt, and
    -- a column that sometimes adds is a column every reader must re-derive.
    amount        NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    document_kind TEXT NOT NULL,
    document_id   UUID NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by    TEXT NOT NULL,

    CONSTRAINT payables_adjustments_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payables_adjustments_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- `cancellation` is accepted from the start and written by nothing yet: it is the member
    -- `v4-settlement.md` S8 names for the credit-note path's mirror, and accepting it costs one
    -- word while adding it later costs an `ALTER` over a closed set — the operation this file
    -- exists to avoid.
    CONSTRAINT payables_adjustments_kind_known
        CHECK (document_kind IN ('supplier_return', 'cancellation'))
);

-- One adjustment per document: a return lowers the debt exactly once, and a second row naming
-- the same return would be the same correction counted twice.
CREATE UNIQUE INDEX IF NOT EXISTS payables_adjustments_document_unique
    ON payables_adjustments (document_kind, document_id);

CREATE INDEX IF NOT EXISTS payables_adjustments_payable_idx
    ON payables_adjustments (payable_id);

-- ── payable_settlements: which debt this money paid, and how much ──────────
--
-- `S6`. Both ceilings are sums over rows because neither side is limited to one row, exactly
-- as `0022` argues for the customer side (`v4-settlement.md` L18, the owner's own words).
CREATE TABLE IF NOT EXISTS payable_settlements (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payable_id UUID NOT NULL REFERENCES payables (id),
    payment_id UUID NOT NULL REFERENCES payments (id),
    amount     NUMERIC(14, 6) NOT NULL CHECK (amount > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by TEXT NOT NULL,

    CONSTRAINT payable_settlements_whole_cents CHECK (amount = round(amount, 2)),
    CONSTRAINT payable_settlements_created_by_not_blank CHECK (btrim(created_by) <> ''),
    -- **Two rows from the same payment to the same debt are one row written twice.** They
    -- would pass both ceilings while double-counting, and their sum says exactly what a single
    -- row of that sum would say. `0022` learned this by writing one and being refused, and the
    -- test that did it was wrong rather than the index. "Paid in parts" means across
    -- **payments**.
    CONSTRAINT payable_settlements_one_row_per_pair UNIQUE (payable_id, payment_id)
);

CREATE INDEX IF NOT EXISTS payable_settlements_payable_idx
    ON payable_settlements (payable_id);
CREATE INDEX IF NOT EXISTS payable_settlements_payment_idx
    ON payable_settlements (payment_id);

-- ── The two ceilings, in one function and one place ────────────────────────
--
-- The mirror of `settlement_within_both_ceilings`, written once with both terms present.
-- **That is the whole argument for shipping the three tables in one file** (`S12`): `0022`
-- re-cut its equivalent to add the second term and lost a clause doing it.
--
-- The message names both numbers because the caller needs to know **which** ceiling it hit:
-- an over-settled debt is a different correction from an over-applied payment.
CREATE OR REPLACE FUNCTION payable_settlement_within_both_ceilings() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
    paid    NUMERIC(14, 6);
    applied NUMERIC(14, 6);
BEGIN
    -- **Both rows are locked, in ONE order: payment first, then payable.** This is the order
    -- `aa95665` established after a correction inverted it and created the deadlock it was
    -- closing (`R1-settlement-lock-order-inversion` / `R3-001` of
    -- `review-52d3b2c6c4f09589`). The order is not free to choose; it is whatever every writer
    -- agrees on, and the payable settlement route takes them the same way round.
    SELECT p.total  INTO paid FROM payments p WHERE p.id = NEW.payment_id FOR UPDATE;
    SELECT o.amount INTO owed FROM payables o  WHERE o.id = NEW.payable_id FOR UPDATE;

    IF owed IS NULL OR paid IS NULL THEN
        RAISE EXCEPTION
            'payable settlement names payable % and payment %; at least one does not exist',
            NEW.payable_id, NEW.payment_id;
    END IF;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM payable_settlements WHERE payable_id = NEW.payable_id;
    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM payables_adjustments WHERE payable_id = NEW.payable_id;
    SELECT COALESCE(SUM(amount), 0) INTO applied
      FROM payable_settlements WHERE payment_id = NEW.payment_id;

    -- **The two ceilings compose here, and this is the only place they do:** a return lowers
    -- what a payment may still cover, so `settlements + adjustments <= amount`. A debt cannot
    -- be resolved twice — once by a document, once by money.
    IF settled + NEW.amount + lowered > owed THEN
        RAISE EXCEPTION
            'payable % is for %, with % already lowered by document(s); % + % would settle more than is owed',
            NEW.payable_id, owed, lowered, settled, NEW.amount;
    END IF;

    IF applied + NEW.amount > paid THEN
        RAISE EXCEPTION
            'payment % is for %, with % already applied; % + % would apply more than was paid',
            NEW.payment_id, paid, applied, applied, NEW.amount;
    END IF;

    -- **Direction and party, below the route as well as above it** (`R3-005` of
    -- `review-b40b15076ff2c498`). A ceiling enforced only above the database is a suggestion to
    -- everyone who reaches below it. A supplier payable is settled by money that WENT OUT, to
    -- that same supplier — the exact opposite of the customer side, where the money must come
    -- in.
    IF NOT EXISTS (
        SELECT 1 FROM payments WHERE id = NEW.payment_id AND direction = 'out'
    ) THEN
        RAISE EXCEPTION
            'payable settlement % names payment % which is not money going out; only money we sent can pay what we owe',
            NEW.id, NEW.payment_id;
    END IF;

    IF NOT EXISTS (
        SELECT 1
          FROM payables o
          JOIN payments p ON p.id = NEW.payment_id
         WHERE o.id = NEW.payable_id
           AND o.party_kind = p.party_kind
           AND o.party_id = p.party_id
    ) THEN
        RAISE EXCEPTION
            'payable settlement % pays payable % with payment % belonging to a different party',
            NEW.id, NEW.payable_id, NEW.payment_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payable_settlements_within_both_ceilings ON payable_settlements;
CREATE TRIGGER payable_settlements_within_both_ceilings
BEFORE INSERT ON payable_settlements
FOR EACH ROW EXECUTE FUNCTION payable_settlement_within_both_ceilings();

-- ── The adjustment ceiling learns about settlements ────────────────────────
--
-- Unlike `0022`, this one is **written once, with both terms**, because there is no earlier
-- body to re-cut. `0021` wrote the customer version against `amount` alone and `0022` replaced
-- it; that whole sequence is what S12 avoids.
CREATE OR REPLACE FUNCTION payables_adjustments_within_amount() RETURNS trigger AS $$
DECLARE
    owed    NUMERIC(14, 6);
    lowered NUMERIC(14, 6);
    settled NUMERIC(14, 6);
BEGIN
    -- **`FOR UPDATE` on the payable row, and it is the whole guarantee.** Without it two
    -- concurrent adjustments of the same payable each read the same committed sum, each pass,
    -- and together lower the debt past zero — the defect `0021` measured as
    -- `R1-ceiling-race` / `R4-ceiling-trigger-reads-without-locking`, and the clause `0022`
    -- then lost in a re-cut.
    SELECT amount INTO owed FROM payables WHERE id = NEW.payable_id FOR UPDATE;

    SELECT COALESCE(SUM(amount), 0) INTO lowered
      FROM payables_adjustments
     WHERE payable_id = NEW.payable_id AND id <> NEW.id;

    SELECT COALESCE(SUM(amount), 0) INTO settled
      FROM payable_settlements WHERE payable_id = NEW.payable_id;

    IF lowered + settled + NEW.amount > owed THEN
        RAISE EXCEPTION
            'payable % is for %, with % already settled; % + % would lower it past zero',
            NEW.payable_id, owed, settled, lowered, NEW.amount;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_adjustments_ceiling ON payables_adjustments;
CREATE TRIGGER payables_adjustments_ceiling
BEFORE INSERT ON payables_adjustments
FOR EACH ROW EXECUTE FUNCTION payables_adjustments_within_amount();

-- ── Append-only, the same three guards every book keeps ────────────────────
--
-- A row-level guard never sees `TRUNCATE`, because it is a set operation and does not fire
-- FOR EACH ROW. `0007` shipped that mistake and `0008` repaired it; both guards go in from
-- the start, as `0017`, `0018`, `0021` and `0022` do.
CREATE OR REPLACE FUNCTION reject_payable_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION
        '% are append-only: corrections are compensating rows, not edits', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_append_only ON payables;
CREATE TRIGGER payables_append_only
BEFORE UPDATE OR DELETE ON payables
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_no_truncate ON payables;
CREATE TRIGGER payables_no_truncate
BEFORE TRUNCATE ON payables
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_adjustments_append_only ON payables_adjustments;
CREATE TRIGGER payables_adjustments_append_only
BEFORE UPDATE OR DELETE ON payables_adjustments
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payables_adjustments_no_truncate ON payables_adjustments;
CREATE TRIGGER payables_adjustments_no_truncate
BEFORE TRUNCATE ON payables_adjustments
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payable_settlements_append_only ON payable_settlements;
CREATE TRIGGER payable_settlements_append_only
BEFORE UPDATE OR DELETE ON payable_settlements
FOR EACH ROW EXECUTE FUNCTION reject_payable_mutation();

DROP TRIGGER IF EXISTS payable_settlements_no_truncate ON payable_settlements;
CREATE TRIGGER payable_settlements_no_truncate
BEFORE TRUNCATE ON payable_settlements
FOR EACH STATEMENT EXECUTE FUNCTION reject_payable_mutation();

-- ── The purchase learns to say "this moved no money" ───────────────────────
--
-- `S2`. The mirror of `0021`'s `ALTER TABLE sales`, and it carries the same measured trap:
-- `0016` installed `accounts_exactly_one_default` as a deferred constraint trigger, so any
-- row-touching statement on `accounts` queues an event checked at COMMIT and a later `ALTER`
-- in the same transaction aborts with *"cannot ALTER TABLE because it has pending trigger
-- events"*. `purchases` has no such trigger today; **the two `ALTER`s here are therefore
-- placed before the first statement that can touch a `purchases` row**, which in this file is
-- none — the guard is a `DO` block that only reads.
ALTER TABLE purchases ALTER COLUMN payment_method_id DROP NOT NULL;

-- ── The two states of a purchase, checked at commit ────────────────────────
--
-- `S2`:
--
--     payment_method_id IS NULL      ->  exactly one payable names this purchase
--     payment_method_id IS NOT NULL  ->  zero payables, and at least one ledger entry
--
-- **`DEFERRABLE INITIALLY DEFERRED` is not optional**, for `0021`'s reason: the purchase is
-- inserted before the payable that names it (a payable needs the purchase's id) and the
-- ledger entry is written after the purchase is read back, so a plain `AFTER INSERT` would
-- refuse the correct order. Deferring to COMMIT lets one transaction build a coherent set of
-- rows and judge it once.
--
-- `IS DISTINCT FROM` over the pair rather than a chain of `<>`: the guard's job is to keep
-- holding when a future migration relaxes something else.
CREATE OR REPLACE FUNCTION purchase_payment_method_is_claimed() RETURNS trigger AS $$
DECLARE
    owed  bigint;
    moved bigint;
BEGIN
    SELECT count(*) INTO owed
      FROM payables
     WHERE source_document_kind = 'purchase' AND source_document_id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'purchase' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF owed <> 1 THEN
            RAISE EXCEPTION
                'purchase % says it moved no money, but % payable(s) name it; a purchase on account has exactly one',
                NEW.id, owed;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'purchase % says it moved no money, but % ledger entr(ies) name it',
                NEW.id, moved;
        END IF;
    ELSE
        IF owed <> 0 THEN
            RAISE EXCEPTION
                'purchase % names payment method %, but % payable(s) also name it; money moved or debt arose, never both',
                NEW.id, NEW.payment_method_id, owed;
        END IF;
        IF moved = 0 THEN
            RAISE EXCEPTION
                'purchase % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS purchases_payment_method_claimed ON purchases;
CREATE CONSTRAINT TRIGGER purchases_payment_method_claimed
AFTER INSERT OR UPDATE OF payment_method_id ON purchases
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION purchase_payment_method_is_claimed();

-- ── The payable can only name a purchase that claims it ────────────────────
--
-- The other half of the pair. Without it the purchase-side trigger could be satisfied by a
-- payable naming a *different* purchase, and the party would be free to disagree with the
-- document: a debt attributed to a supplier who never sold anything.
--
-- `S4`: the party is derived from the document, never trusted from the caller.
CREATE OR REPLACE FUNCTION payable_matches_its_purchase() RETURNS trigger AS $$
DECLARE
    claim_method   UUID;
    purchase_party UUID;
BEGIN
    IF NEW.source_document_kind <> 'purchase' THEN
        -- Closed on purpose: a second source kind arrives with its own branch, the way
        -- `0018` and `0020` extended theirs.
        RAISE EXCEPTION
            'payable source kind % has no rule yet', NEW.source_document_kind;
    END IF;

    SELECT p.payment_method_id, p.supplier_id
      INTO claim_method, purchase_party
      FROM purchases p
     WHERE p.id = NEW.source_document_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'payable names purchase % which does not exist', NEW.source_document_id;
    END IF;

    IF claim_method IS NOT NULL THEN
        RAISE EXCEPTION
            'purchase % names payment method %: money moved, so it owes nothing',
            NEW.source_document_id, claim_method;
    END IF;

    IF NEW.party_kind <> 'supplier' OR NEW.party_id IS DISTINCT FROM purchase_party THEN
        RAISE EXCEPTION
            'payable on purchase % must belong to that purchase''s supplier %, not % %',
            NEW.source_document_id, purchase_party, NEW.party_kind, NEW.party_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS payables_match_their_purchase ON payables;
CREATE CONSTRAINT TRIGGER payables_match_their_purchase
AFTER INSERT ON payables
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION payable_matches_its_purchase();

-- ── A supplier return on an unpaid purchase moves no money ─────────────────
--
-- `S8`, and the sign is the whole point. A return that inherits the purchase's NULL method
-- must write **no** ledger entry and exactly one adjustment; a return that names a method must
-- write a ledger entry and no adjustment. Before this, the inherited path wrote `+total`
-- against the purchase's method — which for an unpaid purchase takes money out of an account
-- that never paid it, `L107`'s shape with the sign reversed.
--
-- **A return may be NULL only when its purchase is NULL.** A return on a paid purchase that
-- moved no money would be money from nowhere, the same rule `0021` states for a credit note.
ALTER TABLE supplier_returns ALTER COLUMN payment_method_id DROP NOT NULL;

CREATE OR REPLACE FUNCTION supplier_return_moves_money_or_lowers_debt()
RETURNS trigger AS $$
DECLARE
    purchase_method UUID;
    moved           bigint;
    lowered         bigint;
BEGIN
    SELECT p.payment_method_id INTO purchase_method
      FROM supplier_returns sr JOIN purchases p ON p.id = sr.purchase_id
     WHERE sr.id = NEW.id;

    SELECT count(*) INTO moved
      FROM money_entries
     WHERE document_kind = 'supplier_return' AND document_id = NEW.id;

    SELECT count(*) INTO lowered
      FROM payables_adjustments
     WHERE document_kind = 'supplier_return' AND document_id = NEW.id;

    IF NEW.payment_method_id IS NULL THEN
        IF purchase_method IS NOT NULL THEN
            RAISE EXCEPTION
                'supplier return % says it moved no money, but its purchase % names payment method %',
                NEW.id, NEW.purchase_id, purchase_method;
        END IF;
        IF moved <> 0 THEN
            RAISE EXCEPTION
                'supplier return % says it moved no money but wrote % ledger entr(ies)',
                NEW.id, moved;
        END IF;
        IF lowered <> 1 THEN
            RAISE EXCEPTION
                'supplier return % moves no money, so it must lower exactly one payable; % found',
                NEW.id, lowered;
        END IF;
    ELSE
        IF moved = 0 THEN
            RAISE EXCEPTION
                'supplier return % names payment method % but wrote no ledger entry',
                NEW.id, NEW.payment_method_id;
        END IF;
        IF lowered <> 0 THEN
            RAISE EXCEPTION
                'supplier return % names payment method %: money came back, so it lowers no debt',
                NEW.id, NEW.payment_method_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS supplier_returns_claim_their_effect ON supplier_returns;
CREATE CONSTRAINT TRIGGER supplier_returns_claim_their_effect
AFTER INSERT ON supplier_returns
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION supplier_return_moves_money_or_lowers_debt();

-- ── No backfill, and the proof is a refusal rather than an assumption ──────
--
-- **Purchases never went on account before this file**, so there is no ordinary pre-existing
-- row to migrate: `purchases.rs` refused the debt method outright, and the method it names is
-- `NOT NULL` for every one of them. What could exist is a database someone hand-edited to
-- point a purchase at the debt account — the same shape `0021` refuses for a credit sale, and
-- refused here with the same remedy, because the book is append-only and rewriting it is what
-- this repository will not do.

-- ── Prove the guards exist, and that nothing is in the way ─────────────────
--
-- LAST, for the reason `0012` records: a check placed earlier reports whatever happens to
-- fail next instead of the guard that is actually missing.
DO $$
DECLARE
    present  bigint;
    legacy   bigint;
    unclaimed bigint;
BEGIN
    SELECT count(*) INTO present
      FROM (VALUES
              ('payables', 'payables_append_only'),
              ('payables', 'payables_no_truncate'),
              ('payables', 'payables_match_their_purchase'),
              ('payables_adjustments', 'payables_adjustments_append_only'),
              ('payables_adjustments', 'payables_adjustments_no_truncate'),
              ('payables_adjustments', 'payables_adjustments_ceiling'),
              ('payable_settlements', 'payable_settlements_append_only'),
              ('payable_settlements', 'payable_settlements_no_truncate'),
              ('payable_settlements', 'payable_settlements_within_both_ceilings'),
              ('purchases', 'purchases_payment_method_claimed'),
              ('supplier_returns', 'supplier_returns_claim_their_effect')
           ) AS wanted(table_name, name)
     WHERE EXISTS (
             SELECT 1 FROM pg_trigger t
              WHERE t.tgname = wanted.name
                AND t.tgrelid = wanted.table_name::regclass
                AND NOT t.tgisinternal
           );
    IF present <> 11 THEN
        RAISE EXCEPTION
            'expected 11 v4-payables guards, found %; refusing to continue', present;
    END IF;

    -- The columns really are nullable, which no trigger would reveal.
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'purchases' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION 'purchases.payment_method_id is still NOT NULL; refusing to continue';
    END IF;
    IF (SELECT is_nullable FROM information_schema.columns
         WHERE table_name = 'supplier_returns' AND column_name = 'payment_method_id') <> 'YES' THEN
        RAISE EXCEPTION
            'supplier_returns.payment_method_id is still NOT NULL; refusing to continue';
    END IF;

    -- **No legacy data may be in the way, and the remedy is a DROP** — the owner's standing
    -- decision: nothing has shipped, so no database holds history worth carrying.
    SELECT count(*) INTO legacy
      FROM money_entries me
      JOIN payment_methods pm ON pm.id = me.payment_method_id
      JOIN accounts a         ON a.id = pm.account_id
     WHERE a.name = 'credit';
    IF legacy > 0 THEN
        RAISE EXCEPTION
            '% money_entries row(s) name the debt account. This migration does not migrate them: the application has never shipped, so the remedy is to drop the development database and start from empty ("docker compose down -v").',
            legacy;
    END IF;

    -- **Both halves of the two-state rule**, because `0021` shipped a check that asserted only
    -- one and its review named it (`R2-002`): a purchase holding a method with no ledger entry
    -- — or a NULL purchase with an entry — would pass a one-sided check and be refused at its
    -- next write.
    SELECT count(*) INTO unclaimed
      FROM purchases p
     WHERE (p.payment_method_id IS NULL)
        <> (EXISTS (SELECT 1 FROM payables o
                     WHERE o.source_document_kind = 'purchase'
                       AND o.source_document_id = p.id))
        OR (p.payment_method_id IS NULL AND EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'purchase' AND me.document_id = p.id))
        OR (p.payment_method_id IS NOT NULL AND NOT EXISTS (
              SELECT 1 FROM money_entries me
               WHERE me.document_kind = 'purchase' AND me.document_id = p.id));
    IF unclaimed > 0 THEN
        RAISE EXCEPTION
            '% purchase(s) disagree with their payables or their ledger entries about whether money moved; refusing to continue',
            unclaimed;
    END IF;

    -- **`S13`, asserted rather than promised: nothing in `0021`/`0022` was modified.** The two
    -- functions this file *could* have re-cut are byte-identical to what those migrations left,
    -- which is what makes "a new table cannot lose a clause it never had" a property instead of
    -- a hope. A future session that reaches for the customer ceiling from here fails this check
    -- and has to say so.
    IF (SELECT prosrc FROM pg_proc WHERE proname = 'settlement_within_both_ceilings')
       NOT LIKE '%money that left cannot settle a debt%' THEN
        RAISE EXCEPTION
            'settlement_within_both_ceilings was modified; this migration must not touch the customer ceiling';
    END IF;
    IF (SELECT prosrc FROM pg_proc WHERE proname = 'obligation_adjustments_within_amount')
       NOT LIKE '%would lower it past zero%' THEN
        RAISE EXCEPTION
            'obligation_adjustments_within_amount was modified; this migration must not touch the customer ceiling';
    END IF;
END;
$$;
