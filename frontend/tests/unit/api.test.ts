import { describe, expect, it, vi } from 'vitest';
import { sayHello } from '#lib/api.js';

describe('sayHello', () => {
  it('builds URL with name and returns parsed JSON', async () => {
    const fetchMock = vi.fn(
      async (_input: URL | RequestInfo, _init?: RequestInit) =>
        new Response(
          JSON.stringify({ message: 'Hello Ada!', server_time: '2026-07-22T00:00:00Z' }),
          {
            status: 200
          }
        )
    );
    vi.stubGlobal('fetch', fetchMock);

    const r = await sayHello('Ada');
    expect(r.message).toBe('Hello Ada!');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const firstArg = fetchMock.mock.calls[0]?.[0];
    expect(firstArg).toBeInstanceOf(URL);
    if (firstArg instanceof URL) {
      expect(firstArg.searchParams.get('name')).toBe('Ada');
    }
  });

  it('throws on non-2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('nope', { status: 500 }))
    );
    await expect(sayHello('X')).rejects.toThrow(/hello failed/);
  });
});
