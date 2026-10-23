// The /pokemon/rarities page (templates/pokemon/rarities.html): one box per
// variant and per rarity, from static/js/pokemon_rarities.js.

const variantList = document.getElementById('variant-list');
const rarityList = document.getElementById('rarity-list');

const line = (label, text) => (text ? `<div><strong>${label}:</strong> ${text}</div>` : '');

for (const [code, variant] of Object.entries(POKEMON_VARIANTS)) {
    variantList.insertAdjacentHTML('beforeend', `
        <section class="card rarity" id="variant-${code}">
            <div class="card-title">${variant.label}</div>
            <p class="rarity-summary">${variant.look}</p>
        </section>`);
}

for (const rarity of POKEMON_RARITIES) {
    rarityList.insertAdjacentHTML('beforeend', `
        <section class="card rarity" id="${pokemonRarityAnchor(rarity.name)}">
            <div class="card-title">${[rarity.name, ...(rarity.aliases ?? [])].join(' / ')}</div>
            <p class="rarity-summary">${rarity.look}</p>
            ${line('Symbol', rarity.symbol)}
            ${line('Japanese cards', rarity.japanese)}
            ${line('Not to confuse with', rarity.lookAlikes)}
        </section>`);
}

rarityList.insertAdjacentHTML('afterend', `<p><small>Not described yet: ${POKEMON_UNDESCRIBED.join(', ')}.</small></p>`);

// Opened from a rarity link, /pokemon/rarities?rarity=Ultra%20Rare (any
// spelling or alias), or an anchor (#variants): the boxes exist only now.
const wanted = new URLSearchParams(location.search).get('rarity');
const target = wanted ? findPokemonRarity(wanted) : null;
const box = target
    ? document.getElementById(pokemonRarityAnchor(target.name))
    : document.getElementById(location.hash.slice(1));

if (box) {
    box.classList.toggle('highlighted', Boolean(target));
    box.scrollIntoView();
}
