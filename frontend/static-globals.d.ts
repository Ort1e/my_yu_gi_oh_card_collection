// What the page scripts (static/js/) share at runtime, for their check
// (tsconfig.static.json checks each one in its own scope). Here, in
// frontend/, so it is never shipped with static/.

// ---------- static/js/rarities.js (the Yu-Gi-Oh! rarity guide's data) ----------

/** How to recognise a Yu-Gi-Oh! rarity; the lines it has. */
type Rarity = {
    name: string;
    aliases?: string[];
    summary: string;
    cardName?: string;
    artwork?: string;
    rest?: string;
    lookAlikes?: string;
};

declare const RARITIES: Rarity[];
declare const UNDESCRIBED: string[];
declare function findRarity(name: string): Rarity | null;
declare function rarityAnchor(name: string): string;

// ---------- static/js/pokemon_rarities.js (the Pokémon guide's data) ----------

/** A Pokémon variant (TCGdex's key → this). */
type PokemonVariant = {
    label: string;
    look: string;
};

/** How to recognise a Pokémon rarity. */
type PokemonRarity = {
    name: string;
    aliases?: string[];
    symbol?: string;
    look?: string;
    japanese?: string;
    [line: string]: string | string[] | undefined;
};

declare const POKEMON_VARIANTS: Record<string, PokemonVariant>;
declare const POKEMON_RARITIES: PokemonRarity[];
declare const POKEMON_UNDESCRIBED: string[];
declare function findPokemonRarity(name: string | null | undefined): PokemonRarity | null;
declare function pokemonRarityAnchor(name: string): string;

// ---------- from a CDN ----------

/** Chart.js, loaded from its CDN on the dashboard and the value page. */
declare const Chart: any;

// ---------- what lib.dom lacks ----------

/** The browser's barcode reader (Chrome on Android), static/js/book_form.js. */
declare class BarcodeDetector {
    constructor(options?: { formats: string[] });
    static getSupportedFormats(): Promise<string[]>;
    detect(source: CanvasImageSource): Promise<{ rawValue: string }[]>;
}
