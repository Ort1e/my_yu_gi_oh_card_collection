// What to read? (templates/books/random.html): a book picked at random among
// the ones the filters keep, from the books the page carries (#books-data),
// so the public copy (static files) can pick too. "Another one" doesn't show
// the same book twice in a row unless it's the only one left.
//
// "Can start it now": no unread earlier volume of its series in the
// collection (the volumes not owned don't count).

const books = JSON.parse(/** @type {string} */ (/** @type {HTMLElement} */ (document.getElementById('books-data')).textContent));

const card = /** @type {HTMLElement} */ (document.getElementById('book-pick'));
const none = /** @type {HTMLElement} */ (document.getElementById('book-pick-none'));
/** @type {(name: string) => any} a part of the card: text, link or switch */
const field = (name) => card.querySelector(`[data-pick="${name}"]`);

const filters = {
    unread: /** @type {HTMLInputElement} */ (document.getElementById('pick-unread')),
    next: /** @type {HTMLInputElement} */ (document.getElementById('pick-next')),
    language: /** @type {HTMLSelectElement} */ (document.getElementById('pick-language')),
    format: /** @type {HTMLSelectElement} */ (document.getElementById('pick-format')),
};

let shown = null;

function canStart(book) {
    if (book.series_id === null || book.volume === null) return true;

    return !books.some((other) =>
        other.series_id === book.series_id
        && other.volume !== null
        && other.volume < book.volume
        && !other.read);
}

function candidates() {
    return books.filter((book) =>
        (!filters.unread.checked || !book.read)
        && (!filters.next.checked || canStart(book))
        && (!filters.language.value || book.language === filters.language.value)
        && (!filters.format.value || book.format === filters.format.value));
}

/** A fact of the list (pages, format): hidden when the book has none. */
function fact(name, value) {
    field(name).textContent = value ?? '';
    field(name).hidden = value === null;
    field(`${name}-label`).hidden = value === null;
}

function showCover(book) {
    card.querySelector('.book-cover-image')?.remove();

    if (!book.cover) return;

    // Open Library answers 404 for a book it has no cover of: the plain
    // "book" under it shows then
    const img = document.createElement('img');
    img.className = 'book-cover-image';
    img.alt = `Cover of ${book.title}`;
    img.referrerPolicy = 'no-referrer';
    img.addEventListener('error', () => img.remove());
    img.src = book.cover;
    /** @type {Element} */ (card.querySelector('.book-cover')).append(img);
}

function show(book, among) {
    shown = book;
    card.hidden = false;
    none.hidden = true;

    field('title').textContent = book.title;
    field('authors').textContent = book.authors;
    field('link').textContent = book.title;
    field('link').href = book.href;
    field('by').textContent = book.authors ? `by ${book.authors}` : '';
    field('series').textContent = book.series;
    fact('pages', book.pages);
    fact('format', book.format);
    field('among').textContent = `Picked among ${among} book${among === 1 ? '' : 's'}.`;

    const read = field('read');
    if (read) read.textContent = book.read ? 'Yes' : 'No';

    const toggle = field('switch');
    if (toggle) {
        toggle.dataset.bookRead = book.id;
        toggle.checked = book.read;
    }

    showCover(book);
}

function pick() {
    const kept = candidates();

    if (kept.length === 0) {
        shown = null;
        card.hidden = true;
        none.hidden = false;
        return;
    }

    const others = kept.length > 1 ? kept.filter((book) => book !== shown) : kept;
    show(others[Math.floor(Math.random() * others.length)], kept.length);
}

/** @type {HTMLElement} */ (document.getElementById('pick-again')).addEventListener('click', pick);
Object.values(filters).forEach((input) => input.addEventListener('change', pick));

// the read switch (saved by book_read.js): the next picks know it
field('switch')?.addEventListener('change', (event) => {
    if (shown) shown.read = /** @type {HTMLInputElement} */ (event.target).checked;
});

pick();
