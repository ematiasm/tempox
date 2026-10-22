/**
 * The proxy from the app's own `/api/*` to the backend.
 *
 * `odd/tasks/docker-prod.md`, S8. This module owns the rule; the route beside it
 * only wires methods to it. Split because SvelteKit refuses any export from a
 * `+server.ts` that is not an HTTP method or one of its few reserved names —
 * measured: exporting `proxyTarget` from the route made every request answer 500
 * with `invalid_export` — and because the rewrite is the only interesting logic
 * and is worth pinning without a socket.
 *
 * The reason this exists at all is a browser fact: `PUBLIC_API_BASE` used to be
 * baked into the bundle at build time, so one image was bound to one deployment,
 * and the baked value was `localhost` — which on any machine but the Docker host
 * means *that* machine. The client now calls its own origin (S7) and the server
 * resolves the backend here, reading `BACKEND_URL` at **runtime**.
 */

/**
 * The development backend, used only when `BACKEND_URL` is absent in dev.
 *
 * This exists because removing `PUBLIC_API_BASE` removed the default that made a
 * bare `npm run dev` work. Measured before this fallback: with no `BACKEND_URL`,
 * `GET /api/products` answered **500** — a `vite dev` session that used to work
 * needed hand-set environment, which is a regression no test covers because the
 * e2e suite always supplies the variable.
 *
 * It is `localhost:3001` because that is where `cargo run` listens, and this is
 * exactly the case the retirement was allowed to simplify: the browser never sees
 * this value, so it cannot reproduce the original LAN defect.
 */
const DEVELOPMENT_BACKEND = 'http://localhost:3001';

/**
 * The backend URL for the configured origin.
 *
 * Read per request, not at module load: `BACKEND_URL` is deployment
 * configuration, and a container restarted with a different value must pick it up
 * without a rebuild.
 *
 * The default applies **only outside production**. A production process that
 * forgot the variable must fail loudly rather than silently proxying to
 * `localhost` — that is the one mistake this fallback must not hide. `NODE_ENV`
 * is the signal the node adapter already sets, so no new variable is introduced.
 */
export function backendUrl(): string {
  const configured = process.env.BACKEND_URL?.trim();
  if (configured) return configured;

  if (process.env.NODE_ENV === 'production') {
    throw new Error(
      'BACKEND_URL is not set. The proxy cannot reach the backend without it. ' +
        'Set it in the container environment (see .env.prod.example).'
    );
  }

  return DEVELOPMENT_BACKEND;
}

/**
 * The URL to call: the client's path and query, against the backend origin.
 *
 * The path is kept whole because the backend mounts everything under `/api` —
 * `backend/src/routes/mod.rs` nests the whole router there — so this forwards
 * what it was given instead of rebuilding it.
 */
export function proxyTarget(incoming: URL, backend: string): string {
  const origin = backend.endsWith('/') ? backend.slice(0, -1) : backend;
  return `${origin}${incoming.pathname}${incoming.search}`;
}

/**
 * One implementation for every method.
 *
 * What it deliberately does NOT do:
 *
 * - **It does not read or rewrite the body.** It is a `Request` stream; passing
 *   it through means a large payload is not buffered twice.
 * - **It does not invent a status.** The backend's status, body and
 *   `content-type` go back as they came. Turning a 404 into a 200 would render
 *   an error as an empty list.
 * - **It does not drop CORS.** Same origin means the browser never needs it. The
 *   backend keeps its permissive layer for the `cargo run` loop.
 */
export async function forward(request: Request): Promise<Response> {
  const incoming = new URL(request.url);
  const hasBody = request.method !== 'GET' && request.method !== 'HEAD';

  const upstream = await fetch(proxyTarget(incoming, backendUrl()), {
    method: request.method,
    headers: request.headers,
    body: hasBody ? request.body : undefined,
    // Node needs this to stream a request body; without it `fetch` throws for a
    // POST with one. Harmless when there is no body.
    duplex: 'half'
  } as RequestInit);

  // 204 and 304 must not carry a body, and `new Response` with one throws.
  if (upstream.status === 204 || upstream.status === 304) {
    return new Response(null, { status: upstream.status });
  }

  return new Response(upstream.body, {
    status: upstream.status,
    headers: { 'content-type': upstream.headers.get('content-type') ?? 'application/json' }
  });
}
