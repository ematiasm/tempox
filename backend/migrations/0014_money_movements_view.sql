-- v3: `money_movements`, the single definition of "money moved".
--
-- **Why a view and not a table.** The stock ledger's movement IS the primary fact:
-- `sale_items.quantity` says what was sold, but nothing else says how much stock
-- moved. With money the document total is already the fact, so a mirror table would
-- add no information — only a second assertion about the same fact, needing a
-- coherence trigger per document kind forever, plus a backfill over immutable rows.
-- A view stores nothing, cannot drift from the documents, and needs no backfill.
-- Balances stay sums, never stored fields.
--
-- **Why it exists at all, rather than a `SUM` per call site.** Four consumers are
-- already planned (the general balance, one account's balance, the daily footer, and
-- a v6 statement), and with credit they become seven or eight. Each hand-written
-- `UNION` re-derives the sign and the destination, and the one that forgets a branch
-- returns a plausible wrong number instead of an error — which is exactly the class
-- of defect `0011` was written to kill for stock. One named definition, one place to
-- extend when a document is added.
--
-- **One row per EFFECT, not per document.** Today every document has one effect, but
-- a collection (v4) and a transfer (v4) each have two: money leaving one account and
-- arriving at another. Writing the view on "one row per document" would force a shape
-- change the moment credit exists, which is the thing choosing a view was supposed to
-- avoid.
--
-- **Sign convention, declared once: positive is money entering the account.**
--   sale              +total   (money came in)
--   credit_note       -total   (money went back to the customer)
--   purchase          -total   (money went out to the supplier)
--   supplier_return   +total   (the supplier gave it back)
--
-- Rediscovering that convention in four call sites is how two of them end up with the
-- opposite sign.
--
-- **What it does NOT do.** It validates nothing, it is not indexed, and it must never
-- be written to: it is a read model. The coherence rules live in the documents' own
-- triggers (`0011` for stock, `0012`/`0013` for destinations).

CREATE OR REPLACE VIEW money_movements AS
-- A sale puts money into the account its method resolves to. `status = 'confirmed'`
-- is the same predicate `/api/cash` used and `list_sales` applies: a sale carries a
-- reserved lifecycle whose other states must not count. `T2` drops that column, and
-- when it does this branch loses the filter because "the row exists" and "it counts"
-- become the same statement — which is already true of the other three.
SELECT s.payment_method_id          AS payment_method_id,
       pm.account_id                AS account_id,
       'sale'::text                 AS document_kind,
       s.id                         AS document_id,
       s.total                      AS amount,
       s.created_at                 AS occurred_at
  FROM sales s
  JOIN payment_methods pm ON pm.id = s.payment_method_id
 WHERE s.status = 'confirmed'

UNION ALL
-- A refund returns money to the customer, through the method the sale was paid with.
SELECT cn.payment_method_id, pm.account_id, 'credit_note'::text, cn.id, -cn.total, cn.created_at
  FROM credit_notes cn
  JOIN payment_methods pm ON pm.id = cn.payment_method_id

UNION ALL
-- A purchase pays the supplier.
SELECT p.payment_method_id, pm.account_id, 'purchase'::text, p.id, -p.total, p.created_at
  FROM purchases p
  JOIN payment_methods pm ON pm.id = p.payment_method_id

UNION ALL
-- A supplier return brings money back.
SELECT sr.payment_method_id, pm.account_id, 'supplier_return'::text, sr.id, sr.total, sr.created_at
  FROM supplier_returns sr
  JOIN payment_methods pm ON pm.id = sr.payment_method_id;

COMMENT ON VIEW money_movements IS
'Read model: one row per effect of money on an account. Positive is money entering. Never written to; balances are SUM over this view.';
