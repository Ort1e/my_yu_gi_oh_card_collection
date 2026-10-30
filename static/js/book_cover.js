// A book's page (templates/books/detail.html): its cover comes from Open
// Library, which answers 404 for a book it has no cover of. Then the image
// is removed and the plain "book" under it shows.

for (const img of document.querySelectorAll('.book-cover-image')) {
    const drop = () => img.remove();

    // already failed before this script ran (a cached 404)
    if (img.complete && img.naturalWidth === 0) {
        drop();
        continue;
    }

    img.addEventListener('error', drop);
}
