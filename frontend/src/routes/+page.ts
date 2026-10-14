import { redirect } from '@sveltejs/kit';

// The shop opens on the day it is working through, not on a menu. `/` is the
// only route that has nothing to show, so it sends the visitor to the screen
// the business actually starts from.
export function load() {
  redirect(307, '/sales');
}
