// The lang_switch value shows the label of the OTHER language (so when EN is
// active the button offers to switch to FR).
import type { Dict } from './types';

const en: Dict = {
  title: 'Tempox — sales and stock',
  tagline: 'Record a sale once; stock and cash follow.',
  greeting_button: 'Say hello',
  greeting_placeholder: 'Your name',
  notes_title: 'Notes',
  notes_placeholder: 'Write a note…',
  notes_add: 'Add',
  notes_empty: 'No notes yet.',
  lang_switch: 'FR',
  nav_sales: 'Sales',
  nav_products: 'Products',
  sales_title: 'Record a sale',
  sales_tagline: 'Stock and cash update together, from the one entry',
  sales_action: 'Record sale',
  sales_draft_title: 'New sale',
  sales_draft_pending: 'The draft sheet is built in T5.',
  sales_ledger_title: 'Sales ledger',
  sales_ledger_sub: 'Every sale is a document; the day reads top-down, newest first',
  sales_ledger_pending: 'The ledger, its date scoping and the cash foot are built in T6.',
  products_title: 'Products',
  products_tagline: 'Stock is derived from the movement ledger, never typed by hand',
  products_action: 'New product',
  products_catalog_title: 'Catalog',
  products_catalog_sub:
    "Search runs server-side on name; the sale form's type-ahead uses the same query",
  products_catalog_pending: 'The catalog, its search and the create form are built in T7.'
};

export default en;
