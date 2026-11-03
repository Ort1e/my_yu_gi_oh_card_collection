// The booster form (templates/pokemon/booster.html): under each variant
// choice of a card printed in several variants, how to recognise the one
// chosen (static/js/pokemon_rarities.js).

for (const select of /** @type {NodeListOf<HTMLSelectElement>} */ (document.querySelectorAll('.variant-select'))) {
    const help = /** @type {HTMLElement} */ (select.parentElement).querySelector('[data-variant-help]');

    if (!help) continue;

    const explain = () => {
        help.textContent = POKEMON_VARIANTS[select.value]?.look ?? '';
    };

    select.addEventListener('change', explain);
    explain();
}
