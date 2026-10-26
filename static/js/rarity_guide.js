// The rarity guides, /rarities (templates/reference/rarities.html) and
// /pokemon/rarities (templates/pokemon/rarities.html): one box per rarity
// (and per Pokémon variant) from the data files, static/js/rarities.js and
// static/js/pokemon_rarities.js, then the box asked for. The page says which
// guide: <div id="rarity-list" data-guide="yugioh"> or "pokemon".

const line = (label, text) => (text ? `<div><strong>${label}:</strong> ${text}</div>` : '');

/** One box per entry: its name and aliases, its summary, then its lines
 * (`[label, key]`, only those it has). */
function addBoxes(list, entries, { anchor, summary, lines }) {
    for (const entry of entries) {
        list.insertAdjacentHTML('beforeend', `
            <section class="card rarity" id="${anchor(entry)}">
                <div class="card-title">${[entry.name, ...(entry.aliases ?? [])].join(' / ')}</div>
                <p class="rarity-summary">${entry[summary]}</p>
                ${lines.map(([label, key]) => line(label, entry[key])).join('')}
            </section>`);
    }
}

/** The box asked for: by a rarity link (?rarity=Ultra%20Rare, any spelling or
 * alias: highlighted) or an anchor (#ultra-rare). The boxes exist only now. */
function showAsked(find, anchor) {
    const wanted = new URLSearchParams(location.search).get('rarity');
    const target = wanted ? find(wanted) : null;
    const box = target
        ? document.getElementById(anchor(target.name))
        : document.getElementById(location.hash.slice(1));

    if (box) {
        box.classList.toggle('highlighted', Boolean(target));
        box.scrollIntoView();
    }
}

const GUIDES = {
    yugioh(list) {
        addBoxes(list, RARITIES, {
            anchor: (rarity) => rarityAnchor(rarity.name),
            summary: 'summary',
            lines: [
                ['Name', 'cardName'],
                ['Artwork', 'artwork'],
                ['Elsewhere', 'rest'],
                ['Not to confuse with', 'lookAlikes'],
            ],
        });

        document.querySelector('#rarity-undescribed small').textContent =
            `Not described yet: ${UNDESCRIBED.join(', ')}.`;

        showAsked(findRarity, rarityAnchor);
    },

    pokemon(list) {
        const variants = Object.entries(POKEMON_VARIANTS)
            .map(([code, variant]) => ({ ...variant, code, name: variant.label }));
        addBoxes(document.getElementById('variant-list'), variants, {
            anchor: (variant) => `variant-${variant.code}`,
            summary: 'look',
            lines: [],
        });

        addBoxes(list, POKEMON_RARITIES, {
            anchor: (rarity) => pokemonRarityAnchor(rarity.name),
            summary: 'look',
            lines: [
                ['Symbol', 'symbol'],
                ['Japanese cards', 'japanese'],
                ['Not to confuse with', 'lookAlikes'],
            ],
        });

        list.insertAdjacentHTML('afterend',
            `<p><small>Not described yet: ${POKEMON_UNDESCRIBED.join(', ')}.</small></p>`);

        showAsked(findPokemonRarity, pokemonRarityAnchor);
    },
};

const guideList = document.getElementById('rarity-list');
GUIDES[guideList.dataset.guide](guideList);
