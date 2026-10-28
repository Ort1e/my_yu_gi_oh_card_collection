// "← Back to …" (`.back-link a`, every page): when the visitor came from
// that list (another page of it, the table view), back to that very page and
// to the same place in it, instead of the list's first page. In the app
// (`/cards?page=3`) and the public copy (`cards-page-3.html`).

(function () {
    const link = document.querySelector('.back-link a');

    if (!link || !document.referrer || history.length < 2) {
        return;
    }

    let from;

    try {
        from = new URL(document.referrer);
    } catch {
        return;
    }

    const to = new URL(link.href);
    const stem = (url) => url.pathname.replace(/\.html$/, '');

    if (from.origin !== to.origin || !stem(from).startsWith(stem(to))) {
        return;
    }

    // the same list (the app keeps the page in the query, the copy in the
    // file name), not a card's page or a form under it
    const rest = stem(from).slice(stem(to).length);

    if (rest !== '' && !/^(-(page|view)-\w+)+$/.test(rest)) {
        return;
    }

    link.href = document.referrer;

    link.addEventListener('click', (e) => {
        if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) {
            return;
        }

        e.preventDefault();
        history.back();
    });
})();
