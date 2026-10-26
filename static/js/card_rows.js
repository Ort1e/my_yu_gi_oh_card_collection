// Card rows of the lot form (templates/lots/add.html): add / remove rows, and
// the "Quick add from a set" panel (templates/partials/quick_add.html), which fills
// rows from card numbers of a default set, or full codes of any set, using
// GET /api/sets/:code (backend/src/routes/reference/sets.rs).

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
const pasteButton = document.getElementById('qa-paste-add');
const rarityHelp = document.getElementById('qa-rarities');

/** The languages of the card codes (domain::card::Language). */
const LANGUAGES = ['EN', 'FR', 'DE', 'IT', 'SP', 'PT'];

/** The sets looked up so far, by set code: "RA05" → { code, name, cards: Map(number → card) }. */
const sets = new Map();

/**
 * The default set, for the numbers typed alone ("37"): { set, language: "FR",
 * prefix: "RA05-FR" }, null until one is loaded. A full code ("MAMO-EN012")
 * uses its own set and language instead.
 */
let current = null;

/**
 * The name of `card` in `language`: English for -EN, French for -FR (English
 * when ygoprodeck has no French name). The set lookup only has French names:
 * for other languages the English name is used, and the server puts it in the
 * right language after saving.
 */
function localName(card, language) {
    return language === 'FR' ? card.name_fr ?? card.name_en : card.name_en;
}

/** "Dragon à Trois Cornes (Tri-Horned Dragon)", or the English name alone. */
/** Text from outside (ygoprodeck's names) made safe to put in HTML. */
function escapeHtml(text) {
    return String(text)
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;');
}

function names(card, language) {
    const local = localName(card, language);

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

/**
 * "ra05-fr037" → { setCode: "RA05", language: "FR", number: "037" }; an error
 * message when the language is missing or unknown.
 */
function parseCardCode(token) {
    const code = token.trim().toUpperCase();
    const [setCode, rest = '', ...more] = code.split('-');
    const language = LANGUAGES.find((l) => rest.startsWith(l) && rest.length > l.length);

    if (!setCode || !language || more.length > 0) {
        return { error: `${code}: write the code with its language, e.g. ${setCode || 'RA05'}-FR037.` };
    }

    return { setCode, language, number: rest.slice(language.length) };
}

/** The set `setCode`, looked up once; throws an Error with the reason. */
async function fetchSet(setCode) {
    if (sets.has(setCode)) return sets.get(setCode);

    const res = await fetch(`/api/sets/${encodeURIComponent(setCode)}`);
    const body = await res.json().catch(() => ({}));

    if (!res.ok) {
        throw new Error(body.error ?? `the set lookup answered ${res.status} ${res.statusText}`);
    }

    const set = {
        code: setCode,
        name: body.name,
        cards: new Map(body.cards.map((card) => [card.number.toUpperCase(), card])),
    };
    sets.set(setCode, set);

    return set;
}

async function loadSet() {
    const parsed = parseSetCode(setInput.value);
    current = null;

    if (parsed.error) return say(parsed.error, Boolean(setInput.value.trim()));

    say(`Loading ${parsed.setCode}…`);

    try {
        const set = await fetchSet(parsed.setCode);
        current = { set, language: parsed.language, prefix: parsed.prefix };
    } catch (e) {
        return say(`Could not load ${parsed.setCode}: ${e.message}`, true);
    }

    numberInput.focus();
    say(`${current.set.name}: ${current.set.cards.size} cards. Type a number (or a full code of any set) and press Enter.`);
    suggestSetCodes();
}

/** The card of `set` numbered "37", "037" or "SE1", or null. */
function findInSet(set, number) {
    number = number.trim().toUpperCase();

    if (set.cards.has(number)) return set.cards.get(number);

    // "37" for "037": compare without the leading zeros
    const bare = number.replace(/^0+/, '');

    for (const [key, card] of set.cards) {
        if (key.replace(/^0+/, '') === bare) return card;
    }

    return null;
}

/**
 * The card for a number of the default set ("37") or a full code of any set
 * ("RA05-FR037", "MAMO-EN012", looked up if needed):
 * { card, set, language, prefix }, or { error }.
 */
async function resolve(token) {
    if (!token.includes('-')) {
        if (!current) return { error: `${token}: load a set first, or type the full code (e.g. RA05-FR037).` };

        const card = findInSet(current.set, token);

        return card ? { ...current, card } : { error: `No card ${token} in ${current.set.name}.` };
    }

    const parsed = parseCardCode(token);

    if (parsed.error) return parsed;

    let set;

    try {
        set = await fetchSet(parsed.setCode);
    } catch (e) {
        return { error: `Could not load ${parsed.setCode}: ${e.message}` };
    }

    const card = findInSet(set, parsed.number);

    if (!card) return { error: `No card ${token.trim().toUpperCase()} in ${set.name}.` };

    return { set, language: parsed.language, prefix: `${parsed.setCode}-${parsed.language}`, card };
}

/**
 * Add one copy of the card of `pick` (from resolve()). A code printed in a
 * single rarity: +1 on the row already holding it, if any. Several rarities:
 * always a new row, since each copy may be a different rarity (to choose in its row).
 */
function addCard({ card, language, prefix }) {
    const code = prefix + card.number;
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

    field(row, 'name').value = localName(card, language);
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
 * Under the message: the rarities the card of `pick` exists in, when there are
 * several, each with how to recognise it (static/js/rarities.js) and a link to
 * the guide. `pick` null hides them.
 */
function showRarities(pick) {
    const rarities = pick?.card.rarities ?? [];

    rarityHelp.hidden = rarities.length < 2;

    if (rarityHelp.hidden) return;

    const items = rarities.map((name) => {
        const known = findRarity(name);
        const hint = known ? ` ${known.summary}` : '';
        const link = known ? ` <a href="/rarities#${rarityAnchor(known.name)}" target="_blank">guide</a>` : '';

        return `<li><strong>${escapeHtml(known?.name ?? name)}</strong>:${hint}${link}</li>`;
    });

    const { card, language, prefix } = pick;
    const title = `${prefix}${card.number} ${names(card, language)}`;
    rarityHelp.innerHTML = `<p>${escapeHtml(title)} exists in ${rarities.length} rarities:</p><ul>${items.join('')}</ul>`;
}

/** Briefly highlight a row that was just filled. */
function flash(row) {
    row.classList.remove('just-added');
    void row.offsetWidth; // restart the animation
    row.classList.add('just-added');
}

/** Add the cards of a list of numbers/codes; returns { added, missing, errors }. */
async function addTokens(tokens) {
    let added = 0;
    const missing = [];
    const errors = [];

    for (const token of tokens) {
        const pick = await resolve(token);

        if (pick.error) {
            missing.push(token);
            errors.push(pick.error);
        } else {
            flash(addCard(pick));
            added++;
        }
    }

    return { added, missing, errors };
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
        button.classList.toggle('active', current?.prefix === code);
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

numberInput.addEventListener('keydown', async (e) => {
    if (e.key !== 'Enter') return;

    e.preventDefault();

    const token = numberInput.value.trim();

    if (!token) return;

    const pick = await resolve(token);

    if (pick.error) {
        showRarities(null);

        return say(pick.error, true);
    }

    flash(addCard(pick));
    say(`Added ${pick.prefix}${pick.card.number} ${names(pick.card, pick.language)}.`);
    suggestSetCodes();
    showRarities(pick);
    numberInput.value = '';
});

pasteButton.addEventListener('click', async () => {
    const tokens = pasteArea.value.split(/[\s,;]+/).filter(Boolean);

    if (tokens.length === 0) return;

    pasteButton.disabled = true;

    const { added, missing, errors } = await addTokens(tokens);

    pasteButton.disabled = false;
    suggestSetCodes();
    showRarities(null); // several cards: no single card's rarities to show

    if (missing.length > 0) {
        say(`Added ${added} cards. Not added: ${errors.join(' ')}`, true);
        pasteArea.value = missing.join('\n');
    } else {
        say(`Added ${added} cards.`);
        pasteArea.value = '';
    }
});
