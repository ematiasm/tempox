-- v1 (P3): a bonus line is a real line whose price is zero.
--
-- plan-v1.md allows `products.list_price >= 0` but forced
-- `sale_items.unit_price > 0`, so a product created at price 0 could be
-- created and then not sold through the default path: the omitted
-- `unit_price` resolved to the product's list price, the money check
-- rejected it, and the caller got a 400 for a product the catalog accepted.
--
-- The rule that wins: a line may be given away. A bonus (regalo) is a real
-- event — the stock leaves the shop, so the row must exist — that happens to
-- carry no money. Three constraints move together, because relaxing only the
-- per-line ones would let the application accept a sale and the schema
-- reject it: a sale whose lines are all bonus lines totals zero too.
--
-- Negative amounts stay impossible everywhere: money cannot be un-earned by
-- typing a minus sign. The strictness moves from `> 0` to `>= 0`, nothing more.

ALTER TABLE sale_items
    DROP CONSTRAINT sale_items_unit_price_check,
    ADD CONSTRAINT sale_items_unit_price_check CHECK (unit_price >= 0);

ALTER TABLE sale_items
    DROP CONSTRAINT sale_items_line_total_check,
    ADD CONSTRAINT sale_items_line_total_check CHECK (line_total >= 0);

-- A fully bonified sale is worth 0 and still real: the stock moved, the
-- document exists, cash is simply unchanged by it.
ALTER TABLE sales
    DROP CONSTRAINT sales_total_check,
    ADD CONSTRAINT sales_total_check CHECK (total >= 0);
