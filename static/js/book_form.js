// The book add and edit forms (templates/books/add.html, edit.html,
// fields.html):
// - "Look up" (or Enter in the ISBN field, which barcode scanners type
//   after the digits) asks /books/api/isbn/:isbn and fills the fields;
// - "Scan" takes a photo of the barcode and reads the ISBN from it;
// - the delete button asks first.
//
// The scan reads the photo in the browser: the phone's BarcodeDetector when
// it has one (only on HTTPS sites), else the ZXing library, loaded from the
// CDN the first time. The photo is decoded once, already reduced (phones
// run out of memory on a whole one), and never sent anywhere.

const form = document.getElementById('book-form');
const isbnField = document.getElementById('isbn');
const lookupButton = document.getElementById('isbn-lookup');
const photoInput = document.getElementById('isbn-photo');
const status = document.getElementById('isbn-status');

// On the edit page, a lookup only fills the empty fields.
const keepFilled = form?.hasAttribute('data-keep-filled');

const ZXING_URL = 'https://cdn.jsdelivr.net/npm/@zxing/library@0.21.3/umd/index.min.js';

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

/** The photo's size, read by the browser without decoding its pixels. */
function photoSize(url) {
    return new Promise((resolve, reject) => {
        const img = new Image();
        img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight });
        img.onerror = () => reject(new Error('the photo could not be read'));
        img.src = url;
    });
}

/** The photo on a canvas at most `maxSide` px wide or high, decoded at that
    size: a phone photo decoded whole takes 50 to 200 MB, too much for some
    phones ("out of memory"). */
async function photoCanvas(file, maxSide) {
    const url = URL.createObjectURL(file);

    try {
        const { width, height } = await photoSize(url);
        const scale = Math.min(1, maxSide / Math.max(width, height));
        const canvas = document.createElement('canvas');
        canvas.width = Math.max(1, Math.round(width * scale));
        canvas.height = Math.max(1, Math.round(height * scale));

        const bitmap = await createImageBitmap(file, {
            resizeWidth: canvas.width,
            resizeHeight: canvas.height,
            resizeQuality: 'medium',
        });
        canvas.getContext('2d').drawImage(bitmap, 0, 0, canvas.width, canvas.height);
        bitmap.close();

        return canvas;
    } finally {
        URL.revokeObjectURL(url);
    }
}

/** A smaller copy of a canvas, at most `maxSide` px wide or high. */
function shrunk(canvas, maxSide) {
    const scale = Math.min(1, maxSide / Math.max(canvas.width, canvas.height));
    const out = document.createElement('canvas');
    out.width = Math.max(1, Math.round(canvas.width * scale));
    out.height = Math.max(1, Math.round(canvas.height * scale));
    out.getContext('2d').drawImage(canvas, 0, 0, out.width, out.height);

    return out;
}

/** A book's ISBN (978/979…) among the barcodes read, or null. */
function bookIsbn(values) {
    return values.find((v) => /^97[89]\d{10}$/.test(v)) ?? null;
}

async function readWithBarcodeDetector(canvas) {
    if (!('BarcodeDetector' in window)) return undefined;

    const formats = await BarcodeDetector.getSupportedFormats();

    if (!formats.includes('ean_13')) return undefined;

    const found = await new BarcodeDetector({ formats: ['ean_13'] }).detect(canvas);
    return bookIsbn(found.map((b) => b.rawValue));
}

let zxingLoading = null;

function loadZxing() {
    zxingLoading ??= new Promise((resolve, reject) => {
        const script = document.createElement('script');
        script.src = ZXING_URL;
        script.onload = () => resolve(window.ZXing);
        script.onerror = () => {
            zxingLoading = null;
            reject(new Error('the barcode reader could not be loaded'));
        };
        document.head.append(script);
    });

    return zxingLoading;
}

async function readWithZxing(canvas) {
    const ZXing = await loadZxing();
    const hints = new Map([
        [ZXing.DecodeHintType.POSSIBLE_FORMATS, [ZXing.BarcodeFormat.EAN_13]],
        [ZXing.DecodeHintType.TRY_HARDER, true],
    ]);
    const reader = new ZXing.MultiFormatReader();
    reader.setHints(hints);

    const source = new ZXing.HTMLCanvasElementLuminanceSource(canvas);

    try {
        const result = reader.decode(new ZXing.BinaryBitmap(new ZXing.HybridBinarizer(source)));
        return bookIsbn([result.getText()]);
    } catch {
        return null; // no barcode found
    }
}

/** The canvas turned a quarter turn (a barcode photographed sideways). */
function turned(canvas) {
    const out = document.createElement('canvas');
    out.width = canvas.height;
    out.height = canvas.width;

    const g = out.getContext('2d');
    g.translate(out.width, 0);
    g.rotate(Math.PI / 2);
    g.drawImage(canvas, 0, 0);
    return out;
}

/** The ISBN in a photo of a barcode, or null. Tries a big then a small copy
    (a far barcode needs pixels, a blurry one is sharper smaller), each as
    taken and turned. The photo is decoded once, already reduced. */
async function readIsbn(file) {
    const big = await photoCanvas(file, 1600);

    for (const canvas of [big, shrunk(big, 800)]) {
        for (const picture of [canvas, turned(canvas)]) {
            const detected = await readWithBarcodeDetector(picture).catch(() => undefined);

            if (detected) return detected;

            const read = await readWithZxing(picture);

            if (read) return read;
        }
    }

    return null;
}

if (photoInput) {
    photoInput.addEventListener('change', async () => {
        const file = photoInput.files[0];

        if (!file) return;

        say('Reading the barcode…');

        let isbn;

        try {
            isbn = await readIsbn(file);
        } catch (e) {
            say(`The scan failed (${e.message}). Type the 13 digits instead.`, { error: true });
            return;
        } finally {
            photoInput.value = ''; // the same photo can be chosen again
        }

        if (!isbn) {
            say('No ISBN barcode found in the photo: try closer, flat and well lit, or type the 13 digits.', { error: true });
            return;
        }

        isbnField.value = isbn;
        lookup();
    });
}

// ---------- delete ----------

document.addEventListener('submit', (e) => {
    const message = e.target.dataset.confirm;

    if (message && !confirm(message)) e.preventDefault();
});
