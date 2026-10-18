// How to recognise each Yu-Gi-Oh! rarity on a physical card. Used by the
// /rarities page (templates/reference/rarities.html) and the quick-add panel
// (static/js/card_rows.js).
//
// Compiled in October 2026 from TCGplayer's "Understanding Yu-Gi-Oh!
// Rarities" (seller.tcgplayer.com), Yugipedia's rarity pages, retailer
// descriptions (Grand Master Rare, Battle Pack foils) and the rarity names
// ygoprodeck uses. `aliases`: other names of the same look. Looks have
// changed over the years and between releases: when in doubt, compare with a
// card whose rarity you know. Names match the rarity list of the app
// (docs/specs/rarity.md).

const RARITIES = [
    {
        name: 'Common',
        summary: 'No foil anywhere.',
        cardName: 'Plain black or white print.',
        artwork: 'Plain, no foil.',
        rest: 'Nothing special.',
    },
    {
        name: 'Rare',
        summary: 'Silver foil name, plain artwork.',
        cardName: 'Silver foil.',
        artwork: 'Plain, no foil.',
        rest: 'Nothing else.',
    },
    {
        name: 'Super Rare',
        summary: 'Holo artwork, plain name.',
        cardName: 'Plain, no foil.',
        artwork: 'Holofoil.',
        rest: 'Since mid-2017, the Attribute and the Level/Rank are foil too.',
        lookAlikes: 'An Ultra Rare looks the same with a gold name.',
    },
    {
        name: 'Ultra Rare',
        summary: 'Gold foil name + holo artwork.',
        cardName: 'Gold foil.',
        artwork: 'Holofoil, as on a Super Rare.',
        rest: 'Attribute and Level/Rank foil on recent cards.',
        lookAlikes: 'Ultimate Rare also has a gold name, but its artwork is embossed (you can feel it).',
    },
    {
        name: 'Ultimate Rare',
        summary: 'Gold name, embossed artwork you can feel.',
        cardName: 'Gold.',
        artwork: 'Foil and embossed: a raised texture.',
        rest: 'Attribute and Level embossed too; on recent prints the card border as well.',
        lookAlikes: 'Ultra Rare: same gold name, but a flat artwork.',
    },
    {
        name: 'Secret Rare',
        summary: 'Rainbow foil name + rainbow (diagonal) foil artwork.',
        cardName: 'Rainbow foil.',
        artwork: 'Rainbow foil with the Secret Rare pattern.',
        rest: 'Attribute and Level/Rank in the same rainbow foil. The border and text box are plain.',
        lookAlikes: 'The other "Secret" rarities spread the foil further: the border (Platinum Secret, Quarter Century) or the whole card.',
    },
    {
        name: 'Short Print',
        aliases: ['Super Short Print'],
        summary: 'Looks exactly like a Common.',
        cardName: 'Plain.',
        artwork: 'Plain.',
        rest: 'Only how often it was packed differs: you can\'t tell it from a Common by looking.',
    },
    {
        name: 'Ghost Rare',
        summary: 'Silvery-white name, ghostly 3D hologram artwork.',
        cardName: 'Silvery-white.',
        artwork: 'A semi-transparent, ghostly 3D hologram instead of the usual picture.',
        rest: 'Nothing else.',
    },
    {
        name: 'Gold Rare',
        summary: 'Gold name, gold card borders, holo artwork.',
        cardName: 'Gold.',
        artwork: 'Holographic.',
        rest: 'Gold card borders: the easiest sign.',
    },
    {
        name: 'Gold Secret Rare',
        summary: 'Gold borders like a Gold Rare, Secret Rare pattern on the name and artwork.',
        cardName: 'Gold, with the Secret Rare pattern.',
        artwork: 'Secret Rare pattern.',
        rest: 'Gold card borders.',
        lookAlikes: 'Gold Rare: same borders, plain holo artwork.',
    },
    {
        name: 'Ghost/Gold Rare',
        summary: 'Ghost Rare artwork with gold card borders.',
        cardName: 'Gold.',
        artwork: 'Ghostly 3D hologram, as on a Ghost Rare.',
        rest: 'Gold card borders.',
    },
    {
        name: 'Premium Gold Rare',
        summary: 'Embossed gold border, icons and parts of the artwork (Maximum Gold).',
        cardName: 'Gold.',
        artwork: 'Parts of it embossed in gold.',
        rest: 'Embossed gold on the card border and the icons. Only in the Maximum Gold sets.',
    },
    {
        name: 'Platinum Rare',
        summary: 'Foil on almost the whole card except the text box, Secret-style name.',
        cardName: 'Secret Rare style.',
        artwork: 'Foil.',
        rest: 'Foil over nearly all the card outside the text box. Only in the Noble Knights of the Round Table box set.',
    },
    {
        name: 'Platinum Secret Rare',
        summary: 'Platinum (silver-grey) foil with the Secret pattern, platinum name.',
        cardName: 'Platinum, with a shattered foil look.',
        artwork: 'Platinum foil with the Secret Rare pattern.',
        rest: 'Depends on the release: on Rarity Collection cards, also the silver frame of the artwork, the text box border and the Attribute; on older tins, almost the whole card face except the text box.',
        lookAlikes: 'Secret Rare is rainbow, not platinum. Collector\'s Rare is more colourful and textured.',
    },
    {
        name: 'Collector\'s Rare',
        summary: 'Rainbow pattern on the borders and artwork, slightly raised "fingerprint" texture.',
        cardName: 'Secret Rare style.',
        artwork: 'Rainbow pattern.',
        rest: 'Card border, text box border, Attribute and Level/Rank in the same rainbow pattern, with a slightly raised, engraved feel; the foil runs in irregular directions, often compared to fingerprints.',
        lookAlikes: 'A more muted cousin of Platinum Secret Rare. Starlight Rare has straight horizontal lines instead of an irregular pattern.',
    },
    {
        name: 'Starlight Rare',
        summary: 'Horizontal grid-like holo over almost the whole card, border included.',
        cardName: 'Foil, part of the overall pattern.',
        artwork: 'The same horizontal, lattice-like holo pattern.',
        rest: 'The pattern covers nearly the whole card face, border included; only the text box (and parts of the artwork) are left plain.',
        lookAlikes: 'Collector\'s Rare also covers the border, but with an irregular, textured pattern. Prismatic Secret Rare has the same lines on the artwork only.',
    },
    {
        name: 'Prismatic Secret Rare',
        summary: 'Starlight-like horizontal grid pattern, on the artwork only.',
        cardName: 'Secret Rare style.',
        artwork: 'Horizontal, lattice-like holo pattern.',
        rest: 'Nothing beyond the artwork.',
    },
    {
        name: 'Grand Master Rare',
        summary: 'Hieroglyphic card borders and a serial number (001 to 100).',
        cardName: 'Not described yet.',
        artwork: 'Reimagined anime artwork, possibly extended.',
        rest: 'Hieroglyphic borders and a serial number from 001 to 100 (ending in "E" for copies sold in Europe). Only 100 copies of each of the 18 cards (Magnificent Monsters, 2026); always in English.',
    },
    {
        name: 'Starfoil Rare',
        aliases: ['Starfoil'],
        summary: 'Foil over the whole card with a pattern of stars.',
        cardName: 'Under the same foil.',
        artwork: 'Under the same foil.',
        rest: 'The foil covers the entire card; look for the stars in it. Star Packs, Battle Pack: Epic Dawn.',
    },
    {
        name: 'Mosaic Rare',
        summary: 'Foil over the whole card with a pattern of small squares.',
        cardName: 'Under the same foil.',
        artwork: 'Under the same foil.',
        rest: 'Square tiles, like pixels, over the entire card. Battle Pack 2: War of the Giants.',
    },
    {
        name: 'Shatterfoil Rare',
        summary: 'Foil over the whole card that looks like shattered glass.',
        cardName: 'Under the same foil.',
        artwork: 'Under the same foil.',
        rest: 'A reflective, broken-glass pattern over the entire card. Battle Pack 3: Monster League.',
    },
    {
        name: 'Ultra Parallel Rare',
        aliases: ['Normal Parallel Rare', 'Super Parallel Rare'],
        summary: 'The base rarity\'s look plus a hazy holo layer over the whole card.',
        cardName: 'As the base rarity (gold for Ultra, plain for Normal and Super).',
        artwork: 'As the base rarity, under the layer.',
        rest: 'A holographic coating over the entire card with a hazy gloss. Duel Terminal cards have their own Parallel Rares.',
    },
    {
        name: 'Quarter Century Secret Rare',
        summary: 'Champagne-gold speckled name, 25th-anniversary logo watermark in the text box.',
        cardName: 'Speckled, champagne-gold foil.',
        artwork: 'Secret Rare foil.',
        rest: 'Secret Rare foil over the whole card, and a watermark of the "25th Quarter Century" logo in the middle of the text box: the easiest sign.',
        lookAlikes: 'Secret Rare: rainbow name, no watermark.',
    },
];

/** Rarities ygoprodeck lists that have no description here (yet). */
const UNDESCRIBED = [
    'Ultra Rare (Pharaoh\'s Rare)',
    'Extra Secret Rare',
    'Ultra Secret Rare',
    '10000 Secret Rare',
    'Duel Terminal Parallel Rares',
];

/** The rarity of a name, ignoring case and punctuation ("PLatinum Secret Rare",
 * "Collectors Rare") and accepting its aliases. */
function findRarity(name) {
    const key = (text) => text.toLowerCase().replace(/[^a-z]/g, '');
    const wanted = key(name);

    return RARITIES.find((r) => [r.name, ...(r.aliases ?? [])].some((n) => key(n) === wanted)) ?? null;
}

/** The anchor of a rarity on the /rarities page: "Ultra Rare" → "ultra-rare". */
function rarityAnchor(name) {
    return name.toLowerCase().replace(/[^a-z]+/g, '-').replace(/^-|-$/g, '');
}
