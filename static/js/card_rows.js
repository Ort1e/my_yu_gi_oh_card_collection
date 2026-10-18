// Card rows of the lot form (templates/lots/add.html): add / remove rows, and
// the "Quick add from a set" panel (templates/partials/quick_add.html), which fills
// rows from a set code and card numbers using GET /api/sets/:code
// (backend/src/routes/reference/sets.rs).

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

// a code typed by hand no longer limits the rarities to the ones of the code
// the quick add filled in
rows.addEventListener('input', (e) => {
    if (!e.target.name?.endsWith('-code')) return;

    const select = field(e.target.closest('.card-row'), 'rarity');

    if (select.dataset.forCode && select.dataset.forCode !== e.target.value.toUpperCase()) {
        limitRarities(select, null);
    }
});

// ---------- quick add from a set ----------

const setInput = document.getElementById('qa-set');
const statusSelect = document.getElementById('qa-status');
const numberInput = document.getElementById('qa-number');
const info = document.getElementById('qa-info');
const pasteArea = document.getElementById('qa-paste');
const rarityHelp = document.getElementById('qa-rarities');

/** The languages of the card codes (domain::card::Language). */
const LANGUAGES = ['EN', 'FR', 'DE', 'IT', 'SP', 'PT'];

/**
 * The loaded set: { code: "RA05", language: "FR", prefix: "RA05-FR", name,
 * cards: Map(number → card) }.
 */
let set = null;

/**
 * The name of `card` in the language of the loaded set: English for an -EN
 * set, French for a -FR set (English when ygoprodeck has no French name).
 * The set lookup only has French names: for other languages the English name
 * is used, and the server puts it in the right language after saving.
 */
function localName(card) {
    return set.language === 'FR' ? card.name_fr ?? card.name_en : card.name_en;
}

/** "Dragon à Trois Cornes (Tri-Horned Dragon)", or the English name alone. */
function names(card) {
    const local = localName(card);

    return local !== card.name_en ? `${local} (${card.name_en})` : card.name_en;
}

function say(message, isError = false) {
    info.textContent = message;
    info.classList.toggle('error-text', isError);
}

/**
 * "ra05-fr" → { setCode: "RA05", language: "FR", prefix: "RA05-FR" }; an
 * error message when the language is missing or unknown: the card codes need it.
 */
function parseSetCode(text) {
    const [setCode, language, ...rest] = text.trim().toUpperCase().split('-');

    if (!setCode) return { error: 'Type a set code, e.g. RA05-FR.' };

    if (language === undefined || language === '') {
        return { error: `Add the language of the cards: ${setCode}-FR or ${setCode}-EN.` };
    }

    if (!LANGUAGES.includes(language) || rest.length > 0) {
        return { error: `${text.trim()}: the language must be one of ${LANGUAGES.join(', ')} (e.g. ${setCode}-FR).` };
    }

    return { setCode, language, prefix: `${setCode}-${language}` };
}

async function loadSet() {
    const parsed = parseSetCode(setInput.value);
    set = null;
    numberInput.disabled = true;

    if (parsed.error) return say(parsed.error, Boolean(setInput.value.trim()));

    say(`Loading ${parsed.setCode}…`);

    try {
        const res = await fetch(`/api/sets/${encodeURIComponent(parsed.setCode)}`);
        const body = await res.json().catch(() => ({}));

        if (!res.ok) {
            const reason = body.error ?? `the set lookup answered ${res.status} ${res.statusText}`;

            return say(`Could not load ${parsed.setCode}: ${reason}`, true);
        }

        set = {
            code: parsed.setCode,
            language: parsed.language,
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
    suggestSetCodes();
}

/**
 * Why a full code can't be used with the loaded set, or null: it must be a
 * code of this set, in its language ("RA05-FR037" when RA05-FR is loaded).
 */
function codeProblem(token) {
    const code = token.trim().toUpperCase();

    if (!code.includes('-')) return null;

    if (code.startsWith(`${set.prefix}`) && code.length > set.prefix.length) return null;

    const [setCode, rest = ''] = code.split('-');

    if (setCode !== set.code) return `${token} is not a card of ${set.prefix}.`;

    const language = LANGUAGES.find((l) => rest.startsWith(l) && rest.length > l.length);

    if (language) return `${token} is in ${language}, the loaded set is ${set.prefix}.`;

    return `${token}: write the code with its language, ${set.prefix}… (the loaded set).`;
}

/** The card of the loaded set for "37", "037", "SE1" or a full code "RA05-FR037". */
function findCard(token) {
    let number = token.trim().toUpperCase();

    if (number.includes('-')) {
        if (codeProblem(number)) return null;

        // "RA05-FR037" → "037", "RA05-FRSE1" → "SE1"
        number = number.slice(set.prefix.length);
    }

    if (set.cards.has(number)) return set.cards.get(number);

    // "37" for "037": compare without the leading zeros
    const bare = number.replace(/^0+/, '');

    for (const [key, card] of set.cards) {
        if (key.replace(/^0+/, '') === bare) return card;
    }

    return null;
}

/**
 * Add one copy of `card`. A code printed in a single rarity: +1 on the row
 * already holding it, if any. Several rarities: always a new row, since each
 * copy may be a different rarity (to choose in its row).
 */
function addCard(card) {
    const code = set.prefix + card.number;
    const rowsNow = [...rows.querySelectorAll('.card-row')];
    const oneRarity = (card.rarities?.length ?? 0) <= 1;
    const existing = oneRarity
        ? rowsNow.find((row) => field(row, 'code').value.toUpperCase() === code)
        : null;

    if (existing) {
        const quantity = field(existing, 'quantity');
        quantity.value = Number(quantity.value || 1) + 1;

        return existing;
    }

    // reuse the empty row the form starts with, else add one
    const empty = rowsNow.find((row) => !field(row, 'name').value && !field(row, 'code').value);
    const row = empty ?? addRow();

    field(row, 'name').value = localName(card);
    field(row, 'en_name').value = card.name_en;
    field(row, 'code').value = code;
    field(row, 'status').value = statusSelect.value;
    field(row, 'quantity').value = 1;

    // only the rarities the code was printed in; a single one is selected
    const rarity = field(row, 'rarity');
    limitRarities(rarity, card.rarities ?? null, code);

    return row;
}

/**
 * Show in the rarity `select` only the rarities of `names` (ygoprodeck names,
 * any spelling), for the card `code`; `names` null shows them all again. One
 * rarity: it is selected. A name matching no option is ignored; if none
 * matches, every option stays.
 */
function limitRarities(select, names, code = '') {
    const key = (text) => text.toLowerCase().replace(/[^a-z0-9]/g, '');
    const wanted = new Set((names ?? []).map(key));
    const options = [...select.options].filter((o) => o.value);
    const matching = options.filter((o) => wanted.has(key(o.value)));
    const limit = matching.length > 0;

    for (const option of options) {
        const shown = !limit || matching.includes(option);
        option.hidden = !shown;
        option.disabled = !shown;
    }

    if (limit) {
        select.dataset.forCode = code;
    } else {
        delete select.dataset.forCode;
    }

    if (select.selectedOptions[0]?.disabled) select.value = '';

    if (matching.length === 1) select.value = matching[0].value;
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

    rarityHelp.innerHTML = `<p>${set.prefix}${card.number} ${names(card)} exists in ${rarities.length} rarities:</p><ul>${items.join('')}</ul>`;
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

// ---------- set code suggestions ----------

const setSuggestions = document.getElementById('qa-set-suggestions');

/**
 * The set codes of the cards already in the lot ("MAMO-FR"), the most
 * frequent first: the card rows, and on a lot page its card list
 * (`data-code` on the rows of templates/lots/detail.html).
 */
function lotSetCodes() {
    const codes = [
        ...[...rows.querySelectorAll('.card-row')].map((row) => field(row, 'code').value),
        ...[...document.querySelectorAll('[data-code]')].map((el) => el.dataset.code),
    ];
    const counts = new Map();

    for (const code of codes) {
        const match = code.trim().toUpperCase().match(/^([A-Z0-9]+-[A-Z]{2})[A-Z0-9]/);

        if (match && LANGUAGES.includes(match[1].split('-')[1])) {
            counts.set(match[1], (counts.get(match[1]) ?? 0) + 1);
        }
    }

    return [...counts].sort((a, b) => b[1] - a[1]).map(([code]) => code);
}

const setChoices = document.getElementById('qa-set-choices');
const MAX_SET_BUTTONS = 8;

/**
 * Offer the lot's set codes: one button each under the set code box (a click
 * loads it, the loaded one is marked), and in the box's suggestions. Any
 * other code can still be typed in the box.
 */
function suggestSetCodes() {
    const codes = lotSetCodes();
    setSuggestions.replaceChildren(...codes.map((code) => new Option(code)));
    setInput.placeholder = codes.length > 0 ? `e.g. ${codes[0]}` : 'e.g. RA05-FR';

    setChoices.hidden = codes.length === 0;

    if (setChoices.hidden) return;

    // the most frequent ones; the others stay in the box's suggestions
    const buttons = codes.slice(0, MAX_SET_BUTTONS).map((code) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'btn-small btn-secondary';
        button.textContent = code;
        button.classList.toggle('active', set?.prefix === code);
        button.addEventListener('click', () => {
            setInput.value = code;
            loadSet();
        });

        return button;
    });

    const more = codes.length - buttons.length;
    const hint = more > 0 ? ` +${more} more in the set code suggestions` : '';

    setChoices.replaceChildren('Sets in this lot: ', ...buttons, hint);
}

suggestSetCodes();
setInput.addEventListener('focus', suggestSetCodes);

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

        return say(codeProblem(token) ?? `No card ${token} in ${set.name}.`, true);
    }

    flash(addCard(card));
    say(`Added ${set.prefix}${card.number} ${names(card)}.`);
    suggestSetCodes();
    showRarities(card);
    numberInput.value = '';
});

document.getElementById('qa-paste-add').addEventListener('click', () => {
    if (!set) return say('Load a set first (set code above).', true);

    const tokens = pasteArea.value.split(/[\s,;]+/).filter(Boolean);

    if (tokens.length === 0) return;

    const missing = addTokens(tokens);
    const added = tokens.length - missing.length;
    suggestSetCodes();
    showRarities(null); // several cards: no single card's rarities to show

    if (missing.length > 0) {
        const problems = missing.map(codeProblem).filter(Boolean);
        const why = problems.length > 0 ? ` ${problems.join(' ')}` : '';
        say(`Added ${added} cards. Not added: ${missing.join(', ')}.${why}`, true);
        pasteArea.value = missing.join('\n');
    } else {
        say(`Added ${added} cards.`);
        pasteArea.value = '';
    }
});
