// Pure helpers for cards and zones.

export const ZONES = ['main', 'extra', 'side'];

/** Sort like Django's builder: by card type, then English name. */
export function sortCards(cards) {
    const key = (c) => [
        (c.card_type || '').toLowerCase(),
        (c.en_name || c.name || '').toLowerCase(),
    ];

    return [...cards].sort((a, b) => {
        const [ta, na] = key(a);
        const [tb, nb] = key(b);

        return ta.localeCompare(tb) || na.localeCompare(nb);
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
    return card.card_data_id ? `/card-image/${card.card_data_id}` : null;
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
