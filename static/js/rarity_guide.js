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

// opened from a link like /rarities#ultra-rare: the box exists only now
document.getElementById(location.hash.slice(1))?.scrollIntoView();
