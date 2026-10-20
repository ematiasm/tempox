/**
 * The HTTP client for the Tempox API.
 *
 * Every call here is a thin, honest pass-through: the client builds the query
 * the caller asked for and returns what the server sent. It never rescales a
 * quantity, converts a price, or invents a default the backend already owns.
 * That restraint is deliberate — money is a decimal string, and the unit is a
 * display label with no arithmetic consequence.
 */

/**
 * The API origin.
 *
 * There is deliberately **no fallback here**. This line used to read
 * `?? 'http://localhost:3001'`, which pointed every unconfigured build at the
 * development backend. That is the one URL a test must never reach: the
 * development ledger is append-only by trigger, so a row a test wrote there
 * cannot be deleted on its own — only `docker compose down -v`, which takes the
 * owner's data with it.
 *
 * Measured, not assumed: building with `PUBLIC_API_BASE` unset put the literal
 * `localhost:3001` into `_app/immutable/chunks/*.js`, so a bare
 * `npx playwright test` really did run against development.
 *
 * The value comes from whoever builds:
 *
 * - `docker-compose.yml` and `.env.example` set it for the dev stack;
 * - `make test-e2e` sets it for the e2e build;
 * - `vite.config.ts` supplies a development default for `vite dev` and vitest.
 *
 * A build that sets none of those now fails loudly here, at module load, instead
 * of silently talking to the wrong database.
 */
const configured = import.meta.env.PUBLIC_API_BASE as string | undefined;

if (!configured) {
  throw new Error(
    'PUBLIC_API_BASE is not set. Point it at the backend this build talks to. ' +
      'It is deliberately required: an unset value used to default to the development ' +
      'backend on :3001, which tests must never reach.'
  );
}

const BASE = configured;

/** A failed request, carrying the status the caller may need to branch on. */
export class ApiError extends Error {
  readonly status: number;

  constructor(status: number, message?: string) {
    super(message ?? `request failed: ${status}`);
    this.name = 'ApiError';
    this.status = status;
  }
}

export type Unit = 'unit' | 'g' | 'ml';

export type Note = { id: string; text: string; created_at: string };

export type Product = {
  id: string;
  name: string;
  /** Currency amount with two decimal places. */
  list_price: string;
  unit: Unit;
  /** Derived from the movement ledger, never a stored field. */
  stock: number;
  created_at: string;
};

export type Customer = { id: string; name: string; created_at: string };

export type SaleItem = {
  id: string;
  sale_id: string;
  product_id: string;
  /** Snapshot taken at sale time: renaming the product never rewrites history. */
  product_name: string;
  /** Integer quantity; unit is a display label only. */
  quantity: number;
  /** Currency amount with two decimal places. `0.00` is a bonus line. */
  unit_price: string;
  line_total: string;
  unit: Unit;
};

export type Sale = {
  id: string;
  customer_id: string;
  total: string;
  payment_method: string;
  status: string;
  reversed_at: string | null;
  created_at: string;
};

export type SaleListItem = Sale & {
  customer_name: string;
  items: SaleItem[];
  credit_notes: CreditNote[];
};

/**
 * One line of a credit note: how much of one sale line came back.
 *
 * `sale_item_id` is the link back to the corrected line, and the name, unit and
 * price are the sale line's snapshots — a refund returns what was charged, not
 * today's catalog price.
 */
export type CreditNoteItem = {
  id: string;
  credit_note_id: string;
  sale_item_id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit_price: string;
  line_total: string;
  unit: Unit;
};

/**
 * The compensating document that reverses part or all of a sale.
 *
 * The sale keeps its own status: a corrected sale still reads `confirmed`,
 * because what changed is not the sale but the fact that a second document
 * exists. Whether a sale is annulled is therefore derived from this array, not
 * read off the sale.
 */
export type CreditNote = {
  id: string;
  sale_id: string;
  /** Always positive: the amount returned. The sign is the display's business. */
  total: string;
  created_at: string;
  items: CreditNoteItem[];
};

export type Cash = { total: string };

/** A shop day, as the server resolved it. */
export type DayRange = {
  /** Inclusive, ISO-8601 UTC. */
  from: string;
  /** Exclusive, ISO-8601 UTC. */
  to: string;
  /** The local day before `from`, for paging backwards. */
  prev: string;
  /** The local day after `to`, for paging forwards. */
  next: string;
  /**
   * The IANA zone the window was computed in, when the server supplied one.
   *
   * The browser-side `dayRange` below cannot know it and leaves it undefined;
   * `fetchDay` always fills it. It exists so a document's time can be rendered
   * in the shop's zone rather than the machine's (S19).
   */
  zone?: string;
};

async function request<T>(url: URL, init?: RequestInit): Promise<T> {
  const r = await fetch(url, init);
  if (!r.ok) throw new ApiError(r.status);
  if (r.status === 204) return undefined as T;
  return (await r.json()) as T;
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body)
  };
}

/** Appends only the parameters that were actually given. */
function setIfPresent(url: URL, key: string, value: string | number | undefined | null) {
  if (value === undefined || value === null) return;
  const text = String(value);
  // An empty string is not a value: `from=` would reach the server as an
  // unparseable timestamp and turn a deliberately unscoped list into a 400.
  if (text === '') return;
  url.searchParams.set(key, text);
}

// ── Sales ────────────────────────────────────────────────────────────────────

export type ListSalesParams = {
  /** Inclusive bound on `created_at`. */
  from?: string;
  /** Exclusive bound on `created_at`. */
  to?: string;
  limit?: number;
  offset?: number;
};

/** Sales newest first, with their nested lines and customer name. */
export async function listSales(params: ListSalesParams = {}): Promise<SaleListItem[]> {
  const url = new URL('/api/sales', BASE);
  setIfPresent(url, 'from', params.from);
  setIfPresent(url, 'to', params.to);
  setIfPresent(url, 'limit', params.limit);
  setIfPresent(url, 'offset', params.offset);
  return request<SaleListItem[]>(url);
}

export type NewSaleLine = {
  product_id: string;
  /** Integer quantity; unit is a display label only. */
  quantity: number;
  /** Currency amount with two decimal places. Omit to use the list price; `0.00` records a bonus line. */
  unit_price?: string;
};

export type NewSale = {
  /** Omitted means the server's seeded walk-in customer. */
  customer_id?: string;
  items: NewSaleLine[];
};

/**
 * Records a sale. The purchase, its lines and the stock movements land in one
 * transaction on the server, so there is no partial sale to clean up here.
 */
export async function createSale(sale: NewSale): Promise<Sale & { items: SaleItem[] }> {
  const body: Record<string, unknown> = { items: sale.items };
  // Omitted, not null: the server distinguishes "no customer" (walk-in) from
  // "an unknown customer id" (404).
  if (sale.customer_id) body.customer_id = sale.customer_id;
  return request<Sale & { items: SaleItem[] }>(new URL('/api/sales', BASE), json('POST', body));
}

export async function getCash(): Promise<Cash> {
  return request<Cash>(new URL('/api/cash', BASE));
}

/** One line of a refund: which sale line, and how many units come back. */
export type NewCreditNoteLine = {
  sale_item_id: string;
  /** Integer quantity, at most what the line has left to return. */
  quantity: number;
};

/**
 * Reverses part or all of a sale by issuing a credit note.
 *
 * The note, its lines and the stock movements land in one transaction on the
 * server, so a failed refund leaves nothing behind. The server is also the
 * authority on how much a line has left to return: this client sends what the
 * cashier asked for and surfaces the refusal (409) rather than second-guessing
 * it with its own arithmetic.
 */
export async function createCreditNote(
  saleId: string,
  items: NewCreditNoteLine[]
): Promise<CreditNote> {
  return request<CreditNote>(
    new URL(`/api/sales/${saleId}/credit-notes`, BASE),
    json('POST', { items })
  );
}

// ── Suppliers ────────────────────────────────────────────────────────────────

export type Supplier = { id: string; name: string; created_at: string };

/**
 * Suppliers, including the seeded `one-off-supplier`.
 *
 * The seed is listed like any other supplier rather than hidden: a purchase that
 * omits its supplier names it explicitly, and a picker that left it out would
 * make that fallback invisible.
 */
export async function listSuppliers(): Promise<Supplier[]> {
  return request<Supplier[]>(new URL('/api/suppliers', BASE));
}

/**
 * Creates a supplier and returns what the server stored.
 *
 * The name is sent exactly as typed. Trimming here would make this client and
 * the server disagree about what counts as blank, and the server is the one that
 * decides. Unlike `findOrCreateCustomer`, a 409 is **not** swallowed: the picker
 * already lists the suppliers, so a duplicate means the user typed a name that
 * is right there to choose, and hiding the conflict would hide that.
 */
export async function createSupplier(name: string): Promise<Supplier> {
  return request<Supplier>(new URL('/api/suppliers', BASE), json('POST', { name }));
}

// ── Purchases ────────────────────────────────────────────────────────────────

export type CreatePurchaseLine = {
  product_id: string;
  /** Integer quantity in the product's base unit. */
  quantity: number;
  /**
   * Currency amount with two decimal places. Omit to use the list price.
   *
   * A purchase price that differs from the catalogue is the normal case, not an
   * exception, which is why this override exists on the same terms a sale line
   * has it.
   */
  unit_price?: string;
};

export type NewPurchase = {
  /** Omitted means the server's seeded `one-off-supplier`. */
  supplier_id?: string;
  items: CreatePurchaseLine[];
};

export type ListPurchasesParams = {
  /** Inclusive bound on `created_at`. */
  from?: string;
  /** Exclusive bound on `created_at`. */
  to?: string;
  limit?: number;
  offset?: number;
};

/** A purchase and its lines, which is how both endpoints answer. */
export type PurchaseListItem = {
  purchase: Purchase;
  items: PurchaseItem[];
  /**
   * The returns that corrected this purchase, oldest first.
   *
   * Always present, never absent: a purchase nobody returned answers with an
   * empty array. The create endpoint answers empty because a purchase cannot be
   * born corrected.
   *
   * Whether a purchase is fully returned is derived from this array, exactly as
   * a sale's annulment is derived from its credit notes.
   */
  supplier_returns: SupplierReturnWithItems[];
};

export type Purchase = {
  id: string;
  supplier_id: string;
  total: string;
  created_at: string;
};

/**
 * One line of a purchase: the product, and the name/unit/price snapshotted when
 * the goods arrived.
 */
export type PurchaseItem = {
  id: string;
  purchase_id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit_price: string;
  line_total: string;
  unit: Unit;
};

/** Purchases newest first, with their nested lines. */
export async function listPurchases(params: ListPurchasesParams = {}): Promise<PurchaseListItem[]> {
  const url = new URL('/api/purchases', BASE);
  setIfPresent(url, 'from', params.from);
  setIfPresent(url, 'to', params.to);
  setIfPresent(url, 'limit', params.limit);
  setIfPresent(url, 'offset', params.offset);
  return request<PurchaseListItem[]>(url);
}

/**
 * Records a purchase. The purchase, its lines and the positive stock movements
 * land in one transaction on the server, so there is no partial purchase to
 * clean up here.
 */
export async function createPurchase(purchase: NewPurchase): Promise<PurchaseListItem> {
  const body: Record<string, unknown> = { items: purchase.items };
  // Omitted, not null: the server distinguishes "no supplier" (the seeded
  // one-off-supplier) from "an unknown supplier id" (404).
  if (purchase.supplier_id) body.supplier_id = purchase.supplier_id;
  // `PurchaseListItem`, not `{purchase, items}`: the server answers the same
  // shape as the list endpoint, with `supplier_returns` empty because a purchase
  // cannot be born corrected. The narrower type described the server wrongly and
  // the comment above the list type claimed otherwise.
  return request<PurchaseListItem>(new URL('/api/purchases', BASE), json('POST', body));
}

/**
 * One line of a return: the purchase line being corrected, and how much goes
 * back. It carries no price on purpose (S3).
 */
export type NewSupplierReturnLine = {
  purchase_item_id: string;
  /** Integer quantity, at most what the purchase line has left to return. */
  quantity: number;
};

/** A return of goods to their supplier: the compensating document for a purchase. */
export type SupplierReturn = {
  id: string;
  purchase_id: string;
  /** Always positive: the amount coming back. The sign is the display's business. */
  total: string;
  created_at: string;
};

export type SupplierReturnItem = {
  id: string;
  supplier_return_id: string;
  /** The purchase line this return corrects, so a reader never infers it. */
  purchase_item_id: string;
  product_id: string;
  product_name: string;
  quantity: number;
  unit_price: string;
  line_total: string;
  unit: Unit;
};

/**
 * A return with its lines, which is how both the create response and the nested
 * reads answer.
 *
 * The header nests under `return` rather than being flattened, because the JSON
 * key the server chose is `return` (a Rust keyword, escaped on that side). This
 * type is the contract, not a guess: `GET /api/purchases` and
 * `POST /api/purchases/{id}/supplier-returns` both answer this shape.
 */
export type SupplierReturnWithItems = {
  return: SupplierReturn;
  items: SupplierReturnItem[];
};

/**
 * Returns goods to their supplier, correcting a purchase without writing it.
 *
 * The return, its lines and the negative stock movements land in one transaction
 * on the server, so a refused return leaves nothing behind. The server is also
 * the authority on how much a line has left: this client sends what the operator
 * asked for and surfaces the refusal (409) rather than second-guessing it.
 *
 * No price is sent (S3): what the return is worth is read server-side from the
 * purchase line, so a return can never be worth more than the purchase did.
 */
export async function createSupplierReturn(
  purchaseId: string,
  items: NewSupplierReturnLine[]
): Promise<SupplierReturnWithItems> {
  return request<SupplierReturnWithItems>(
    new URL(`/api/purchases/${purchaseId}/supplier-returns`, BASE),
    json('POST', { items })
  );
}

// ── Products ─────────────────────────────────────────────────────────────────

export type ListProductsParams = {
  /** Case-insensitive substring match on the name. */
  q?: string;
  limit?: number;
  offset?: number;
};

export async function listProducts(params: ListProductsParams = {}): Promise<Product[]> {
  const url = new URL('/api/products', BASE);
  const q = params.q?.trim();
  if (q) url.searchParams.set('q', q);
  setIfPresent(url, 'limit', params.limit);
  setIfPresent(url, 'offset', params.offset);
  return request<Product[]>(url);
}

export type NewProduct = {
  name: string;
  /** Currency amount with two decimal places. */
  list_price: string;
  unit?: Unit;
  initial_stock?: number;
};

export async function createProduct(product: NewProduct): Promise<Product> {
  return request<Product>(new URL('/api/products', BASE), json('POST', product));
}

// ── Customers ────────────────────────────────────────────────────────────────

export async function listCustomers(): Promise<Customer[]> {
  return request<Customer[]>(new URL('/api/customers', BASE));
}

/**
 * Finds the customer with this name, creating it only if it does not exist.
 *
 * The plain create would make a duplicate name look like a failure, and the
 * cashier would be shown an error for typing a customer the shop already has.
 * So a 409 conflict is not surfaced: it is read as "this name exists", and the
 * existing row is selected instead.
 */
export async function findOrCreateCustomer(name: string): Promise<Customer> {
  const trimmed = name.trim();
  try {
    return await request<Customer>(
      new URL('/api/customers', BASE),
      json('POST', { name: trimmed })
    );
  } catch (e) {
    if (!(e instanceof ApiError) || e.status !== 409) throw e;
  }

  const customers = await listCustomers();
  const existing = customers.find((c) => c.name === trimmed);
  if (!existing) {
    // The server refused the name but does not have it: that is a real
    // inconsistency, not a conflict to paper over.
    throw new ApiError(409, `customer "${trimmed}" conflicts but was not found`);
  }
  return existing;
}

// ── Time ─────────────────────────────────────────────────────────────────────

/**
 * The local day containing `at`, as an explicit range the server can apply
 * without knowing the shop's timezone.
 *
 * `to` is exclusive, matching the backend: two ranges that meet at midnight
 * partition the day's sales instead of counting the midnight sale twice. The
 * bounds are built from local calendar fields, so "today" means the shop's
 * today rather than UTC's.
 */
export function dayRange(at: Date = new Date()): DayRange {
  const y = at.getFullYear();
  const m = at.getMonth();
  const d = at.getDate();
  return {
    from: new Date(y, m, d, 0, 0, 0, 0).toISOString(),
    to: new Date(y, m, d + 1, 0, 0, 0, 0).toISOString(),
    prev: new Date(y, m, d - 1, 0, 0, 0, 0).toISOString(),
    next: new Date(y, m, d + 1, 0, 0, 0, 0).toISOString()
  };
}

/** The shop-local day containing `at`, as determined by the server. */
/** Which span of the shop's calendar a ledger page is scoped to. */
export type Scope = 'day' | 'week' | 'month';

/**
 * Asks the server which instants make up the span containing `at`.
 *
 * The span is never computed here. `scope` is a word, the boundaries come back
 * as instants: a client that chose its own boundaries could scope itself to a
 * week the shop never worked, which is the same reason the day is computed
 * server-side (S7).
 */
export async function fetchDay(at: Date, scope: Scope = 'day'): Promise<DayRange> {
  const url = new URL('/api/day', BASE);
  url.searchParams.set('at', at.toISOString());
  // Sent only when it is not the default, so an older server that does not know
  // the parameter keeps answering exactly as before.
  if (scope !== 'day') url.searchParams.set('scope', scope);
  return request<DayRange>(url);
}

// ── The walking skeleton (kept, not extended) ────────────────────────────────

export async function sayHello(name: string): Promise<{ message: string; server_time: string }> {
  const url = new URL('/api/hello', BASE);
  if (name) url.searchParams.set('name', name);
  const r = await fetch(url);
  if (!r.ok) throw new Error(`hello failed: ${r.status}`);
  return r.json();
}

export async function listNotes(): Promise<Note[]> {
  return request<Note[]>(new URL('/api/notes', BASE));
}

export async function createNote(text: string): Promise<Note> {
  return request<Note>(new URL('/api/notes', BASE), json('POST', { text }));
}

export async function deleteNote(id: string): Promise<void> {
  const r = await fetch(new URL(`/api/notes/${id}`, BASE), { method: 'DELETE' });
  if (!r.ok) throw new ApiError(r.status, `deleteNote failed: ${r.status}`);
}
