// A card's page (cards/detail.html, pokemon/detail.html): a click or a tap on
// its picture shows it as large as the screen allows (`.lightbox.card-zoom`,
// style.css); a click or a tap anywhere, or Escape, closes it. The way to see
// a card large on a phone, where card_preview.js does nothing. In the app and
// the public copy.

(function () {
    const picture = document.querySelector('.card-detail-image');

    if (!picture) {
        return;
    }

    const lightbox = document.createElement('dialog');
    lightbox.className = 'lightbox card-zoom';
    lightbox.setAttribute('aria-label', picture.alt);

    const large = document.createElement('img');
    large.alt = picture.alt;
    lightbox.appendChild(large);
    document.body.appendChild(lightbox);

    picture.addEventListener('click', () => {
        const ratio = (picture.naturalWidth || 59) / (picture.naturalHeight || 86);
        const height = Math.min(window.innerHeight * 0.9, (window.innerWidth * 0.94 - 16) / ratio, 1100);

        large.src = picture.currentSrc || picture.src;
        large.style.height = `${height}px`;
        large.style.width = `${height * ratio}px`;
        lightbox.showModal();
    });

    lightbox.addEventListener('click', () => lightbox.close());
})();
