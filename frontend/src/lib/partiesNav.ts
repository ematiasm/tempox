/**
 * The two destinations that are about a party rather than a document.
 *
 * `v5-customer-account.md` S1 and S7: `/clientes` and `/proveedores` are separate
 * pages, not tabs, because they are separate jobs in the shop's day and the rail
 * marks one active. This module exists so the rail's list and the pages' identity
 * cannot drift apart.
 *
 * **The route is not translated; the label is.** A URL is a contract — a cashier
 * pastes it, a bookmark keeps it, a test asserts it — so `/clientes` stays
 * `/clientes` even in `en`. What changes is the word rendered on the link.
 *
 * It returns both languages rather than reading the active one, for the same reason
 * `locale.ts` exposes both: the unit test can then compare them without depending on
 * which language the test process happens to be in.
 */
export type PartyTab = { href: string; label: string };

export const partiesNav: { es: PartyTab[]; en: PartyTab[] } = {
  es: [
    { href: '/clientes', label: 'Clientes' },
    { href: '/proveedores', label: 'Proveedores' }
  ],
  en: [
    { href: '/clientes', label: 'Customers' },
    { href: '/proveedores', label: 'Suppliers' }
  ]
};
