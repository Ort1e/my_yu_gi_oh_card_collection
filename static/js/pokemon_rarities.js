// How to recognise each Pokémon card rarity and variant on the physical
// card. Used by the /pokemon/rarities page (static/js/pokemon_rarity_guide.js),
// the card finder (static/js/pokemon_finder.js) and the booster form
// (static/js/pokemon_booster.js).
//
// Compiled in October 2026 from the Pokémon Company's announcement of the
// Scarlet & Violet rarity symbols, CGC's and Dexerto's symbol guides and
// Japanese retailers' rarity guides. `name`: TCGdex's label (the one the app
// shows); `aliases`: TCGdex's other spellings of the same rarity. `japanese`:
// the letters printed on the Japanese cards of the same tier.

const POKEMON_VARIANTS = {
    normal: {
        label: 'Normal',
        look: 'Nothing shines.',
    },
    holo: {
        label: 'Holo',
        look: 'Only the picture shines (a rainbow foil); the border and text are matte.',
    },
    reverse: {
        label: 'Reverse holo',
        look: 'Everything but the picture shines: border, name, text box. One per booster, any rarity; some sets use a Poké Ball or Master Ball pattern.',
    },
    firstEdition: {
        label: '1st edition',
        look: 'A black "Edition 1" stamp at the left, under the picture (sets of 1999 to 2002).',
    },
    wPromo: {
        label: 'W promo',
        look: 'A "W" stamp: a Wizards of the Coast promo (1999 to 2003).',
    },
};

const POKEMON_RARITIES = [
    {
        name: 'Common',
        symbol: '● black circle',
        look: 'No foil (its reverse holo shines around the picture).',
        japanese: 'C',
    },
    {
        name: 'Uncommon',
        symbol: '◆ black diamond',
        look: 'No foil (its reverse holo shines around the picture).',
        japanese: 'U',
    },
    {
        name: 'Rare',
        symbol: '★ black star',
        look: 'From Scarlet & Violet on: the picture is holo. Before: no foil.',
        japanese: 'R',
    },
    {
        name: 'Holo Rare',
        aliases: ['Rare Holo'],
        symbol: '★ black star',
        look: 'The picture shines, the rest is matte (before Scarlet & Violet).',
        japanese: 'R',
    },
    {
        name: 'Double rare',
        symbol: '★★ two black stars',
        look: 'A Pokémon ex (Scarlet & Violet), holo picture in a frame: not a full art.',
        japanese: 'RR',
        lookAlikes: 'Ultra Rare: the same ex drawn over the whole card, two silver stars.',
    },
    {
        name: 'Holo Rare V',
        aliases: ['Holo Rare VMAX', 'Holo Rare VSTAR'],
        symbol: '★ black star',
        look: 'A Pokémon V, VMAX or VSTAR (Sword & Shield) in a frame, holo: not a full art.',
        japanese: 'RR, RRR',
        lookAlikes: 'Ultra Rare: the full-art V, textured all over.',
    },
    {
        name: 'Ultra Rare',
        symbol: '★★ two silver stars (Scarlet & Violet); ★ black before',
        look: 'Full art: a Pokémon ex, V or GX, or a Supporter, drawn over the whole card, the surface textured. Number above the set size.',
        japanese: 'SR',
        lookAlikes: 'Special Illustration Rare: two gold stars, a painted scene.',
    },
    {
        name: 'Illustration rare',
        symbol: '★ one gold star',
        look: 'Full art: a common Pokémon painted over the whole card, in an artist\'s own style, often a scene. Number above the set size.',
        japanese: 'AR',
    },
    {
        name: 'Special illustration rare',
        symbol: '★★ two gold stars',
        look: 'Full art: a Pokémon ex or a Supporter painted in a scene over the whole card. The chase cards of a set.',
        japanese: 'SAR',
        lookAlikes: 'Ultra Rare: two silver stars, a plainer textured full art.',
    },
    {
        name: 'Hyper rare',
        symbol: '★★★ three gold stars',
        look: 'The whole card gold and textured: a gold ex, Supporter, Item or Energy.',
        japanese: 'UR',
        lookAlikes: 'Secret Rare: the gold cards before Scarlet & Violet.',
    },
    {
        name: 'Secret Rare',
        symbol: 'any star; the number is above the set size (201/185)',
        look: 'Before Scarlet & Violet: rainbow cards (pastel rainbow foil over a full art) and gold cards.',
        japanese: 'HR, UR',
    },
    {
        name: 'Full Art Trainer',
        symbol: '★ black star',
        look: 'A Supporter or Item drawn over the whole card, textured (Sun & Moon, Sword & Shield).',
        japanese: 'SR',
    },
    {
        name: 'ACE SPEC Rare',
        symbol: '★ pink star',
        look: '"ACE SPEC" written on the card: a powerful Trainer, one per deck. Holo.',
        japanese: 'ACE',
    },
    {
        name: 'Shiny rare',
        symbol: '★ gold star with a sparkle',
        look: 'A shiny (other colour) Pokémon, holo picture, not a full art (Paldean Fates and other shiny sets).',
        japanese: 'S',
    },
    {
        name: 'Shiny Ultra Rare',
        aliases: ['Shiny rare V', 'Shiny rare VMAX'],
        symbol: '★★ gold stars with a sparkle; or a number like SV107',
        look: 'A shiny Pokémon ex or V in full art, on a silver textured background.',
        japanese: 'SSR',
    },
    {
        name: 'Radiant Rare',
        symbol: '★ black star, "Radiant" in the name',
        look: 'A shiny Pokémon on a stylised, sparkly holo background (Sword & Shield). One per deck.',
        japanese: 'K',
    },
    {
        name: 'Amazing Rare',
        symbol: '★ black star',
        look: 'The Pokémon bursts out of its frame on a rainbow splash (Sword & Shield, 2020-2021).',
        japanese: 'A',
    },
    {
        name: 'Mega Hyper Rare',
        symbol: 'its own symbol (Mega Evolution sets, from 2025)',
        look: 'The top card of a Mega Evolution set: compare with the set\'s list.',
        japanese: 'MUR',
    },
    {
        name: 'RGB Rare',
        symbol: 'no number: the letter R, G or B instead',
        look: 'The three Mew cards of 30th Celebration (2026), in red, green and blue, numbered R, G and B. Type the letter in the booster form.',
        japanese: '',
    },
    {
        name: 'Futuristic Rare',
        symbol: 'its own symbol; number above the set size',
        look: 'The top card of 30th Celebration (2026): Mew ex, number 158.',
        japanese: '',
    },
    {
        name: 'Classic Collection',
        aliases: ['None', 'Sans Rareté'],
        symbol: 'no rarity symbol',
        look: 'A famous old card reprinted in its original look with an anniversary stamp (Celebrations 2021, 30th Celebration 2026). In 30th Celebration they share the code 30C with the main set: their numbers end in /030.',
        japanese: '',
    },
    {
        name: 'Promo',
        symbol: 'a black star with "PROMO", or a code like SVP 001',
        look: 'Given in a product or an event; no rarity tier.',
        japanese: 'P',
    },
];

// Rarities TCGdex knows and the guide doesn't describe yet (old or rare ones).
const POKEMON_UNDESCRIBED = [
    'Black White Rare', 'Crown', 'LEGEND', 'Mega Attack Rare', 'Pikachu Rare',
    'Rare Holo LV.X', 'Rare PRIME',
];

const pokemonFold = (text) => text.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase().trim();

/** The guide's entry for a rarity name (any spelling or alias), or null. */
function findPokemonRarity(name) {
    const wanted = pokemonFold(name ?? '');

    return POKEMON_RARITIES.find((r) => [r.name, ...(r.aliases ?? [])].some((n) => pokemonFold(n) === wanted)) ?? null;
}

/** `Special illustration rare` → `special-illustration-rare`. */
function pokemonRarityAnchor(name) {
    return pokemonFold(name).replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}
