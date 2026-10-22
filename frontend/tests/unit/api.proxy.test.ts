import { beforeEach, describe, expect, it, vi } from 'vitest';
import { forward, proxyTarget } from '#lib/apiProxy.js';

/**
 * The proxy route: the one place the browser's `/api/*` becomes the backend's
 * `/api/*`.
 *
 * `odd/tasks/docker-prod.md`, S8. It reads `BACKEND_URL` at **runtime**, so one
 * image serves any deployment and no value is baked at build time. That is what
 * lets `api.ts` ask its own origin (S7) and what removed the need for
 * `PUBLIC_API_BASE` entirely (S9).
 *
 * The cases below pin the two things a proxy must not do: drop part of the
 * request on the way out, and claim a status the backend did not send. A proxy
 * that swallows a 404 into a 200 turns a real defect into a blank screen.
 */

/** A `Request` the route can receive, with a body when the caller has one. */
function request(url: string, init?: RequestInit): Request {
  return new Request(url, init);
}

/** The single URL the route's `fetch` was called with. */
function calledTarget(mock: ReturnType<typeof vi.fn>): string {
  const arg = mock.mock.calls[0]?.[0];
  return typeof arg === 'string' ? arg : (arg as URL).toString();
}

beforeEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.stubEnv('BACKEND_URL', 'http://backend:3001');
  vi.stubEnv('DATABASE_URL', 'postgres://tempox:tempox@postgres:5432/tempox_test');
});

describe('proxyTarget (S8)', () => {
  it('rewrites the origin and keeps the path the backend serves', () => {
    const target = proxyTarget(
      new URL('http://app:5173/api/products?limit=50'),
      'http://backend:3001'
    );
    expect(target).toBe('http://backend:3001/api/products?limit=50');
  });

  it('tolerates a trailing slash on the configured backend', () => {
    // `.env.prod` is written by a person; `http://backend:3001/` must not
    // produce `http://backend:3001//api/products`.
    const target = proxyTarget(new URL('http://app:5173/api/sales'), 'http://backend:3001/');
    expect(target).toBe('http://backend:3001/api/sales');
  });

  it('keeps a nested path and its query intact', () => {
    const target = proxyTarget(
      new URL('http://app:5173/api/purchases/abc/supplier-returns?x=1&y=2'),
      'http://backend:3001'
    );
    expect(target).toBe('http://backend:3001/api/purchases/abc/supplier-returns?x=1&y=2');
  });
});

describe('the proxy forwards and answers faithfully (S8)', () => {
  it('passes the method, the body and the response back to the client', async () => {
    const backend = new Response(JSON.stringify({ id: 'p1' }), {
      status: 201,
      headers: { 'content-type': 'application/json' }
    });
    const fetchMock = vi.fn(async () => backend);
    vi.stubGlobal('fetch', fetchMock);

    const body = JSON.stringify({ name: 'Harina', list_price: '10.00' });
    const response = await forward(
      new Request('http://app:5173/api/products', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body
      })
    );

    // `fetchMock` is typed as taking no arguments, so the tuple has no index 1;
    // the call really does receive `(url, init)`. Read it through the mock's own
    // recorded arguments rather than fighting the inferred signature.
    const init = (fetchMock.mock.calls as unknown as [string, RequestInit][])[0]![1];
    expect(calledTarget(fetchMock)).toBe('http://backend:3001/api/products');
    expect(init.method).toBe('POST');
    // The body is forwarded as the stream it arrived as, not buffered and
    // re-serialized: that is the point of proxying a `Request`. Read it back to
    // prove the bytes survive, rather than asserting on the object's shape.
    expect(await new Response(init.body).text()).toBe(body);
    expect(response.status).toBe(201);
    expect(await response.json()).toEqual({ id: 'p1' });
  });

  it('keeps the backend status instead of claiming success', async () => {
    // The defect this prevents: a 404 or a 422 surfacing as a 200 with an empty
    // body, which the UI would render as "no products" rather than an error.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('nope', { status: 404 }))
    );

    const response = await forward(request('http://app:5173/api/products/abc'));

    expect(response.status).toBe(404);
  });

  it('answers 204 with no body, as the backend does', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response(null, { status: 204 }))
    );

    const response = await forward(request('http://app:5173/api/notes/abc', { method: 'DELETE' }));

    expect(response.status).toBe(204);
    expect(await response.text()).toBe('');
  });

  it('carries the query string through, which is how every list is scoped', async () => {
    const fetchMock = vi.fn(async () => new Response('[]', { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);

    await forward(request('http://app:5173/api/sales?from=2026-10-21T03:00:00Z&limit=50'));

    expect(calledTarget(fetchMock)).toBe(
      'http://backend:3001/api/sales?from=2026-10-21T03:00:00Z&limit=50'
    );
  });
});
