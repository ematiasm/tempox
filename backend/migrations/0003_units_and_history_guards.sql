-- v1 units of measure + history guards.
-- Quantities are integers in the product's base unit (pieces, grams, ml) —
-- the same smallest-unit pattern as money in cents.

ALTER TABLE products ADD COLUMN IF NOT EXISTS unit TEXT NOT NULL DEFAULT 'unit'
    CHECK (unit IN ('unit', 'g', 'ml'));

ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS unit TEXT NOT NULL DEFAULT 'unit'
    CHECK (unit IN ('unit', 'g', 'ml'));

-- A product with history cannot change its unit: its stock movements and
-- sale lines are denominated in the old unit.
CREATE OR REPLACE FUNCTION guard_product_unit() RETURNS trigger AS $$
BEGIN
    IF NEW.unit <> OLD.unit THEN
        IF EXISTS (SELECT 1 FROM stock_movements WHERE product_id = OLD.id)
           OR EXISTS (SELECT 1 FROM sale_items WHERE product_id = OLD.id) THEN
            RAISE EXCEPTION 'unit of product % cannot change: it has history', OLD.id;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS products_unit_guard ON products;
CREATE TRIGGER products_unit_guard
BEFORE UPDATE OF unit ON products
FOR EACH ROW EXECUTE FUNCTION guard_product_unit();

-- Sale lines are pure history. A correction is a compensating row (reversal),
-- never an edit.
CREATE OR REPLACE FUNCTION reject_sale_item_mutation() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'sale_items are immutable: corrections are compensating rows, not edits';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sale_items_immutable ON sale_items;
CREATE TRIGGER sale_items_immutable
BEFORE UPDATE OR DELETE ON sale_items
FOR EACH ROW EXECUTE FUNCTION reject_sale_item_mutation();

-- A sale's economics are immutable. Only the status transition may be
-- written (reversal sets status/reversed_at) — never total, customer, or
-- payment method.
CREATE OR REPLACE FUNCTION guard_sale_economics() RETURNS trigger AS $$
BEGIN
    IF NEW.total <> OLD.total
       OR NEW.customer_id <> OLD.customer_id
       OR NEW.payment_method <> OLD.payment_method THEN
        RAISE EXCEPTION 'sale % economics are immutable: status may change, money may not', OLD.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS sales_economics_guard ON sales;
CREATE TRIGGER sales_economics_guard
BEFORE UPDATE ON sales
FOR EACH ROW EXECUTE FUNCTION guard_sale_economics();
