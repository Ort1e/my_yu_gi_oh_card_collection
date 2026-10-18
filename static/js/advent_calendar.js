// The advent calendar pages (templates/advent/calendar.html,
// templates/books/advent/calendar.html): doors that open
// on a click (remembered in this browser), "open / close every door", and the
// photo shown large.

const calendar = document.getElementById('advent');
const year = calendar.dataset.year;
const doors = [...calendar.querySelectorAll('.door[data-day]')];
// the book calendars have their own key (data-storage-key)
const storageKey = calendar.dataset.storageKey ?? `advent-calendar-${year}-opened`;

/** The days opened before, in this browser (a convenience: may be empty). */
function openedDays() {
    try {
        return new Set(JSON.parse(localStorage.getItem(storageKey) ?? '[]'));
    } catch {
        return new Set();
    }
}

function remember() {
    const days = doors.filter((d) => d.classList.contains('open')).map((d) => d.dataset.day);

    try {
        localStorage.setItem(storageKey, JSON.stringify(days));
    } catch {
        // private window or storage blocked: the doors just close on reload
    }
}

// ---------- doors ----------

const opened = openedDays();

for (const door of doors) {
    if (opened.has(door.dataset.day)) door.classList.add('open');

    door.querySelector('.door-front').addEventListener('click', () => {
        door.classList.add('open');
        remember();
    });
}

/** Open (or close) the doors one after the other, like an advent morning. */
function setAll(open) {
    doors.forEach((door, i) => {
        setTimeout(() => door.classList.toggle('open', open), i * 60);
    });

    setTimeout(remember, doors.length * 60);
}

document.getElementById('advent-open-all').addEventListener('click', () => setAll(true));
document.getElementById('advent-close-all').addEventListener('click', () => setAll(false));

// ---------- photo, large ----------

const lightbox = document.getElementById('advent-lightbox');

calendar.addEventListener('click', (e) => {
    const photo = e.target.closest('.advent-photo');

    if (!photo) return;

    lightbox.querySelector('img').src = photo.dataset.full;
    lightbox.showModal();
});

// a click on the dark backdrop closes it too
lightbox.addEventListener('click', (e) => {
    if (e.target === lightbox) lightbox.close();
});
