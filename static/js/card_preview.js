// The card seen large: hovering a picture of a card grid
// (partials/card_grid.html) or one marked `data-zoom` (the Pokémon wishlist)
// shows it large beside it, on the side with room, until the mouse leaves
// (`.card-preview`, tiles.css). With a mouse only: phones and tablets keep
// the page as it is (card_zoom.js, a tap). In the app and the public copy.

(function () {
    if (!window.matchMedia('(hover: hover) and (pointer: fine)').matches) {
        return;
    }

    const pictures = document.querySelectorAll('.card-tile-picture img, img[data-zoom]:not(.card-detail-image)');

    if (pictures.length === 0) {
        return;
    }

    const preview = document.createElement('img');
    preview.className = 'card-preview';
    preview.alt = '';
    preview.hidden = true;
    document.body.appendChild(preview);

    // the card's proportions (Yu-Gi-Oh! 59 × 86 mm, Pokémon 63 × 88); out of
    // a grid, the picture's own
    function ratio(picture) {
        if (picture.closest('.card-tile.yugioh')) {
            return 59 / 86;
        }

        if (picture.closest('.card-tile') || !picture.naturalHeight) {
            return 63 / 88;
        }

        return picture.naturalWidth / picture.naturalHeight;
    }

    let timer = null;

    function show(picture) {
        const gap = 12;
        const rect = picture.getBoundingClientRect();
        const height = Math.min(window.innerHeight * 0.8, 560);
        const width = height * ratio(picture);

        let left = rect.right + gap;

        if (left + width > window.innerWidth - gap) {
            left = Math.max(gap, rect.left - gap - width);
        }

        const middle = rect.top + rect.height / 2 - height / 2;
        const top = Math.min(Math.max(gap, middle), window.innerHeight - height - gap);

        preview.src = picture.currentSrc || picture.src;
        preview.style.width = `${width}px`;
        preview.style.height = `${height}px`;
        preview.style.left = `${left}px`;
        preview.style.top = `${top}px`;
        preview.hidden = false;
    }

    function hide() {
        clearTimeout(timer);
        preview.hidden = true;
    }

    for (const picture of pictures) {
        // a short wait: moving across the grid doesn't flash every card
        picture.addEventListener('mouseenter', () => {
            clearTimeout(timer);
            timer = setTimeout(() => show(picture), 150);
        });

        picture.addEventListener('mouseleave', hide);
    }

    window.addEventListener('scroll', hide, { passive: true });
})();
