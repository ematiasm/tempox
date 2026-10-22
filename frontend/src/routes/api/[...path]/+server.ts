import type { RequestHandler } from './$types.js';
import { forward } from '#lib/apiProxy.js';

/**
 * The app's own `/api/*`, proxied to the backend.
 *
 * `odd/tasks/docker-prod.md`, S8. The rule lives in `#lib/apiProxy.js`; this file
 * only wires methods to it, because SvelteKit refuses any export here that is not
 * an HTTP method — measured: exporting the helper from this module answered 500
 * with `invalid_export`.
 *
 * `[...path]` (rest parameter) so one route covers every endpoint, including the
 * nested ones like `/api/purchases/:id/supplier-returns`. The backend already owns
 * the API's shape; restating it here would be a second copy to keep in step.
 *
 * Nothing in this file knows which endpoints exist, and that is the point.
 */
export const GET: RequestHandler = ({ request }) => forward(request);
export const POST: RequestHandler = ({ request }) => forward(request);
export const PUT: RequestHandler = ({ request }) => forward(request);
export const PATCH: RequestHandler = ({ request }) => forward(request);
export const DELETE: RequestHandler = ({ request }) => forward(request);
