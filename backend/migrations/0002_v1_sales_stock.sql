-- v1: sales slice — products, customers, sales, sale_items, stock_movements.
-- Money is stored as BIGINT cents. The walk-in customer is the default for
-- sales with no named customer.

CREATE TABLE IF NOT EXISTS customers (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS products (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    list_price BIGINT NOT NULL CHECK (list_price >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sales (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    customer_id    UUID NOT NULL REFERENCES customers (id),
    total          BIGINT NOT NULL CHECK (total > 0),
    payment_method TEXT NOT NULL DEFAULT 'cash',
    status         TEXT NOT NULL DEFAULT 'confirmed'
                   CHECK (status IN ('draft', 'confirmed', 'cancel', 'done')),
    reversed_at    TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sale_items (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sale_id      UUID NOT NULL REFERENCES sales (id),
    product_id   UUID NOT NULL REFERENCES products (id),
    product_name TEXT NOT NULL,
    quantity     INTEGER NOT NULL CHECK (quantity > 0),
    unit_price   BIGINT NOT NULL CHECK (unit_price > 0),
    line_total   BIGINT NOT NULL CHECK (line_total > 0)
);

CREATE TABLE IF NOT EXISTS stock_movements (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES products (id),
    quantity   INTEGER NOT NULL CHECK (quantity <> 0),
    reason     TEXT NOT NULL,
    sale_id    UUID REFERENCES sales (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- walk-in: the default customer for sales with no named customer.
-- A real fiscal category, seeded with a fixed id so the API can reference it.
INSERT INTO customers (id, name)
VALUES ('00000000-0000-0000-0000-000000000001', 'walk-in')
ON CONFLICT DO NOTHING;

CREATE INDEX IF NOT EXISTS sales_created_at_idx ON sales (created_at DESC);
CREATE INDEX IF NOT EXISTS sale_items_sale_id_idx ON sale_items (sale_id);
CREATE INDEX IF NOT EXISTS stock_movements_product_id_idx ON stock_movements (product_id);
