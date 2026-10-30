// The book add and edit forms (templates/books/add.html, edit.html,
// fields.html):
// - "Look up" (or Enter in the ISBN field, which barcode scanners type
//   after the digits) asks /books/api/isbn/:isbn and fills the fields;
// - "Scan": the camera live in the page, the barcode read as soon as it is
//   in view (the browser's BarcodeDetector, else a frame now and then sent
//   to the NAS). Browsers give a page the camera on HTTPS only: on the home
//   network's http://, Chrome needs its setting "Insecure origins treated
//   as secure" (docs/recipes.md, "Use the app from a phone"). Without it,
//   "Scan" opens the camera app instead, and the NAS reads the photo;
// - "From a photo" picks one already taken, read by the NAS too
//   (POST /books/api/isbn-scan).
//
// The camera app over the browser was the trouble: on a phone short of
// memory, Android closed Chrome meanwhile ("not enough memory", the photo
// lost). The live scanner never leaves the page, and keeps no photo.

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

// ---------- live scanner ----------

const liveButton = document.getElementById('isbn-live');
const photoLabel = document.getElementById('isbn-photo-label');
const dialog = document.getElementById('scan-dialog');
const video = document.getElementById('scan-video');
const scanHelp = document.getElementById('scan-help');

/** The page may use the camera (HTTPS, or Chrome's setting for this address). */
const canStream = window.isSecureContext && !!navigator.mediaDevices?.getUserMedia;

let stream = null;
let stopped = true;

const isBookIsbn = (text) => /^97[89]\d{10}$/.test(text);

/** The browser's own barcode reader, when it reads EAN-13 (Chrome on Android). */
async function nativeDetector() {
    if (!('BarcodeDetector' in window)) return null;

    try {
        const formats = await BarcodeDetector.getSupportedFormats();
        return formats.includes('ean_13') ? new BarcodeDetector({ formats: ['ean_13'] }) : null;
    } catch {
        return null;
    }
}

function stopScanner() {
    stopped = true;
    stream?.getTracks().forEach((track) => track.stop());
    stream = null;
    video.srcObject = null;

    if (dialog.open) dialog.close();
}

function found(isbn) {
    stopScanner();
    isbnField.value = isbn;
    lookup();
}

/** The current frame as a JPEG, at most 1280 px: for the NAS to read. */
function frameBlob() {
    const scale = Math.min(1, 1280 / Math.max(video.videoWidth, video.videoHeight));
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(video.videoWidth * scale);
    canvas.height = Math.round(video.videoHeight * scale);
    canvas.getContext('2d').drawImage(video, 0, 0, canvas.width, canvas.height);

    return new Promise((resolve) => canvas.toBlob(resolve, 'image/jpeg', 0.85));
}

/** Read frames until a book's ISBN is found or the scanner is closed. */
async function readFrames(detector) {
    // the browser's reader is fast: often; the NAS takes a second or two
    const pause = detector ? 250 : 1500;

    while (!stopped) {
        if (video.readyState >= 2) {
            try {
                if (detector) {
                    const codes = await detector.detect(video);
                    const isbn = codes.map((c) => c.rawValue).find(isBookIsbn);

                    if (isbn) return found(isbn);
                } else {
                    const blob = await frameBlob();
                    const answer = blob && !stopped ? await scanOnServer(blob) : {};

                    if (answer.isbn) return found(answer.isbn);
                }
            } catch {
                // a frame that couldn't be read: the next one
            }
        }

        await new Promise((resolve) => setTimeout(resolve, pause));
    }
}

async function startScanner() {
    say('');
    scanHelp.textContent = 'Point the camera at the barcode on the back…';
    dialog.showModal();
    stopped = false;

    try {
        stream = await navigator.mediaDevices.getUserMedia({
            video: { facingMode: 'environment', width: { ideal: 1280 }, height: { ideal: 720 } },
            audio: false,
        });
    } catch (e) {
        stopScanner();
        say(`The camera could not be opened (${e.message}): use "From a photo", or type the 13 digits.`, { error: true });
        return;
    }

    video.srcObject = stream;
    await video.play().catch(() => {});

    const detector = await nativeDetector();

    if (!detector) scanHelp.textContent = 'Point the camera at the barcode on the back (read by the NAS: hold it a second or two)…';

    readFrames(detector);
}

if (liveButton && canStream) {
    liveButton.hidden = false;
    photoLabel.hidden = true;
    liveButton.addEventListener('click', startScanner);
    document.getElementById('scan-cancel').addEventListener('click', stopScanner);
    dialog.addEventListener('cancel', stopScanner); // the phone's back button
}
