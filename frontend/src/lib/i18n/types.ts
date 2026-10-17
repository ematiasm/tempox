/**
 * The languages the interface speaks.
 *
 * Two, and only two (S2): the shop's own language and the language the specs and
 * the code are written in. `fr` was the placeholder that proved the i18n slot
 * worked; it was never a product requirement, and a third hand-maintained
 * dictionary is a cost with no reader.
 */
export type Lang = 'es-AR' | 'en';

export interface Dict {
  notes_title: string;
  notes_placeholder: string;
  notes_add: string;
  notes_empty: string;
  nav_sales: string;
  nav_products: string;
  /** Assistive-technology label for the rail (`Main`). */
  nav_main: string;
  /** Assistive-technology label for the language toggle. */
  lang_toggle: string;
  sales_title: string;
  sales_tagline: string;
  sales_action: string;
  sales_draft_title: string;
  sales_ledger_title: string;
  sales_ledger_sub: string;
  products_title: string;
  products_tagline: string;
  products_action: string;
  products_catalog_title: string;
  products_catalog_sub: string;
  sales_product: string;
  sales_customer: string;
  sales_walkin: string;
  sales_new_customer: string;
  sales_new_customer_placeholder: string;
  sales_customer_existing: (name: string) => string;
  sales_qty: string;
  sales_price: string;
  sales_line: string;
  sales_remove: string;
  sales_no_lines: string;
  sales_live_total: string;
  sales_line_one: string;
  sales_lines: string;
  sales_pieces: string;
  /** Receives the already-formatted amount, so the locale owns the figure (S12). */
  sales_bonus_note: (amount: string) => string;
  sales_clear: string;
  sales_invalid: string;
  sales_error_title: string;
  sales_confirmed: string;
  sales_draft_hint: string;
  sales_price_unit: string;
  sales_scope: string;
  sales_today: string;
  sales_yesterday: string;
  sales_older: string;
  sales_newer: string;
  sales_legend_confirmed: string;
  sales_legend_annulled: string;
  sales_loading: string;
  sales_document_one: string;
  sales_documents: string;
  sales_ledger_error: string;
  sales_retry: string;
  sales_no_sales_today: string;
  sales_no_sales_day: string;
  sales_time: string;
  sales_sale: string;
  sales_items: string;
  sales_status: string;
  sales_total: string;
  sales_annulled: string;
  sales_lines_of: string;
  sales_unit_price: string;
  sales_bonus: string;
  sales_cash_on_hand: string;
  sales_cash_note: string;
  sales_shown_sales: string;
  sales_shown_note: string;
  sales_bonus_lines: string;
  /** Receives the already-formatted amount, so the locale owns the figure (S12). */
  sales_bonus_lines_note: (amount: string) => string;
  sales_refund_action: string;
  sales_refund_confirm: string;
  sales_refund_yes: string;
  sales_refund_no: string;
  sales_refund_qty: string;
  sales_refund_left: (left: number, sold: number) => string;
  sales_refund_none: string;
  sales_refund_error: string;
  sales_refund_invalid: string;
  sales_credit_note: string;
  sales_refunded: string;
  sales_refunded_note: string;
  products_invalid: string;
  products_search: string;
  products_error: string;
  products_no_match: string;
  products_clear_search: string;
  products_empty: string;
  products_create_first: string;
  products_unit: string;
  products_list_price: string;
  products_stock: string;
  products_product_one: string;
  products_products: string;
  products_derived: string;
  products_derived_note: string;
  products_new_title: string;
  products_new_sub: string;
  products_name: string;
  products_name_placeholder: string;
  products_initial_stock: string;
  products_create: string;
  products_creating: string;
  products_created: (name: string) => string;
  products_create_error: string;
  /** The product picker: field label, listbox label, and its four states. */
  picker_placeholder: string;
  picker_label: string;
  picker_matches: string;
  picker_searching: string;
  picker_search_failed: string;
  picker_no_match: string;
  picker_empty: string;
  picker_in_stock: (stock: string) => string;
  /** The draft's sr-only remove-column header. */
  sales_remove_column: string;
  /** The refund field's accessible name, which names the line it corrects. */
  sales_return_line: (name: string) => string;
}
