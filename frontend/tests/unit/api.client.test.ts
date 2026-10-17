import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  createCreditNote,
  createProduct,
  createSale,
  dayRange,
  fetchDay,
  findOrCreateCustomer,
  getCash,
  listCustomers,
  listProducts,
  listSales
} from '#lib/api.js';

/** The URL the client actually called, as a URL object. */
function calledUrl(mock: ReturnType<typeof vi.fn>, index = 0): URL {
  const arg = mock.mock.calls[index]?.[0];
  expect(arg).toBeInstanceOf(URL);
  return arg as URL;
}

/** The init the client actually passed, if any. */
function calledInit(mock: ReturnType<typeof vi.fn>, index = 0): RequestInit | undefined {
  return mock.mock.calls[index]?.[1] as RequestInit | undefined;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' }
  });
}

beforeEach(() => {
  vi.unstubAllGlobals();
});

describe('listSales', () => {
  it('sends the range, the limit and the offset it was given', async () => {
    const fetchMock = vi.fn(async () => jsonResponse([]));
    vi.stubGlobal('fetch', fetchMock);

    await listSales({
      from: '2026-10-14T00:00:00Z',
      to: '2026-10-15T00:00:00Z',
      limit: 50,
      offset: 50
    });

    const url = calledUrl(fetchMock);
    expect(url.pathname).toBe('/api/sales');
    expect(url.searchParams.get('from')).toBe('2026-10-14T00:00:00Z');
    expect(url.searchParams.get('to')).toBe('2026-10-15T00:00:00Z');
    expect(url.searchParams.get('limit')).toBe('50');
    expect(url.searchParams.get('offset')).toBe('50');
  });

  it('omits absent bounds instead of sending empty ones', async () => {
    // An empty `from` would arrive at the server as an unparseable timestamp
    // and turn a deliberately unscoped list into a 400.
    const fetchMock = vi.fn(async () => jsonResponse([]));
    vi.stubGlobal('fetch', fetchMock);

    await listSales({});

    const url = calledUrl(fetchMock);
    expect(url.search).toBe('');
  });

  it('throws ApiError carrying the status on failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('nope', { status: 500 }))
    );

    const err = await listSales({}).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(500);
  });
});

describe('listProducts', () => {
  it('sends q with limit and offset', async () => {
    const fetchMock = vi.fn(async () => jsonResponse([]));
    vi.stubGlobal('fetch', fetchMock);

    await listProducts({ q: 'harina', limit: 20, offset: 0 });

    const url = calledUrl(fetchMock);
    expect(url.pathname).toBe('/api/products');
    expect(url.searchParams.get('q')).toBe('harina');
    expect(url.searchParams.get('limit')).toBe('20');
    expect(url.searchParams.get('offset')).toBe('0');
  });

  it('omits an absent or blank q, which means the whole catalog', async () => {
    const fetchMock = vi.fn(async () => jsonResponse([]));
    vi.stubGlobal('fetch', fetchMock);

    await listProducts({ q: '   ' });

    const url = calledUrl(fetchMock);
    expect(url.searchParams.has('q')).toBe(false);
  });
});

describe('listCustomers and getCash', () => {
  it('lists customers', async () => {
    const fetchMock = vi.fn(async () =>
      jsonResponse([{ id: 'c1', name: 'walk-in', created_at: '2026-10-14T00:00:00Z' }])
    );
    vi.stubGlobal('fetch', fetchMock);

    const customers = await listCustomers();
    expect(calledUrl(fetchMock).pathname).toBe('/api/customers');
    expect(customers[0]?.name).toBe('walk-in');
  });

  it('reads the cash total', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ total: '184300.00' }));
    vi.stubGlobal('fetch', fetchMock);

    const cash = await getCash();
    expect(calledUrl(fetchMock).pathname).toBe('/api/cash');
    expect(cash.total).toBe('184300.00');
  });
});

describe('findOrCreateCustomer (S7)', () => {
  it('returns the customer the server created', async () => {
    const fetchMock = vi.fn(async () =>
      jsonResponse({ id: 'new-1', name: 'Kiosco Sur', created_at: '2026-10-14T00:00:00Z' }, 201)
    );
    vi.stubGlobal('fetch', fetchMock);

    const customer = await findOrCreateCustomer('Kiosco Sur');
    expect(customer.id).toBe('new-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('selects the existing customer on 409 instead of surfacing an error', async () => {
    // Decision 7: a duplicate name is never a red error. The second call is the
    // lookup that finds the row the server refused to duplicate.
    //
    // The mock branches on the method as well as the path: create and list share
    // `/api/customers`, so a path-only mock would answer the POST with the list.
    const fetchMock = vi.fn(async (url: URL | RequestInfo, init?: RequestInit) => {
      const path = url instanceof URL ? url.pathname : String(url);
      if (path !== '/api/customers') return new Response('not found', { status: 404 });
      if (init?.method === 'POST') return new Response('conflict', { status: 409 });
      return jsonResponse([
        { id: 'existing-1', name: 'Panadería La Esquina', created_at: '2026-10-14T00:00:00Z' }
      ]);
    });
    vi.stubGlobal('fetch', fetchMock);

    const customer = await findOrCreateCustomer('Panadería La Esquina');
    expect(customer.id).toBe('existing-1');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('does not swallow a failure that is not a conflict', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('boom', { status: 500 }))
    );

    await expect(findOrCreateCustomer('X')).rejects.toThrow();
  });
});

describe('createProduct and createSale (S4)', () => {
  it('creates a product in unit only, passing the price through untouched', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ id: 'p1' }, 201));
    vi.stubGlobal('fetch', fetchMock);

    await createProduct({
      name: 'Harina 000 Panda 1 kg',
      list_price: '25.00',
      unit: 'unit',
      initial_stock: 12
    });

    const init = calledInit(fetchMock);
    expect(calledUrl(fetchMock).pathname).toBe('/api/products');
    expect(init?.method).toBe('POST');
    expect(JSON.parse(String(init?.body))).toEqual({
      name: 'Harina 000 Panda 1 kg',
      list_price: '25.00',
      unit: 'unit',
      initial_stock: 12
    });
  });

  it('sends the sale exactly as given, with no client-side rescaling', async () => {
    // A bonus line is a real zero-price line; decimal amounts pass through unchanged.
    const fetchMock = vi.fn(async () => jsonResponse({ id: 's1' }, 201));
    vi.stubGlobal('fetch', fetchMock);

    await createSale({
      items: [
        { product_id: 'p1', quantity: 3, unit_price: '25.00' },
        { product_id: 'p2', quantity: 500, unit_price: '0.01' },
        { product_id: 'p3', quantity: 1, unit_price: '0.00' }
      ]
    });

    const init = calledInit(fetchMock);
    expect(calledUrl(fetchMock).pathname).toBe('/api/sales');
    expect(JSON.parse(String(init?.body))).toEqual({
      items: [
        { product_id: 'p1', quantity: 3, unit_price: '25.00' },
        { product_id: 'p2', quantity: 500, unit_price: '0.01' },
        { product_id: 'p3', quantity: 1, unit_price: '0.00' }
      ]
    });
  });

  it('omits an absent customer so the server defaults to walk-in', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ id: 's1' }, 201));
    vi.stubGlobal('fetch', fetchMock);

    await createSale({ items: [{ product_id: 'p1', quantity: 1, unit_price: '1.00' }] });

    const body = JSON.parse(String(calledInit(fetchMock)?.body));
    expect(body).not.toHaveProperty('customer_id');
  });
});

describe('createCreditNote (S3, S7)', () => {
  it('posts the refund to the sale it corrects, exactly as given', async () => {
    const fetchMock = vi.fn(async () => jsonResponse({ id: 'n1' }, 201));
    vi.stubGlobal('fetch', fetchMock);

    await createCreditNote('s1', [
      { sale_item_id: 'i1', quantity: 3 },
      { sale_item_id: 'i2', quantity: 1 }
    ]);

    const url = calledUrl(fetchMock);
    // The sale is in the path, not in the body: the refund belongs to a
    // document, and a body-sale_id would let the two disagree.
    expect(url.pathname).toBe('/api/sales/s1/credit-notes');
    expect(calledInit(fetchMock)?.method).toBe('POST');
    expect(JSON.parse(String(calledInit(fetchMock)?.body))).toEqual({
      items: [
        { sale_item_id: 'i1', quantity: 3 },
        { sale_item_id: 'i2', quantity: 1 }
      ]
    });
  });

  it('carries the status of a refused refund so the caller can explain it', async () => {
    // 409 is the server's cumulative guard answering: the client must not try
    // to reproduce that arithmetic, only to report it.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse({ error: 'too much' }, 409))
    );

    await expect(
      createCreditNote('s1', [{ sale_item_id: 'i1', quantity: 99 }])
    ).rejects.toMatchObject({
      status: 409
    });
  });
});

describe('fetchDay (S8)', () => {
  it('sends the scoped instant as an ISO string and returns the server range', async () => {
    const range = {
      from: '2026-10-14T03:00:00Z',
      to: '2026-10-15T03:00:00Z',
      prev: '2026-10-13T03:00:00Z',
      next: '2026-10-15T03:00:00Z'
    };
    const fetchMock = vi.fn(async () => jsonResponse(range));
    vi.stubGlobal('fetch', fetchMock);
    const at = new Date('2026-10-14T16:42:00-03:00');

    await expect(fetchDay(at)).resolves.toEqual(range);

    const url = calledUrl(fetchMock);
    expect(url.pathname).toBe('/api/day');
    expect(url.searchParams.get('at')).toBe('2026-10-14T19:42:00.000Z');
  });

  it('throws ApiError carrying the status on failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('nope', { status: 500 }))
    );

    const error = await fetchDay(new Date('2026-10-14T19:42:00.000Z')).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(500);
  });
});

describe('dayRange (S8, S9)', () => {
  it('spans local midnight to local midnight around the given instant', () => {
    const at = new Date(2026, 9, 14, 18, 42); // 14 Oct 2026, 18:42 local
    const { from, to } = dayRange(at);
    const start = new Date(from);
    const end = new Date(to);

    expect(start.getFullYear()).toBe(2026);
    expect(start.getMonth()).toBe(9);
    expect(start.getDate()).toBe(14);
    expect(start.getHours()).toBe(0);
    expect(start.getMinutes()).toBe(0);

    expect(end.getDate()).toBe(15);
    expect(end.getHours()).toBe(0);

    expect(start.getTime()).toBeLessThanOrEqual(at.getTime());
    expect(end.getTime()).toBeGreaterThan(at.getTime());
  });

  it('makes consecutive days adjacent, so no sale is counted twice', () => {
    // The backend excludes `to`; the client has to agree with it or the sale
    // recorded at midnight appears in both days.
    const day = new Date(2026, 9, 14, 12, 0);
    const next = new Date(2026, 9, 15, 12, 0);
    expect(dayRange(day).to).toBe(dayRange(next).from);
  });

  it('reports the previous and next local days for paging', () => {
    const at = new Date(2026, 9, 14, 18, 42);
    expect(dayRange(at).prev).toBe(dayRange(new Date(2026, 9, 13, 12, 0)).from);
    expect(dayRange(at).next).toBe(dayRange(new Date(2026, 9, 15, 12, 0)).from);
  });
});
