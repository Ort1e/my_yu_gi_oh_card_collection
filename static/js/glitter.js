// A short sparkle when an add page opens, in the colours of the season or of a festive day.
// `<script src="/static/js/glitter.js" defer>`: add page, every visit.
// `data-glitter="festive"`: a home, only on a festive day and once a day per browser.
// Test: `?glitter-date=YYYY-MM-DD` fakes the date, `?glitter=force` ignores the once-a-day key.
// Cost: ~30 absolutely positioned spans animated by the compositor (transform + opacity), then gone.

/** Birthdays, as [month, day]. */
const BIRTHDAYS = [
    [1, 6],
    [11, 28],
];

const RAINBOW = ['#ff5252', '#ff9800', '#ffeb3b', '#4caf50', '#2196f3', '#9c27b0'];

/**
 * @typedef {object} Palette
 * @property {string[]} colors
 * @property {string[]} shapes file names in /static/img/glitter/
 * @property {boolean} festive
 * @property {number} [advent] the day number shown on three baubles (1-24 December)
 */

/**
 * Easter Sunday of a year (anonymous Gregorian computus).
 * @param {number} year
 * @returns {Date}
 */
function easterSunday(year) {
    const a = year % 19;
    const b = Math.floor(year / 100);
    const c = year % 100;
    const d = Math.floor(b / 4);
    const e = b % 4;
    const f = Math.floor((b + 8) / 25);
    const g = Math.floor((b - f + 1) / 3);
    const h = (19 * a + b - d - g + 15) % 30;
    const i = Math.floor(c / 4);
    const k = c % 4;
    const l = (32 + 2 * e + 2 * i - h - k) % 7;
    const m = Math.floor((a + 11 * h + 22 * l) / 451);
    const month = Math.floor((h + l - 7 * m + 114) / 31);
    const day = ((h + l - 7 * m + 114) % 31) + 1;

    return new Date(year, month - 1, day);
}

/**
 * The palette of a date, first match wins.
 * @param {Date} date
 * @param {boolean} addPage the add pages' Christmas lasts all of 1-26 December (mini advent calendar)
 * @returns {Palette}
 */
function paletteFor(date, addPage) {
    const month = date.getMonth() + 1;
    const day = date.getDate();

    if (BIRTHDAYS.some(([m, d]) => m === month && d === day)) {
        return {
            colors: ['#ffd700', '#ffe066', ...RAINBOW],
            shapes: ['balloon', 'party', 'cake'],
            festive: true,
        };
    }

    if (month === 10 && day >= 25) {
        return {
            colors: ['#ff8a00', '#ff6d00', '#8e24aa', '#6a1b9a', '#9be71c'],
            shapes: ['pumpkin', 'bat', 'ghost'],
            festive: true,
        };
    }

    if (month === 12 && day <= 26 && (addPage || day >= 20)) {
        /** @type {Palette} */
        const christmas = {
            colors: ['#d32f2f', '#2e7d32', '#ffd700', '#ffffff'],
            shapes: ['star', 'tree', 'snowflake'],
            festive: day >= 20,
        };

        if (addPage && day <= 24) {
            christmas.advent = day;
        }

        return christmas;
    }

    const easter = easterSunday(date.getFullYear()).getTime();
    const noon = new Date(date.getFullYear(), month - 1, day).getTime();
    const dayMs = 24 * 60 * 60 * 1000;

    if (noon >= easter - 2 * dayMs && noon <= easter + dayMs) {
        return {
            colors: ['#f8bbd0', '#c5e1a5', '#fff59d', '#b3e5fc', '#d1c4e9'],
            shapes: ['chick', 'egg', 'tulip'],
            festive: true,
        };
    }

    if (month >= 3 && month <= 5) {
        return {
            colors: ['#81c784', '#a5d6a7', '#f48fb1', '#f8bbd0', '#fff176'],
            shapes: ['blossom', 'seedling'],
            festive: false,
        };
    }

    if (month >= 6 && month <= 8) {
        return {
            colors: ['#ffd54f', '#ffb300', '#ff8f00', '#4dd0e1', '#fff176'],
            shapes: ['sun', 'sunflower'],
            festive: false,
        };
    }

    if (month >= 9 && month <= 11) {
        return {
            colors: ['#ff8f00', '#e65100', '#bf360c', '#ffb74d', '#a1887f'],
            shapes: ['leaves', 'maple'],
            festive: false,
        };
    }

    return {
        colors: ['#b3e5fc', '#81d4fa', '#e1f5fe', '#cfd8dc', '#ffffff'],
        shapes: ['snowflake', 'snowman'],
        festive: false,
    };
}

/**
 * @param {number} min
 * @param {number} max
 * @returns {number}
 */
function between(min, max) {
    return min + Math.random() * (max - min);
}

/**
 * @param {string[]} list
 * @returns {string}
 */
function pick(list) {
    return list[Math.floor(Math.random() * list.length)] ?? '#ffffff';
}

/**
 * @param {Date} date
 * @returns {string} local ISO date
 */
function isoDate(date) {
    const pad = (/** @type {number} */ n) => String(n).padStart(2, '0');

    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * Today, or the date of `?glitter-date=`.
 * @param {URLSearchParams} params
 * @returns {Date}
 */
function currentDate(params) {
    const fake = /^(\d{4})-(\d{2})-(\d{2})$/.exec(params.get('glitter-date') ?? '');

    if (fake) {
        return new Date(Number(fake[1]), Number(fake[2]) - 1, Number(fake[3]));
    }

    return new Date();
}

/**
 * Put a particle in the layer and start its animation.
 * @param {HTMLElement} layer
 * @param {HTMLElement} element
 * @param {number} delay ms
 * @param {number} duration ms
 * @param {number} hold share of the duration spent at full size (0-1)
 */
function launch(layer, element, delay, duration, hold) {
    const dx = between(-40, 40);
    const dy = between(60, 160);
    const turn = between(-60, 60);

    element.style.position = 'absolute';
    element.style.left = `${between(2, 96)}%`;
    element.style.top = `${between(2, 55)}%`;
    layer.appendChild(element);

    element.animate(
        [
            { transform: 'translate(0, 0) scale(0) rotate(0deg)', opacity: 0, offset: 0 },
            { transform: `translate(${dx * 0.2}px, ${dy * 0.15}px) scale(1) rotate(${turn * 0.3}deg)`, opacity: 1, offset: 0.2 },
            { transform: `translate(${dx * 0.2 + 4}px, ${dy * 0.15 + 8}px) scale(1) rotate(${turn * 0.4}deg)`, opacity: 1, offset: 0.2 + hold },
            { transform: `translate(${dx}px, ${dy}px) scale(0) rotate(${turn}deg)`, opacity: 0, offset: 1 },
        ],
        { duration, delay, easing: 'ease-in-out', fill: 'backwards' },
    );
}

/**
 * A sparkle (four-point star) or a round dot.
 * @param {string} color
 * @returns {HTMLElement}
 */
function sparkle(color) {
    const element = document.createElement('span');
    const size = between(4, 10);

    element.style.width = `${size}px`;
    element.style.height = `${size}px`;
    element.style.background = color;

    if (Math.random() < 0.65) {
        element.style.clipPath = 'polygon(50% 0, 62% 38%, 100% 50%, 62% 62%, 50% 100%, 38% 62%, 0 50%, 38% 38%)';
        element.style.width = `${size * 1.5}px`;
        element.style.height = `${size * 1.5}px`;
    } else {
        element.style.borderRadius = '50%';
    }

    return element;
}

/**
 * A small picture of the occasion; a file that fails to load removes its particle.
 * @param {string} name
 * @returns {HTMLElement}
 */
function shape(name) {
    const element = document.createElement('img');
    const size = Math.round(between(14, 18));

    element.src = `/static/img/glitter/${name}.svg`;
    element.alt = '';
    element.width = size;
    element.height = size;
    element.addEventListener('error', () => element.remove());

    return element;
}

/**
 * A gold bauble carrying the day's number (mini advent calendar).
 * @param {number} day
 * @returns {HTMLElement}
 */
function bauble(day) {
    const element = document.createElement('span');

    element.textContent = String(day);
    element.style.width = '24px';
    element.style.height = '24px';
    element.style.borderRadius = '50%';
    element.style.background = 'radial-gradient(circle at 35% 30%, #fff3b0, #ffd700 45%, #b8860b)';
    element.style.color = '#fff';
    element.style.font = 'bold 12px/24px sans-serif';
    element.style.textAlign = 'center';
    element.style.textShadow = '0 0 2px rgba(0, 0, 0, 0.8), 0 1px 1px rgba(0, 0, 0, 0.6)';

    return element;
}

/**
 * @param {Palette} palette
 */
function run(palette) {
    const layer = document.createElement('div');

    layer.style.cssText = 'position: fixed; inset: 0; overflow: hidden; pointer-events: none; z-index: 9999;';
    layer.setAttribute('aria-hidden', 'true');

    const total = 30;
    const spread = 2500;
    const pictures = 6;
    const baubles = palette.advent === undefined ? 0 : 3;

    for (let i = 0; i < total; i++) {
        const delay = between(0, spread);
        let element;

        if (i < baubles && palette.advent !== undefined) {
            element = bauble(palette.advent);
            launch(layer, element, delay * 0.8, 3000, 0.45);
            continue;
        }

        if (i < baubles + pictures && palette.shapes.length > 0) {
            element = shape(palette.shapes[i % palette.shapes.length] ?? '');
        } else {
            element = sparkle(pick(palette.colors));
        }

        launch(layer, element, delay, between(1200, 2000), 0.15);
    }

    document.body.appendChild(layer);
    window.setTimeout(() => layer.remove(), spread + 3200);
}

function start() {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
        return;
    }

    if (document.querySelector('.error')) {
        return;
    }

    const params = new URLSearchParams(window.location.search);
    const home = script?.dataset.glitter === 'festive';
    const date = currentDate(params);
    const palette = paletteFor(date, !home);

    if (home) {
        if (!palette.festive) {
            return;
        }

        const today = isoDate(date);

        try {
            if (params.get('glitter') !== 'force' && window.localStorage.getItem('glitter-last') === today) {
                return;
            }

            window.localStorage.setItem('glitter-last', today);
        } catch {
            // Storage can be off: sparkle anyway.
        }
    }

    run(palette);
}

const script = /** @type {HTMLScriptElement | null} */ (document.currentScript);

start();
