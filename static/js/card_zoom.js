// A card's picture marked `data-zoom` (a card's page, the Pokémon wishlist):
// a click or a tap on it shows it as large as the screen allows
// (`.lightbox.card-zoom`, style.css); a click or a tap anywhere, or Escape,
// closes it. The way to see a card large on a phone, where card_preview.js
// does nothing. In the app and the public copy.

(function () {
    const pictures = document.querySelectorAll('img[data-zoom]');

    if (pictures.length === 0) {
        return;
    }

    const lightbox = document.createElement('dialog');
    lightbox.className = 'lightbox card-zoom';

    const large = document.createElement('img');
    lightbox.appendChild(large);
    document.body.appendChild(lightbox);

    function open(picture) {
        const ratio = (picture.naturalWidth || 59) / (picture.naturalHeight || 86);

        // never more than 1.5 times the picture's own size: blurred beyond
        const natural = picture.naturalHeight ? picture.naturalHeight * 1.5 : 1100;
        const height = Math.min(window.innerHeight * 0.9, (window.innerWidth * 0.94 - 16) / ratio, natural);

        large.src = picture.currentSrc || picture.src;
        large.alt = picture.alt;
        large.style.height = `${height}px`;
        large.style.width = `${height * ratio}px`;
        lightbox.setAttribute('aria-label', picture.alt);
        lightbox.showModal();
    }

    for (const picture of pictures) {
        picture.addEventListener('click', () => open(picture));
    }

    lightbox.addEventListener('click', () => lightbox.close());
})();
