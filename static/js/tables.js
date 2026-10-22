// Data tables (static/css/style.css, "Data tables"), on any page that loads
// this script:
// - <table class="sortable">: a click on a heading with `data-sort="text"` or
//   `data-sort="number"` sorts the rows of the <tbody> by that column (again:
//   the other way round). A cell can give the value to sort by in
//   `data-sort-value` (a date, a rarity's rank); else its text is used.
//   Empty values ("", "—") always go last.
// - <input type="search" data-filter-table="<table id>">: shows only the rows
//   whose text contains every word typed (case and accents ignored); an
//   element with `data-count-for="<table id>"` says how many are shown.
//   Buttons with `data-filter-for="<table id>"` and `data-filter-tag` keep
//   only the rows with that exact `data-tag` (a rarity: "Rare" is not "Super
//   Rare"); a second click clears it.

/** Lowercase, without accents: "Beauté" → "beaute". */
const fold = (text) => text.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();

const cellValue = (row, index) => {
    const cell = row.cells[index];

    if (!cell) return '';

    return (cell.dataset.sortValue ?? cell.textContent).trim();
};

const isEmpty = (value) => value === '' || value === '—';

// ---------- sorting ----------

function sortBy(table, heading) {
    const index = [...heading.parentElement.children].indexOf(heading);
    const numeric = heading.dataset.sort === 'number';
    const ascending = heading.getAttribute('aria-sort') !== 'ascending';

    const compare = (a, b) => {
        const [x, y] = [cellValue(a, index), cellValue(b, index)];

        if (isEmpty(x) || isEmpty(y)) return isEmpty(x) - isEmpty(y);

        const order = numeric
            ? parseFloat(x.replace(',', '.')) - parseFloat(y.replace(',', '.'))
            : fold(x).localeCompare(fold(y));

        return ascending ? order : -order;
    };

    const body = table.tBodies[0];
    const rows = [...body.rows].sort(compare);
    body.append(...rows);

    table.querySelectorAll('th[aria-sort]').forEach((th) => th.removeAttribute('aria-sort'));
    heading.setAttribute('aria-sort', ascending ? 'ascending' : 'descending');
}

document.querySelectorAll('table.sortable').forEach((table) => {
    table.querySelectorAll('thead th[data-sort]').forEach((heading) => {
        heading.title ||= 'Sort by this column';
        heading.addEventListener('click', () => sortBy(table, heading));
    });
});

// ---------- filtering ----------

function filter(table, query, tag) {
    const words = fold(query).split(/\s+/).filter(Boolean);
    let shown = 0;

    for (const row of table.tBodies[0].rows) {
        const text = fold(row.textContent);
        const match = words.every((word) => text.includes(word)) && (tag === null || row.dataset.tag === tag);
        row.hidden = !match;

        if (match) shown++;
    }

    const count = document.querySelector(`[data-count-for="${table.id}"]`);

    if (count) {
        const total = table.tBodies[0].rows.length;
        count.textContent = shown < total ? `${shown} of ${total} rows` : `${total} rows`;
    }
}

document.querySelectorAll('input[data-filter-table]').forEach((input) => {
    const table = document.getElementById(input.dataset.filterTable);

    if (!table) return;

    const buttons = [...document.querySelectorAll(`[data-filter-tag][data-filter-for="${table.id}"]`)];
    let tag = null;

    const apply = () => filter(table, input.value, tag);

    input.addEventListener('input', apply);

    for (const button of buttons) {
        button.addEventListener('click', () => {
            tag = tag === button.dataset.filterTag ? null : button.dataset.filterTag;
            buttons.forEach((b) => b.classList.toggle('active', b.dataset.filterTag === tag));
            apply();
        });
    }

    apply();
});
