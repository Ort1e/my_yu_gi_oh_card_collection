// The book add and edit forms (templates/books/add.html, edit.html,
// fields.html):
// - "Look up" (or Enter in the ISBN field, which barcode scanners type
//   after the digits) asks /books/api/isbn/:isbn and fills the fields;
// - "Scan" takes a photo of the barcode, "From a photo" picks one already
//   taken, and the NAS reads the ISBN on it (POST /books/api/isbn-scan);
// - the delete button asks first.
//
// The photo is sent as it is: read in the phone's browser, a whole camera
// photo took more memory than some phones have ("out of memory"). The NAS
// keeps nothing of it. "From a photo" also spares the phone the camera app
// opened over the browser (on a phone short of memory, Android may close
// the browser meanwhile).

const form = document.getElementById('book-form');
const isbnField = document.getElementById('isbn');
const lookupButton = document.getElementById('isbn-lookup');
// "Scan" (the camera) and "From a photo" (one already taken)
const photoInputs = document.querySelectorAll('#isbn-photo, #isbn-photo-file');
const status = document.getElementById('isbn-status');

// On the edit page, a lookup only fills the empty fields.
const keepFilled = form?.hasAttribute('data-keep-filled');

// ---------- status line ----------

/** Show a message under the ISBN field; `link` adds a link after it. */
function say(message, { error = false, link = null } = {}) {
    status.textContent = message;
    status.classList.toggle('is-error', error);

    if (link) {
        const a = document.createElement('a');
        a.href = link.href;
        a.textContent = link.text;
        status.append(' ', a);
    }
}

// ---------- lookup ----------

/** The form fields a lookup answer fills: field id → answer value. */
function fieldValues(book) {
    return {
        isbn: book.isbn,
        title: book.title,
        authors: (book.authors ?? []).join(', '),
        publisher: book.publisher,
        year: book.year,
        language: book.language,
        pages: book.pages,
        series: book.series,
        volume: book.volume,
        format: book.format,
    };
}

function fill(book) {
    let filledMore = false;

    for (const [id, value] of Object.entries(fieldValues(book))) {
        const field = document.getElementById(id);

        if (!field || value === null || value === undefined || value === '') continue;

        if (keepFilled && field.value.trim() !== '' && id !== 'isbn') continue;

        field.value = value;
        field.classList.remove('just-filled');
        void field.offsetWidth; // restart the flash
        field.classList.add('just-filled');

        if (['publisher', 'year', 'language', 'pages', 'format'].includes(id)) filledMore = true;
    }

    if (filledMore) document.querySelector('.more-fields')?.setAttribute('open', '');
}

async function lookup() {
    const isbn = isbnField.value.trim();

    if (!isbn) {
        say('Type or scan an ISBN first.', { error: true });
        isbnField.focus();
        return;
    }

    say('Looking it up…');
    lookupButton.disabled = true;

    let response;
    let answer;

    try {
        response = await fetch(`/books/api/isbn/${encodeURIComponent(isbn)}`, {
            headers: { Accept: 'application/json' },
        });
        answer = await response.json();
    } catch {
        say('The lookup failed (no connection?). Try again, or fill the fields by hand.', { error: true });
        return;
    } finally {
        lookupButton.disabled = false;
    }

    if (response.status === 401) {
        const next = encodeURIComponent(location.pathname + location.search);
        say('Log in first.', { error: true, link: { href: `/login?next=${next}`, text: 'Log in' } });
        return;
    }

    const owned = answer.owned_book_id && !keepFilled
        ? { href: `/books/${answer.owned_book_id}`, text: 'Open it' }
        : null;

    if (!response.ok) {
        if (answer.isbn) isbnField.value = answer.isbn;

        if (owned) {
            say('This book is already in the list.', { error: true, link: owned });
        } else {
            say(answer.error ?? 'The lookup failed.', { error: true });
        }
        return;
    }

    fill(answer);

    if (owned) {
        say(`Already in the list: ${answer.title}.`, { error: true, link: owned });
        return;
    }

    say(`Found: ${answer.title ?? 'a book'}. Check the fields, then save.`);
}

if (form) {
    lookupButton.addEventListener('click', lookup);

    // Enter in the ISBN field (typed, or sent by a barcode scanner) looks up
    // instead of saving the form
    isbnField.addEventListener('keydown', (e) => {
        if (e.key !== 'Enter') return;

        e.preventDefault();
        lookup();
    });
}

// ---------- barcode scan ----------

/** The ISBN on the photo, read on the NAS: { isbn } or { error }. */
async function scanOnServer(file) {
    const body = new FormData();
    body.append('photo', file);

    const response = await fetch('/books/api/isbn-scan', {
        method: 'POST',
        headers: { Accept: 'application/json' },
        body,
    });

    if (response.status === 401) return { login: true };

    return response.json();
}

async function scan(input) {
    const file = input.files[0];

    if (!file) return;

    say('Reading the barcode (a few seconds)…');

    let answer;

    try {
        answer = await scanOnServer(file);
    } catch {
        say('The photo could not be sent (no connection?). Try again, or type the 13 digits.', { error: true });
        return;
    } finally {
        input.value = ''; // the same photo can be chosen again
    }

    if (answer.login) {
        const next = encodeURIComponent(location.pathname + location.search);
        say('Log in first.', { error: true, link: { href: `/login?next=${next}`, text: 'Log in' } });
        return;
    }

    if (!answer.isbn) {
        say(answer.error ?? 'The scan failed: type the 13 digits instead.', { error: true });
        return;
    }

    isbnField.value = answer.isbn;
    lookup();
}

for (const input of photoInputs) {
    input.addEventListener('change', () => scan(input));
}

// ---------- delete ----------

document.addEventListener('submit', (e) => {
    const message = e.target.dataset.confirm;

    if (message && !confirm(message)) e.preventDefault();
});
