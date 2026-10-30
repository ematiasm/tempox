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
  nav_accounts: string;
  nav_customers: string;
  nav_suppliers: string;
  customers_title: string;
  customers_tagline: string;
  customers_empty: string;
  customers_error: string;
  customers_loading: string;
  party_panel_pending: string;
  party_panel_close: string;
  party_panel_identity: string;
  party_panel_balance: string;
  party_panel_documents: string;
  party_panel_owed: string;
  party_panel_owed_to_them: string;
  party_panel_in_favour: string;
  party_panel_in_our_favour: string;
  party_panel_none_unpaid: string;
  party_panel_not_found: string;
  party_panel_error: string;
  party_panel_loading: string;
  party_panel_not_found_short: string;
  party_panel_unpaid_sales: string;
  party_panel_unpaid_purchases: string;
  // ── The collection form in the balance box (`v5-payments.md` S9) ──
  /** The button that opens the form, per side: a client's money arrives, a supplier's leaves. */
  collect_action_customer: string;
  collect_action_supplier: string;
  collect_amount: string;
  collect_method: string;
  collect_manual_heading: string;
  /** What the unallocated remainder will do, in words, without naming documents (S9). */
  collect_remainder: (amount: string) => string;
  collect_remainder_none: string;
  collect_submit_customer: string;
  collect_submit_supplier: string;
  collect_cancel: string;
  /** One sentence per blocker, so the disabled submit names what it is waiting for. */
  collect_need_method: string;
  collect_need_total: string;
  collect_amount_hint: string;
  collect_over_allocated: string;
  collect_line_over_debt: string;
  collect_duplicate_line: string;
  collect_line_unreadable: string;
  collect_line_not_positive: string;
  collect_failed: string;
  /** The result of a collection, per applied row. `source` decides the sentence. */
  collect_applied_manual: (amount: string) => string;
  collect_applied_oldest: (amount: string) => string;
  collect_unapplied: (amount: string) => string;
  collect_recorded: string;
  suppliers_title: string;
  suppliers_tagline: string;
  suppliers_empty: string;
  suppliers_error: string;
  accounts_title: string;
  accounts_tagline: string;
  accounts_till: string;
  accounts_methods: string;
  accounts_balance: string;
  accounts_new: string;
  accounts_new_placeholder: string;
  accounts_new_method: string;
  accounts_new_method_placeholder: string;
  accounts_make_default: string;
  accounts_is_default: string;
  accounts_active: string;
  accounts_inactive: string;
  accounts_deactivate: string;
  accounts_activate: string;
  accounts_rename: string;
  accounts_save: string;
  accounts_empty: string;
  accounts_read_failed: string;
  accounts_loading: string;
  accounts_write_failed: string;
  accounts_till_hint: string;
  accounts_method_belongs: (account: string) => string;
  sales_method_failed: string;
  /** A document that moved no money says so instead of rendering an empty cell. */
  method_on_account: string;
  /** A method row whose name is empty is a defect, not an on-account document. */
  method_unknown: string;
  sales_method_default: string;
  sales_method: string;
  purchases_method: string;
  nav_main: string;
  /** Assistive-technology label for the language toggle. */
  lang_toggle: string;
  sales_title: string;
  sales_tagline: string;
  sales_action: string;
  sales_action_hint: string;
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
  sales_scope_day: string;
  sales_scope_week: string;
  sales_scope_month: string;
  sales_today: string;
  sales_yesterday: string;
  sales_more_one: string;
  sales_more_many: string;
  sales_scope_now: string;
  sales_older: string;
  sales_newer: string;
  sales_loading: string;
  sales_document_one: string;
  sales_documents: string;
  sales_ledger_error: string;
  sales_stale_title: string;
  sales_retry: string;
  sales_no_sales_today: string;
  sales_no_sales_day: string;
  sales_time: string;
  sales_open_document: string;
  sales_refunded_state: string;
  sales_running_balance: string;
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
  sales_shown_note_partial: (shown: number, total: number) => string;
  sales_bonus_lines: string;
  /** Receives the already-formatted amount, so the locale owns the figure (S12). */
  sales_bonus_lines_note: (amount: string) => string;
  sales_refund_action: string;
  sales_refund_confirm: string;
  sales_refund_yes: string;
  sales_refund_no: string;
  sales_refund_qty: string;
  sales_refund_include_col: string;
  sales_refund_include: (name: string) => string;
  sales_refund_left: (left: number, sold: number) => string;
  sales_refund_none: string;
  sales_refund_error: string;
  sales_refund_invalid: string;
  sales_refund_stale: string;
  sales_credit_note: string;
  sales_refunded: string;
  sales_refunded_note: string;
  /** Each message names the field it is about: one sentence for both was wrong. */
  products_invalid_price: string;
  products_invalid_stock: string;
  /** What the disabled create button is waiting for, field by field. */
  products_missing_name: string;
  products_missing_price: string;
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
  more_column: string;
  more_columns: string;
  products_products_shown: string;
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

  // ── Purchases (v2) ────────────────────────────────────────────────────────
  // A purchase is the mirror of a sale in the other direction: goods in, money
  // out. The vocabulary is the shop's, not a translation of the English code.
  nav_purchases: string;
  purchases_title: string;
  purchases_tagline: string;
  purchases_action: string;
  purchases_document: string;
  purchases_supplier: string;
  purchases_confirmed_state: string;
  purchases_returned_state: string;
  purchases_lines_of: string;
  purchases_footing_gross: string;
  purchases_footing_gross_note: string;
  purchases_footing_returned: string;
  purchases_footing_returned_note: string;
  purchases_footing_net: string;
  purchases_footing_net_note: string;
  purchases_action_hint: string;
  purchases_draft_title: string;
  purchases_ledger_title: string;
  purchases_ledger_sub: string;
  /** The seeded supplier that a purchase falls back to. Stored as `one-off-supplier`. */
  purchases_one_off_supplier: string;
  purchases_new_supplier: string;
  purchases_new_supplier_placeholder: string;
  purchases_supplier_existing: (name: string) => string;
  purchases_no_lines: string;
  purchases_live_total: string;
  purchases_price_unit: string;
  purchases_draft_hint: string;
  purchases_error_title: string;
  purchases_invalid: string;
  purchases_legend: string;
  purchases_no_purchases_today: string;
  purchases_no_purchases_day: string;
  purchases_document_one: string;
  purchases_documents: string;
  purchases_ledger_error: string;
  purchases_stale_title: string;
  purchases_supplier_required: string;
  purchases_load_suppliers_failed: string;
  purchases_create_supplier_error: string;

  // ── Supplier returns (v2.5) ──────────────────────────────────────────────
  // The mirror of the credit note, with the direction reversed: goods go back
  // to the supplier and money comes back. The vocabulary is the shop's.
  purchases_supplier_return: string;
  purchases_return_action: string;
  purchases_return_confirm: string;
  purchases_return_yes: string;
  purchases_return_no: string;
  /** The return quantity column's header. */
  purchases_return_qty: string;
  /** How much of a line is left to return, of what was bought. */
  purchases_return_left: (left: number, bought: number) => string;
  /** Shown when every line of the purchase has already gone back. */
  purchases_return_none: string;
  purchases_return_error: string;
  /**
   * Shown when the return DID reach the server but the day could not be re-read.
   *
   * Deliberately not the refusal prefix: telling an operator "not returned" when
   * the server has it invites a retry that the guard then refuses, or worse, a
   * second return of goods that already went back.
   */
  purchases_return_recorded: string;
  purchases_return_invalid: string;
  /** The return field's accessible name, which names the line it corrects. */
  purchases_return_line: (name: string) => string;

  // ── Expenses (v4-expenses) ──────────────────────────────────────────────
  nav_expenses: string;
  expenses_title: string;
  expenses_tagline: string;
  expenses_ledger_title: string;
  expenses_payee: string;
  expenses_payee_placeholder: string;
  expenses_category: string;
  expenses_amount: string;
  expenses_method: string;
  expenses_on_account: string;
  expenses_occurred_at: string;
  expenses_action: string;
  expenses_action_hint: string;
  expenses_recorded: string;
  expenses_empty: string;
  expenses_empty_day: string;
  expenses_spent: string;
  expenses_purchases: string;
  expenses_returned: string;
  expenses_net: string;
  expenses_paid: string;
  expenses_outstanding: string;
  expenses_amount_advanced: string;
  expenses_advance_action: string;
  expenses_advanced: string;
  expenses_advance_only_owed: string;
  /**
   * Said when the older-debts page is not the whole list (issue #115).
   *
   * A function rather than a string because the count is the point: "there are more" is a
   * shrug, and "12 older debts are not shown" is something the operator can act on. The
   * alternative — silently truncating — is exactly the defect this message closes.
   */
  expenses_older_owed_notice: (count: number) => string;
  /**
   * Said when **the day itself** is longer than one page.
   *
   * A different sentence from the one above on purpose: that one says there are older debts
   * this screen did not render, this one says the day holds more documents than the stack
   * below it — and only the second means the day's own figures describe a day the reader
   * cannot see in full (`R2-001` of `review-16c84931d9274aaf`).
   */
  expenses_day_truncated: string;
  /** The pager's back step, on the day's own stack. */
  expenses_page_older: string;
  /** The pager's forward step. */
  expenses_page_newer: string;
  /**
   * Which page of the day is shown, out of how many.
   *
   * A function rather than a string because both numbers are the point: "page 2 of 5" tells a
   * reader where they are in a day they cannot see at once, which is what the notice above it
   * cannot say on its own.
   */
  expenses_page_of: (page: number, pages: number) => string;
  /**
   * Which page of a ledger is shown, out of how many — shared by the three ledgers that page.
   *
   * One key for three screens on purpose: the expenses pager, the sales ledger and the
   * purchases ledger are answering the same question, and three hand-maintained strings would
   * drift the way the three implementations of the arithmetic did.
   */
  ledger_page_of: (page: number, pages: number) => string;
  /** "showing N of M documents", said when a ledger page is not the whole span. */
  ledger_shown_partial: (shown: number, total: number) => string;
  expenses_category_new: string;
  expenses_category_new_action: string;
  documents_one: string;
  documents_other: string;

  expenses_read_failed: string;
  expenses_write_failed: string;
  expenses_method_till: string;

  expenses_loading: string;
}
