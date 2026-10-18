// Card rows of the lot form (templates/lots/add.html): add / remove rows, and
// the "Quick add from a set" panel (templates/partials/quick_add.html), which fills
// rows from a set code and card numbers using GET /api/sets/:code
// (docs/specs/set-lookup.md).

const rows = document.getElementById('card-rows');
const addButton = document.getElementById('add-row');
const rowTemplate = document.getElementById('card-row-template');

let nextIndex = rows.querySelectorAll('.card-row').length;

// ---------- rows ----------

/** Append an empty card row (before the "+ Add a card" button) and return it. */
function addRow() {
    const html = rowTemplate.innerHTML.replaceAll('__i__', nextIndex++);
    addButton.insertAdjacentHTML('beforebegin', html);

    return addButton.previousElementSibling;
}

/** A field of a row, by the end of its name: `cards-3-code` → field(row, 'code'). */
const field = (row, name) => row.querySelector(`[name$="-${name}"]`);

addButton.addEventListener('click', () => addRow());

rows.addEventListener('click', (e) => {
    if (e.target.classList.contains('remove-row')) e.target.closest('.card-row').remove();
});

// ---------- quick add from a set ----------

const setInput = document.getElementById('qa-set');
const statusSelect = document.getElementById('qa-status');
const numberInput = document.getElementById('qa-number');
const info = document.getElementById('qa-info');
const pasteArea = document.getElementById('qa-paste');
const rarityHelp = document.getElementById('qa-rarities');

/** The loaded set: { prefix: "RA05-FR", name, cards: Map(number → card) }. */
let set = null;

function say(message, isError = false) {
    info.textContent = message;
    info.classList.toggle('error-text', isError);
}

/** "ra05-fr" → { setCode: "RA05", prefix: "RA05-FR" }. */
function parseSetCode(text) {
    const prefix = text.trim().toUpperCase();
    const setCode = prefix.split('-')[0];

    return setCode ? { setCode, prefix: prefix.includes('-') ? prefix : `${prefix}-` } : null;
}

async function loadSet() {
    const parsed = parseSetCode(setInput.value);
    set = null;
    numberInput.disabled = true;

    if (!parsed) return say('Type a set code, e.g. RA05-FR.');

    say(`Loading ${parsed.setCode}…`);

    try {
        const res = await fetch(`/api/sets/${encodeURIComponent(parsed.setCode)}`);
        const body = await res.json().catch(() => ({}));

        if (!res.ok) {
            const reason = body.error ?? `the set lookup answered ${res.status} ${res.statusText}`;

            return say(`Could not load ${parsed.setCode}: ${reason}`, true);
        }

        set = {
            prefix: parsed.prefix,
            name: body.name,
            cards: new Map(body.cards.map((card) => [card.number.toUpperCase(), card])),
        };
    } catch (e) {
        return say(`Could not load ${parsed.setCode}: ${e.message}`, true);
    }

    numberInput.disabled = false;
    numberInput.focus();
    say(`${set.name}: ${set.cards.size} cards. Type a number and press Enter.`);
}

/** The card of the loaded set for "37", "037", "SE1" or a full code "RA05-FR037". */
function findCard(token) {
    let number = token.trim().toUpperCase();

    if (number.includes('-')) {
        const [setCode, rest] = number.split('-');

        // a code from another set
        if (setCode !== set.prefix.split('-')[0]) return null;

        // drop the language letters: "FR037" → "037", "FRSE1" → "SE1"
        number = rest.replace(/^[A-Z]{2}(?=[A-Z0-9])/, '');
    }

    if (set.cards.has(number)) return set.cards.get(number);

    // "37" for "037": compare without the leading zeros
    const bare = number.replace(/^0+/, '');

    for (const [key, card] of set.cards) {
        if (key.replace(/^0+/, '') === bare) return card;
    }

    return null;
}

/** Add one copy of `card`: a new row, or +1 on the row already holding its code. */
function addCard(card) {
    const code = set.prefix + card.number;
    const rowsNow = [...rows.querySelectorAll('.card-row')];
    const existing = rowsNow.find((row) => field(row, 'code').value.toUpperCase() === code);

    if (existing) {
        const quantity = field(existing, 'quantity');
        quantity.value = Number(quantity.value || 1) + 1;

        return existing;
    }

    // reuse the empty row the form starts with, else add one
    const empty = rowsNow.find((row) => !field(row, 'name').value && !field(row, 'code').value);
    const row = empty ?? addRow();

    field(row, 'name').value = card.name_fr ?? card.name_en;
    field(row, 'en_name').value = card.name_en;
    field(row, 'code').value = code;
    field(row, 'status').value = statusSelect.value;
    field(row, 'quantity').value = 1;

    return row;
}

/**
 * Under the message: the rarities the card exists in, when there are several,
 * each with how to recognise it (static/js/rarities.js) and a link to the guide.
 */
function showRarities(card) {
    const rarities = card?.rarities ?? [];

    rarityHelp.hidden = rarities.length < 2;

    if (rarityHelp.hidden) return;

    const items = rarities.map((name) => {
        const known = findRarity(name);
        const hint = known ? ` ${known.summary}` : '';
        const link = known ? ` <a href="/rarities#${rarityAnchor(known.name)}" target="_blank">guide</a>` : '';

        return `<li><strong>${known?.name ?? name}</strong>:${hint}${link}</li>`;
    });

    rarityHelp.innerHTML = `<p>${set.prefix}${card.number} exists in ${rarities.length} rarities:</p><ul>${items.join('')}</ul>`;
}

/** Briefly highlight a row that was just filled. */
function flash(row) {
    row.classList.remove('just-added');
    void row.offsetWidth; // restart the animation
    row.classList.add('just-added');
}

/** Add the cards of a list of numbers/codes; returns the tokens not found. */
function addTokens(tokens) {
    const missing = [];

    for (const token of tokens) {
        const card = findCard(token);

        if (card) {
            flash(addCard(card));
        } else {
            missing.push(token);
        }
    }

    return missing;
}

setInput.addEventListener('change', loadSet);

setInput.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;

    e.preventDefault(); // don't submit the lot
    loadSet();
});

numberInput.addEventListener('keydown', (e) => {
    if (e.key !== 'Enter') return;

    e.preventDefault();

    const token = numberInput.value.trim();

    if (!token || !set) return;

    const card = findCard(token);

    if (!card) {
        showRarities(null);

        return say(`No card ${token} in ${set.name}.`, true);
    }

    flash(addCard(card));
    say(`Added ${set.prefix}${card.number} ${card.name_fr ?? card.name_en}.`);
    showRarities(card);
    numberInput.value = '';
});

document.getElementById('qa-paste-add').addEventListener('click', () => {
    if (!set) return say('Load a set first (set code above).', true);

    const tokens = pasteArea.value.split(/[\s,;]+/).filter(Boolean);

    if (tokens.length === 0) return;

    const missing = addTokens(tokens);
    const added = tokens.length - missing.length;
    showRarities(null); // several cards: no single card's rarities to show

    if (missing.length > 0) {
        say(`Added ${added} cards. Not in ${set.name}: ${missing.join(', ')}.`, true);
        pasteArea.value = missing.join('\n');
    } else {
        say(`Added ${added} cards.`);
        pasteArea.value = '';
    }
});
