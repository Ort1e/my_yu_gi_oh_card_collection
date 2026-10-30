// Pure helpers for cards and zones.

export const ZONES = ['main', 'extra', 'side'];

/**
 * The kind of a card from its type, as `yugioh_domain::stats::kind`: 0
 * monsters, 1 Extra Deck monsters, 2 spells, 3 traps, 4 the rest.
 */
export function kindRank(cardType) {
    const type = (cardType || '').toLowerCase();

    if (type.includes('spell')) return 2;
    if (type.includes('trap')) return 3;
    if (['fusion', 'synchro', 'xyz', 'link'].some((extra) => type.includes(extra))) return 1;
    if (type.includes('monster')) return 0;

    return 4;
}

/** As `yugioh_domain::card::sort_name`: trimmed, lowercase, no accents. */
export function sortName(name) {
    return (name || '')
        .trim()
        .toLowerCase()
        .normalize('NFD')
        .replace(/\p{M}/gu, '')
        .replace(/æ/g, 'ae')
        .replace(/œ/g, 'oe');
}

/**
 * Sort like the read-only page (`/deck-versions/:id`): by kind, then English
 * name ignoring case and accents (the copies of a card together, whatever
 * their language), then the name shown, then the id.
 */
export function sortCards(cards) {
    const key = (c) => [kindRank(c.card_type), sortName(c.name_en || c.name), sortName(c.name), c.id];

    const compare = (x, y) => (x < y ? -1 : x > y ? 1 : 0);

    return [...cards].sort((a, b) => {
        const ka = key(a);
        const kb = key(b);

        return compare(ka[0], kb[0]) || compare(ka[1], kb[1]) || compare(ka[2], kb[2]) || compare(ka[3], kb[3]);
    });
}

export function zonePrice(cards) {
    return cards.reduce((sum, c) => sum + (c.price ? Number(c.price) : 0), 0);
}

export const euros = (n) => `${n.toFixed(2)}€`;

/** "Banned" | "Limited" | "Semi-Limited" | undefined for the chosen ban list. */
export function banStatus(card, banListId) {
    return banListId == null ? undefined : card.ban?.[String(banListId)];
}

/** The English name, when it isn't the name already shown (a French card). */
export function englishName(card) {
    const english = card.name_en;

    return english && english.toLowerCase() !== card.name.trim().toLowerCase() ? english : null;
}

export function imageUrl(card) {
    return card.image ?? null;
}

/** Random sample of `n` cards, without replacement. */
export function draw(cards, n) {
    const a = [...cards];

    for (let i = a.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [a[i], a[j]] = [a[j], a[i]];
    }

    return a.slice(0, Math.min(n, a.length));
}

/** YDKE URL of the zones: base64 of the ygopro ids as little-endian u32. */
export function ydkeUrl(zones, keep = () => true) {
    const part = (cards) => {
        const ids = cards
            .filter((c) => keep(c) && c.ygopro_id != null)
            .map((c) => c.ygopro_id);

        const bytes = new Uint8Array(ids.length * 4);
        const view = new DataView(bytes.buffer);

        ids.forEach((id, i) => view.setUint32(i * 4, id, true));

        return btoa(String.fromCharCode(...bytes));
    };

    return `ydke://${part(zones.main)}!${part(zones.extra)}!${part(zones.side)}!`;
}

/** "1st", "2nd", "3rd", "4th", "11th", "21st"… (domain::tournament::ordinal). */
export function ordinal(n) {
    const suffix = [11, 12, 13].includes(n % 100) ? 'th' : { 1: 'st', 2: 'nd', 3: 'rd' }[n % 10] ?? 'th';

    return `${n}${suffix}`;
}
