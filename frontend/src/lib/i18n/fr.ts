import type { Dict } from './types';

const fr: Dict = {
  title: 'Tempox — ventes et stock',
  tagline: 'Enregistrez une vente une fois ; le stock et la caisse suivent.',
  greeting_button: 'Dire bonjour',
  greeting_placeholder: 'Ton nom',
  notes_title: 'Notes',
  notes_placeholder: 'Écris une note…',
  notes_add: 'Ajouter',
  notes_empty: 'Aucune note pour le moment.',
  lang_switch: 'EN',
  nav_sales: 'Ventes',
  nav_products: 'Produits',
  sales_title: 'Enregistrer une vente',
  sales_tagline: 'Le stock et la caisse se mettent à jour ensemble, depuis la même saisie',
  sales_action: 'Enregistrer',
  sales_draft_title: 'Nouvelle vente',
  sales_draft_pending: 'Le brouillon est construit en T5.',
  sales_ledger_title: 'Journal des ventes',
  sales_ledger_sub:
    'Chaque vente est un document ; la journée se lit de haut en bas, la plus récente en premier',
  sales_ledger_pending:
    'Le journal, sa portée par date et le pied de caisse sont construits en T6.',
  products_title: 'Produits',
  products_tagline: 'Le stock est dérivé du journal des mouvements, jamais saisi à la main',
  products_action: 'Nouveau produit',
  products_catalog_title: 'Catalogue',
  products_catalog_sub:
    "La recherche s'exécute côté serveur sur le nom ; l'autocomplétion du formulaire de vente utilise la même requête",
  products_catalog_pending:
    'Le catalogue, sa recherche et le formulaire de création sont construits en T7.',
  sales_product: 'Produit',
  sales_customer: 'Client',
  sales_walkin: 'Passage — Consumidor Final',
  sales_new_customer: 'Nouveau client',
  sales_new_customer_placeholder: 'Nom du nouveau client…',
  sales_customer_existing: (name: string) =>
    `« ${name} » existe déjà — il est sélectionné comme client de cette vente.`,
  sales_qty: 'Qté',
  sales_price: 'Prix',
  sales_line: 'Total',
  sales_remove: 'Retirer',
  sales_no_lines:
    'Aucune ligne — choisissez un produit ci-dessus : il arrive ici avec son prix prérempli.',
  sales_live_total: 'Total en direct',
  sales_line_one: 'ligne',
  sales_lines: 'lignes',
  sales_pieces: 'unités de base',
  sales_bonus_note: 'dont une ligne offerte à 0,00 $',
  sales_record: 'Enregistrer',
  sales_recording: 'Enregistrement…',
  sales_clear: 'Vider',
  sales_invalid: 'Un champ ne contient pas un nombre valide — corrigez-le avant d’enregistrer.',
  sales_error_title: 'Non enregistré : ',
  sales_confirmed: 'Confirmé',
  sales_draft_hint: 'Ctrl+Entrée enregistre',
  sales_price_unit: '$/unité',
  sales_recorded_this_session:
    ' enregistrée(s) dans cette session — la liste du journal est construite en T6.'
};

export default fr;
