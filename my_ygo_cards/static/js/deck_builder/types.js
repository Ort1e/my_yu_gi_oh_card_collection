/**
 * @typedef {Object} DeckUpdateData
 * @property {string[]} [main_deck]
 * @property {string[]} [extra_deck]
 * @property {string[]} [side_deck]
 * @property {string} [version_name]
 * @property {string} [ydke_url]
 */

/**
 * @typedef {Object} ApiUrls
 * @property {() => string} deckVersionDetail
 * @property {() => string} deckVersionClone
 * @property {() => string} deckVersionProxyCreate
 * @property {(card_id: string | number) => string} deckVersionProxyDelete
 * @property {() => string} deckVersionCardLists
 * @property {() => string} deckVersionCategories
 * @property {(card_id: string | number) => string} deckVersionAssignCategories
 * @property {(category_id: string | number) => string} deckVersionCategoryDelete
 * @property {() => string} deckImportYdke
 * @property {() => string} deckListUrl
 * @property {(deck_version_id: string | number) => string} deckBuilderUrl
 * @property {(card_id: string | number) => string} cardBaseUrl
 * @property {() => string} monteCarloSimulationUrl
 */