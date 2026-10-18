// The /rarities page (templates/reference/rarities.html): one box per rarity, from
// static/js/rarities.js.

const list = document.getElementById('rarity-list');

const line = (label, text) => (text ? `<div><strong>${label}:</strong> ${text}</div>` : '');

for (const rarity of RARITIES) {
    list.insertAdjacentHTML('beforeend', `
        <section class="card rarity" id="${rarityAnchor(rarity.name)}">
            <div class="card-title">${[rarity.name, ...(rarity.aliases ?? [])].join(' / ')}</div>
            <p class="rarity-summary">${rarity.summary}</p>
            ${line('Name', rarity.cardName)}
            ${line('Artwork', rarity.artwork)}
            ${line('Elsewhere', rarity.rest)}
            ${line('Not to confuse with', rarity.lookAlikes)}
        </section>`);
}

document.querySelector('#rarity-undescribed small').textContent =
    `Not described yet: ${UNDESCRIBED.join(', ')}.`;

// Opened from a link like /rarities#ultra-rare, or from a rarity badge,
// /rarities?rarity=Super%20Short%20Print (any spelling or alias): the box
// exists only now.
const wanted = new URLSearchParams(location.search).get('rarity');
const target = wanted ? findRarity(wanted) : null;
const box = target
    ? document.getElementById(rarityAnchor(target.name))
    : document.getElementById(location.hash.slice(1));

if (box) {
    box.classList.toggle('highlighted', Boolean(target));
    box.scrollIntoView();
}
