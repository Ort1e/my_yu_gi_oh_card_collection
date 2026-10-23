// The Pokémon card finder (templates/partials/pokemon_finder.html), on the
// copy forms (add, edit) and the wish form:
// - "Find" (or Enter) asks /pokemon/api/search: a printed code (FNE 067)
//   or a name, in the language chosen in the form (Japanese: the Japanese
//   cards; a Japanese search in Latin letters finds the Pokémon's Japanese
//   cards);
// - picking a result fills the `card` field and shows the card: picture,
//   rarity (with the guide's link) and its printed variants to choose
//   from, with today's price and how to recognise each
//   (static/js/pokemon_rarities.js); on the add page, "Full art" is ticked
//   from the rarity;
// - the delete button of the wish form asks first.

const finderForm = document.querySelector('#copy-form, #wish-form');
const queryField = document.getElementById('finder-q');
const searchButton = document.getElementById('finder-search');
const statusLine = document.getElementById('finder-status');
const results = document.getElementById('finder-results');
const cardField = document.getElementById('card');
const languageField = document.getElementById('language');
const catalogueField = document.getElementById('catalogue');
const variantField = document.getElementById('variant');
const fullArtField = document.getElementById('full_art');
const variantChoices = document.getElementById('variant-choices');

const suggestFullArt = finderForm?.hasAttribute('data-suggest-full-art');

// ---------- status line ----------

function say(message, error = false) {
    statusLine.textContent = message;
    statusLine.classList.toggle('is-error', error);
}

/** GET a finder URL; the JSON answer, or throws its error message. */
async function getJson(url) {
    let response;

    try {
        response = await fetch(url, { headers: { Accept: 'application/json' } });
    } catch {
        throw new Error('The app did not answer.');
    }

    if (response.status === 401) throw new Error('Log in first (Account).');

    const body = await response.json().catch(() => ({}));

    if (!response.ok) throw new Error(body.error ?? `Error ${response.status}.`);

    return body;
}

// ---------- the language and catalogue of the search ----------

/** The language to search in: the form's, else the catalogue's. */
function searchLanguage() {
    const language = languageField?.value;

    if (language) return language;

    return catalogueField?.value === 'ja' ? 'ja' : 'fr';
}

function catalogueOf(language) {
    return language === 'ja' ? 'ja' : 'intl';
}

// ---------- search ----------

async function search() {
    const q = queryField.value.trim();

    if (!q) {
        say('Type a name, or the code printed at the bottom of the card (FNE 067).', true);
        return;
    }

    say('Searching…');
    results.replaceChildren();

    let answer;

    try {
        answer = await getJson(`/pokemon/api/search?${new URLSearchParams({ q, lang: searchLanguage() })}`);
    } catch (error) {
        say(error.message, true);
        return;
    }

    if (answer.cards.length === 1) {
        say('');
        await pick(answer.cards[0].id, answer.catalogue, answer.cards[0].secret);
        return;
    }

    say(`${answer.cards.length} cards: pick yours.`);

    for (const card of answer.cards) {
        results.append(resultButton(card, answer.catalogue));
    }
}

function resultButton(card, catalogue) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'finder-result';
    button.title = `${card.name} – ${card.id}`;

    if (card.thumb) {
        const img = document.createElement('img');
        img.src = card.thumb;
        img.alt = '';
        img.loading = 'lazy';
        button.append(img);
    }

    const label = document.createElement('span');
    label.textContent = `${card.name} · ${card.set_id} ${card.local_id}`;
    button.append(label);

    button.addEventListener('click', () => {
        results.replaceChildren();
        say('');
        pick(card.id, catalogue, card.secret);
    });

    return button;
}

// ---------- the card picked ----------

async function pick(id, catalogue, secret = false) {
    cardField.value = id;

    if (catalogueField) catalogueField.value = catalogue;

    await showCard({ choose: true, secret });
}

/**
 * Show the card of the `card` field. `choose`: a card just picked, so set
 * its default variant (and the full art suggestion on the add page).
 */
async function showCard({ choose = false, secret = false } = {}) {
    const id = cardField.value.trim();
    const box = document.getElementById('finder-card');

    if (!id) {
        box.hidden = true;
        variantChoices.hidden = true;
        return;
    }

    const params = new URLSearchParams({ id });

    if (languageField?.value) {
        params.set('lang', languageField.value);
    } else if (catalogueField) {
        params.set('catalogue', catalogueField.value);
    }

    let card;

    try {
        card = await getJson(`/pokemon/api/card?${params}`);
    } catch (error) {
        say(error.message, true);
        box.hidden = true;
        return;
    }

    const image = document.getElementById('finder-card-image');
    image.hidden = !card.image;

    if (card.image) image.src = card.image;

    document.getElementById('finder-card-name').textContent = card.name;
    document.getElementById('finder-card-other').textContent = card.other_name;
    document.getElementById('finder-card-set').textContent = `${card.set_name} · No. ${card.local_id}`;

    const rarity = document.getElementById('finder-card-rarity');
    rarity.replaceChildren();

    if (card.rarity) {
        const link = document.createElement('a');
        link.href = card.rarity_href;
        link.target = '_blank';
        link.className = 'rarity-badge';
        link.textContent = card.rarity;
        rarity.append('Rarity: ', link);

        const guide = findPokemonRarity(card.rarity);

        if (guide) rarity.append(` ${guide.symbol}.`);
    }

    document.getElementById('finder-card-hint').textContent = secret
        ? 'Its number is above the set size: a chase card. Check the symbol next to the number.'
        : '';
    box.hidden = false;

    if (choose && variantField && variantField.value !== '' && card.variants_known) {
        variantField.value = card.default_variant;
    }

    if (choose && suggestFullArt && fullArtField) fullArtField.checked = card.full_art;

    showVariants(card);
}

/** One button per printed variant: its price and how to recognise it. */
function showVariants(card) {
    variantChoices.replaceChildren();

    if (!variantField || !card.variants_known) {
        variantChoices.hidden = true;
        return;
    }

    const intro = document.createElement('p');
    intro.textContent = card.variants.length > 1
        ? 'This card was printed in several variants: which one is yours?'
        : 'This card was printed in one variant:';
    variantChoices.append(intro);

    for (const variant of card.variants) {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'variant-choice';
        button.dataset.variant = variant.code;

        const title = document.createElement('strong');
        title.textContent = variant.label + (variant.price ? ` · ${variant.price} €` : '');
        button.append(title);

        const look = document.createElement('small');
        look.textContent = POKEMON_VARIANTS[variant.code]?.look ?? '';
        button.append(look);

        button.addEventListener('click', () => {
            variantField.value = variant.code;
            markVariant();
        });
        variantChoices.append(button);
    }

    const guide = document.createElement('a');
    guide.href = '/pokemon/rarities#variants';
    guide.target = '_blank';
    guide.textContent = 'The guide';
    variantChoices.append(guide);

    variantChoices.hidden = false;
    markVariant();
}

function markVariant() {
    for (const button of variantChoices.querySelectorAll('.variant-choice')) {
        button.classList.toggle('chosen', button.dataset.variant === variantField.value);
    }
}

// ---------- wiring ----------

if (queryField) {
    searchButton.addEventListener('click', search);

    queryField.addEventListener('keydown', (event) => {
        if (event.key !== 'Enter') return;

        event.preventDefault();
        search();
    });

    variantField?.addEventListener('change', markVariant);

    // Japanese and the other languages are two catalogues: a card picked in
    // one is not in the other.
    let catalogue = catalogueOf(searchLanguage());

    const catalogueChanged = () => {
        const now = catalogueOf(searchLanguage());

        if (now === catalogue) {
            showCard();
            return;
        }

        catalogue = now;

        if (cardField.value) {
            cardField.value = '';
            say('Japanese cards are other cards: find the card again.');
        }

        showCard();
    };

    languageField?.addEventListener('change', catalogueChanged);
    catalogueField?.addEventListener('change', catalogueChanged);
    cardField.addEventListener('change', () => showCard({ choose: true }));

    // a form shown again (errors, edit page, a wish's "I got it")
    if (cardField.value.trim()) showCard();
}

document.addEventListener('submit', (event) => {
    const message = event.target.dataset.confirm;

    if (message && !confirm(message)) event.preventDefault();
});
