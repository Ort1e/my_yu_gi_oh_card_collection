// A short burst of glitter when an add page opens (and, lighter and once a
// day, on the three collection homes: <script data-glitter="light">). The
// skin follows the date: birthdays, Easter, Halloween, a mini advent calendar
// from 1 to 24 December, else the season. Test: ?glitter=<skin> (birthday,
// easter, halloween, christmas, spring, summer, autumn, winter) and
// ?glitter-date=YYYY-MM-DD in the URL.
//
// Cheap by design: one fixed canvas that ignores the pointer, every shape
// pre-rendered once as a sprite, ~50 particles, a few seconds, then the
// canvas is removed and nothing runs any more.
//
// To add a kind: a `draw(g, r, color)` below and an entry in a skin's list.
// To add a skin: a function in SKINS and a rule in skinFor().

/**
 * @typedef {(g: CanvasRenderingContext2D, r: number, color: string) => void} Draw
 * @typedef {'fall' | 'flutter' | 'rise' | 'twinkle'} Motion
 * @typedef {{ draw: Draw, colors: string[], weight: number, motion: Motion, size?: number, res?: number }} Kind
 * @typedef {{ kinds: Kind[], count: number, duration: number, door?: number }} Skin
 */

/** Days of the year with a skin of their own: [month, day]. */
const BIRTHDAYS = [
    [11, 28],
    [1, 6],
];

const GOLD = '#f5c542';
const SPRITE = 12; // sprite radius, css px

const params = new URLSearchParams(location.search);
const mode = document.currentScript?.dataset.glitter ?? 'burst';

// ---------- drawing (origin at the centre, r = radius) ----------

/** Fill the current path. */
function paint(g, color) {
    g.fillStyle = color;
    g.fill();
}

function oval(g, x, y, rx, ry, rot = 0) {
    g.beginPath();
    g.ellipse(x, y, rx, ry, rot, 0, Math.PI * 2);
}

function rect(g, x, y, w, h) {
    g.beginPath();
    g.rect(x, y, w, h);
}

/** @param {number} n points @param {number} inner ratio of the inner radius */
function starPath(g, r, n, inner) {
    g.beginPath();

    for (let i = 0; i < n * 2; i++) {
        const a = (Math.PI * i) / n - Math.PI / 2;
        const d = i % 2 === 0 ? r : r * inner;
        g.lineTo(Math.cos(a) * d, Math.sin(a) * d);
    }

    g.closePath();
}

/** @type {Draw} */
const sparkle = (g, r, c) => {
    g.beginPath();
    g.moveTo(0, -r);
    g.quadraticCurveTo(0, 0, r, 0);
    g.quadraticCurveTo(0, 0, 0, r);
    g.quadraticCurveTo(0, 0, -r, 0);
    g.quadraticCurveTo(0, 0, 0, -r);
    paint(g, c);
};

/** @type {Draw} */
const dot = (g, r, c) => {
    oval(g, 0, 0, r * 0.6, r * 0.6);
    paint(g, c);
};

/** @type {Draw} */
const star = (g, r, c) => {
    starPath(g, r, 5, 0.45);
    paint(g, c);
};

/** @type {Draw} */
const snowflake = (g, r, c) => {
    for (let i = 0; i < 3; i++) {
        g.save();
        g.rotate((Math.PI * i) / 3);
        rect(g, -r, -r * 0.09, r * 2, r * 0.18);
        paint(g, c);

        for (const x of [-0.65, 0.65]) {
            oval(g, r * x, 0, r * 0.14, r * 0.3);
            paint(g, c);
        }

        g.restore();
    }

    oval(g, 0, 0, r * 0.22, r * 0.22);
    paint(g, c);
};

/** @type {Draw} */
const bauble = (g, r, c) => {
    oval(g, 0, r * 0.15, r * 0.75, r * 0.75);
    paint(g, c);
    rect(g, -r * 0.2, -r * 0.8, r * 0.4, r * 0.3);
    paint(g, GOLD);
    oval(g, -r * 0.25, -r * 0.1, r * 0.15, r * 0.25, 0.6);
    paint(g, 'rgba(255,255,255,.6)');
};

/** @type {Draw} */
const candyCane = (g, r, c) => {
    g.save();
    g.beginPath();
    g.moveTo(-r * 0.55, r * 0.9);
    g.lineTo(-r * 0.55, -r * 0.2);
    g.arc(r * 0.1, -r * 0.2, r * 0.65, Math.PI, 0);
    g.lineTo(r * 0.25, -r * 0.2);
    g.arc(r * 0.1, -r * 0.2, r * 0.15, 0, Math.PI, true);
    g.lineTo(-r * 0.05, r * 0.9);
    g.closePath();
    paint(g, '#ffffff');
    g.clip();

    for (let y = -1; y < 1.2; y += 0.5) {
        g.beginPath();
        g.moveTo(-r, r * y);
        g.lineTo(r, r * (y - 0.4));
        g.lineTo(r, r * (y - 0.1));
        g.lineTo(-r, r * (y + 0.3));
        paint(g, c);
    }

    g.restore();
};

/** @type {Draw} */
const holly = (g, r, c) => {
    oval(g, -r * 0.35, -r * 0.2, r * 0.7, r * 0.32, -0.6);
    paint(g, c);
    oval(g, r * 0.35, -r * 0.2, r * 0.7, r * 0.32, 0.6);
    paint(g, c);

    for (const x of [-0.3, 0.3, 0]) {
        oval(g, r * x, r * (x === 0 ? 0.1 : 0.4), r * 0.3, r * 0.3);
        paint(g, '#c0392b');
    }
};

/** @type {Draw} */
const bell = (g, r, c) => {
    g.beginPath();
    g.moveTo(-r * 0.8, r * 0.5);
    g.quadraticCurveTo(-r * 0.7, -r, 0, -r);
    g.quadraticCurveTo(r * 0.7, -r, r * 0.8, r * 0.5);
    g.closePath();
    paint(g, c);
    oval(g, 0, r * 0.62, r * 0.18, r * 0.18);
    paint(g, '#8a6d1d');
};

/** @type {Draw} */
const gift = (g, r, c) => {
    rect(g, -r * 0.75, -r * 0.45, r * 1.5, r * 1.2);
    paint(g, c);
    rect(g, -r * 0.12, -r * 0.45, r * 0.24, r * 1.2);
    paint(g, GOLD);
    rect(g, -r * 0.75, r * 0.05, r * 1.5, r * 0.22);
    paint(g, GOLD);
    oval(g, -r * 0.28, -r * 0.62, r * 0.28, r * 0.17, -0.4);
    paint(g, GOLD);
    oval(g, r * 0.28, -r * 0.62, r * 0.28, r * 0.17, 0.4);
    paint(g, GOLD);
};

/** @type {Draw} */
const mitten = (g, r, c) => {
    oval(g, 0, 0, r * 0.55, r * 0.75);
    paint(g, c);
    oval(g, -r * 0.55, r * 0.05, r * 0.22, r * 0.38, 0.5);
    paint(g, c);
    rect(g, -r * 0.6, r * 0.5, r * 1.2, r * 0.35);
    paint(g, '#ffffff');
};

/** @type {Draw} */
const gingerbread = (g, r, c) => {
    oval(g, 0, r * 0.15, r * 0.3, r * 0.45);
    paint(g, c);
    oval(g, 0, -r * 0.5, r * 0.36, r * 0.36);
    paint(g, c);

    for (const x of [-1, 1]) {
        oval(g, x * r * 0.5, -r * 0.02, r * 0.4, r * 0.15, x * -0.25);
        paint(g, c);
        oval(g, x * r * 0.27, r * 0.65, r * 0.15, r * 0.35, x * 0.35);
        paint(g, c);
        oval(g, x * r * 0.13, -r * 0.55, r * 0.05, r * 0.05);
        paint(g, '#ffffff');
    }

    for (const y of [0.05, 0.3]) {
        oval(g, 0, r * y, r * 0.07, r * 0.07);
        paint(g, '#ffffff');
    }
};

/** @type {Draw} */
const candle = (g, r, c) => {
    rect(g, -r * 0.25, -r * 0.3, r * 0.5, r * 1.2);
    paint(g, c);
    oval(g, 0, -r * 0.65, r * 0.18, r * 0.36);
    paint(g, '#ff9f1c');
};

/** @type {Draw} */
const tree = (g, r, c) => {
    for (const [y, w] of [[-0.9, 0.5], [-0.35, 0.75], [0.2, 1]]) {
        g.beginPath();
        g.moveTo(0, r * y);
        g.lineTo(r * w, r * (y + 0.75));
        g.lineTo(-r * w, r * (y + 0.75));
        g.closePath();
        paint(g, c);
    }

    rect(g, -r * 0.12, r * 0.9, r * 0.24, r * 0.2);
    paint(g, '#7b4a1e');
};

/** @type {Draw} */
const angel = (g, r, c) => {
    starPath(g, r * 0.85, 5, 0.45);
    paint(g, c);
    oval(g, 0, -r * 0.95, r * 0.4, r * 0.12);
    paint(g, '#ffffff');
};

/** @type {Draw} */
const flower = (g, r, c) => {
    for (let i = 0; i < 5; i++) {
        const a = (Math.PI * 2 * i) / 5;
        oval(g, Math.cos(a) * r * 0.5, Math.sin(a) * r * 0.5, r * 0.4, r * 0.4);
        paint(g, c);
    }

    oval(g, 0, 0, r * 0.25, r * 0.25);
    paint(g, '#ffd23f');
};

/** @type {Draw} */
const petal = (g, r, c) => {
    oval(g, 0, 0, r * 0.9, r * 0.5, 0.4);
    paint(g, c);
};

/** @type {Draw} */
const leaf = (g, r, c) => {
    g.beginPath();
    g.moveTo(-r, r * 0.6);
    g.quadraticCurveTo(-r * 0.6, -r, r, -r * 0.7);
    g.quadraticCurveTo(r * 0.6, r * 0.9, -r, r * 0.6);
    paint(g, c);
    oval(g, 0, -r * 0.05, r * 0.7, r * 0.05, -0.65);
    paint(g, 'rgba(0,0,0,.3)');
};

/** @type {Draw} */
const egg = (g, r, c) => {
    g.save();
    oval(g, 0, 0, r * 0.65, r * 0.85);
    paint(g, c);
    g.clip();
    rect(g, -r, -r * 0.15, r * 2, r * 0.3);
    paint(g, '#ffffff');
    g.restore();
};

/** @type {Draw} */
const chick = (g, r, c) => {
    oval(g, 0, r * 0.25, r * 0.7, r * 0.6);
    paint(g, c);
    oval(g, r * 0.15, -r * 0.4, r * 0.4, r * 0.4);
    paint(g, c);
    g.beginPath();
    g.moveTo(r * 0.5, -r * 0.45);
    g.lineTo(r * 0.95, -r * 0.35);
    g.lineTo(r * 0.5, -r * 0.25);
    paint(g, '#ff8c42');
    oval(g, r * 0.25, -r * 0.5, r * 0.07, r * 0.07);
    paint(g, '#222222');
};

/** @type {Draw} */
const pumpkin = (g, r, c) => {
    oval(g, 0, r * 0.1, r * 0.85, r * 0.7);
    paint(g, c);

    for (const x of [-0.35, 0.35]) {
        oval(g, r * x, r * 0.1, r * 0.06, r * 0.6);
        paint(g, 'rgba(0,0,0,.2)');
    }

    rect(g, -r * 0.1, -r * 0.85, r * 0.2, r * 0.35);
    paint(g, '#3a7d2c');
};

/** @type {Draw} */
const bat = (g, r, c) => {
    g.beginPath();
    g.moveTo(0, -r * 0.3);
    g.quadraticCurveTo(r * 0.5, -r, r, -r * 0.2);
    g.quadraticCurveTo(r * 0.7, 0, r * 0.55, r * 0.15);
    g.quadraticCurveTo(r * 0.35, -r * 0.05, r * 0.2, r * 0.3);
    g.lineTo(0, r * 0.1);
    g.lineTo(-r * 0.2, r * 0.3);
    g.quadraticCurveTo(-r * 0.35, -r * 0.05, -r * 0.55, r * 0.15);
    g.quadraticCurveTo(-r * 0.7, 0, -r, -r * 0.2);
    g.quadraticCurveTo(-r * 0.5, -r, 0, -r * 0.3);
    paint(g, c);
};

/** @type {Draw} */
const balloon = (g, r, c) => {
    oval(g, 0, -r * 0.2, r * 0.6, r * 0.75);
    paint(g, c);
    g.beginPath();
    g.moveTo(-r * 0.12, r * 0.6);
    g.lineTo(r * 0.12, r * 0.6);
    g.lineTo(0, r * 0.45);
    paint(g, c);
    oval(g, -r * 0.2, -r * 0.4, r * 0.1, r * 0.2, 0.4);
    paint(g, 'rgba(255,255,255,.5)');
};

/** @type {Draw} */
const cake = (g, r, c) => {
    rect(g, -r * 0.8, -r * 0.1, r * 1.6, r * 0.8);
    paint(g, c);
    rect(g, -r * 0.8, -r * 0.1, r * 1.6, r * 0.3);
    paint(g, '#ffffff');
    rect(g, -r * 0.06, -r * 0.55, r * 0.12, r * 0.45);
    paint(g, '#4dabf7');
    oval(g, 0, -r * 0.7, r * 0.1, r * 0.17);
    paint(g, '#ff9f1c');
};

/** A numbered tile, like a door of the advent calendar. @param {number} day @returns {Draw} */
const door = (day) => (g, r, c) => {
    rect(g, -r * 0.95, -r * 0.95, r * 1.9, r * 1.9);
    paint(g, GOLD);
    rect(g, -r * 0.8, -r * 0.8, r * 1.6, r * 1.6);
    paint(g, c);
    g.fillStyle = '#ffffff';
    g.font = `bold ${r * 1.25}px sans-serif`;
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    g.fillText(String(day), 0, r * 0.07);
};

// ---------- kinds and skins ----------

/**
 * @param {Draw} draw @param {string[]} colors @param {number} weight
 * @param {Motion} [motion] @param {number} [size]
 * @returns {Kind}
 */
function kind(draw, colors, weight, motion = 'fall', size = 1) {
    return { draw, colors, weight, motion, size };
}

/** The glitter itself: on top of every skin. */
const glitter = (/** @type {string[]} */ colors, weight = 2) => kind(sparkle, colors, weight, 'twinkle', 0.7);

/** The twelve Christmas kinds, in the order the days unlock them. */
const XMAS = [
    kind(snowflake, ['#ffffff', '#cfe8ff'], 1),
    kind(star, [GOLD, '#fff3b0'], 1, 'twinkle'),
    kind(bauble, ['#d62828', '#2a9d8f', '#4361ee', '#9b5de5'], 1),
    kind(candyCane, ['#d62828'], 1),
    kind(holly, ['#2e7d32', '#3a9d44'], 1),
    kind(bell, [GOLD, '#e0b13a'], 1),
    kind(gift, ['#d62828', '#2a9d8f', '#4361ee'], 1),
    kind(mitten, ['#d62828', '#4361ee', '#2a9d8f'], 1),
    kind(gingerbread, ['#b5651d', '#a0522d'], 1),
    kind(candle, ['#f1e3c6', '#d62828'], 1),
    kind(tree, ['#2e7d32', '#1b5e20'], 1),
    kind(angel, [GOLD, '#fff3b0'], 1, 'twinkle', 1),
];

/** Gilded: the same shape in gold. @param {Kind} k @param {number} weight @param {boolean} [gilded] @returns {Kind} */
const weighted = (k, weight, gilded = false) => ({ ...k, weight, colors: gilded ? [GOLD, '#ffe08a', '#e0b13a'] : k.colors });

/** Christmas, day `n` of the mini calendar (1-24), 25 = the finale. @returns {Skin} */
function christmasDay(n) {
    const sparks = glitter([GOLD, '#ffffff', '#fff3b0']);

    if (n >= 24) {
        const kinds = [...XMAS.map((k) => weighted(k, 1.5)), sparks, kind(dot, [GOLD, '#fff3b0'], 3, 'fall', 0.8), kind(star, [GOLD], 4, 'twinkle', 0.7)];

        return { kinds, count: 190, duration: 4.4, door: n };
    }

    const featured = (n - 1) % 12;
    const unlocked = XMAS.slice(0, Math.min(n, 12));
    const kinds = unlocked.map((k, i) => weighted(k, i === featured ? 4 : 1));

    // second half: the kinds come back gilded
    for (const k of XMAS.slice(0, Math.max(0, n - 12))) {
        kinds.push(weighted(k, 1, true));
    }

    kinds.push(sparks);

    return { kinds, count: Math.round(60 + ((n - 1) * 60) / 23), duration: 2.5 + (n - 1) / 23, door: n };
}

/** @type {Record<string, (day?: number) => Skin>} */
const SKINS = {
    birthday: () => ({
        count: 110,
        duration: 3,
        kinds: [
            kind(dot, ['#ef476f', '#ffd166', '#06d6a0', '#118ab2', '#9b5de5'], 4, 'fall', 0.8),
            kind(balloon, ['#ef476f', '#ffd166', '#118ab2', '#9b5de5'], 2, 'rise', 1),
            kind(cake, ['#f4a6c0', '#c8a2e0'], 1, 'fall', 1),
            glitter(['#ffffff', '#ffd166']),
        ],
    }),
    easter: () => ({
        count: 90,
        duration: 2.8,
        kinds: [
            kind(egg, ['#f7b2d9', '#b8e0f7', '#c8f0b0', '#ffe49a', '#d7bdf7'], 3, 'fall', 0.9),
            kind(chick, ['#ffe066'], 1.5, 'fall', 0.9),
            kind(flower, ['#ffffff', '#ffb3c6', '#c8b6ff'], 2, 'fall', 0.9),
            glitter(['#ffffff', '#ffe49a']),
        ],
    }),
    halloween: () => ({
        count: 100,
        duration: 2.8,
        kinds: [
            kind(pumpkin, ['#ff7518', '#f08a24'], 25, 'fall', 0.9),
            kind(bat, ['#1b1b1b', '#2e2e3a'], 20, 'flutter', 1),
            glitter(['#9b5de5', '#c77dff', '#ff7518'], 55),
        ],
    }),
    christmas: (day = 12) => christmasDay(day),
    spring: () => ({
        count: 90,
        duration: 2.5,
        kinds: [kind(petal, ['#ffc2d4', '#ffb3c6', '#fff0f5'], 5, 'fall', 0.7), kind(flower, ['#ffffff', '#ffd6e0'], 1, 'fall', 0.7), glitter(['#ffffff', '#ffb3c6'])],
    }),
    summer: () => ({
        count: 90,
        duration: 2.5,
        kinds: [glitter([GOLD, '#ffd966', '#ffb703', '#ffffff'], 6), kind(dot, [GOLD, '#ffb703'], 2, 'twinkle', 0.6)],
    }),
    autumn: () => ({
        count: 90,
        duration: 2.5,
        kinds: [kind(leaf, ['#e76f51', '#f4a261', '#bc6c25', '#9c4a1a', '#e9c46a'], 6, 'fall', 0.9), glitter(['#f4a261', '#ffffff'])],
    }),
    winter: () => ({
        count: 90,
        duration: 2.5,
        kinds: [kind(snowflake, ['#ffffff', '#cfe8ff', '#a8d0f0'], 5, 'fall', 0.8), glitter(['#ffffff', '#a8d0f0'])],
    }),
};

/** Days since the epoch (calendar day, DST-proof). @param {Date} d */
function dayNumber(d) {
    return Math.round(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 864e5);
}

/** Easter Sunday (anonymous Gregorian algorithm). @param {number} year */
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

/** The skin of a day: birthdays, then festivals, then the season. @param {Date} date @returns {Skin} */
function skinFor(date) {
    const month = date.getMonth() + 1;
    const day = date.getDate();
    const md = month * 100 + day;

    if (BIRTHDAYS.some(([m, d]) => m === month && d === day)) {
        return SKINS.birthday();
    }

    const easter = dayNumber(easterSunday(date.getFullYear()));
    const today = dayNumber(date);

    if (today >= easter - 2 && today <= easter + 1) {
        return SKINS.easter();
    }

    if (md >= 1020 && md <= 1031) {
        return SKINS.halloween();
    }

    if (month === 12 && day <= 25) {
        return SKINS.christmas(day);
    }

    if (month === 12 && day >= 26) {
        const skin = SKINS.winter();
        skin.kinds.push(kind(star, [GOLD], 1, 'twinkle', 0.6));

        return skin;
    }

    if (md >= 1221 || md < 320) return SKINS.winter();
    if (md < 621) return SKINS.spring();
    if (md < 922) return SKINS.summer();

    return SKINS.autumn();
}

// ---------- the burst ----------

/** @param {string} iso YYYY-MM-DD @returns {Date | null} */
function parseDate(iso) {
    const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);

    return m ? new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3])) : null;
}

/** @param {Date} d */
function isoDate(d) {
    const pad = (/** @type {number} */ n) => String(n).padStart(2, '0');

    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Light mode: once a day per browser. True when this visit may burst. @param {string} today */
function dailyOnce(today) {
    try {
        if (localStorage.getItem('glitter-last') === today) {
            return false;
        }

        localStorage.setItem('glitter-last', today);

        return true;
    } catch {
        return false; // storage blocked: no burst rather than one every time
    }
}

/** @type {Map<string, HTMLCanvasElement>} */
const sprites = new Map();

/** A shape in a colour, drawn once. @param {Kind} k @param {string} color @param {number} dpr */
function spriteOf(k, color, dpr) {
    const key = `${k.draw.name}|${color}`;
    let canvas = sprites.get(key);

    if (!canvas) {
        canvas = document.createElement('canvas');
        const q = dpr * (k.res ?? 1);
        canvas.width = canvas.height = Math.ceil(SPRITE * 2 * q);

        const g = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'));
        g.scale(q, q);
        g.translate(SPRITE, SPRITE);
        k.draw(g, SPRITE * 0.75, color);
        sprites.set(key, canvas);
    }

    return canvas;
}

/** @template T @param {T[]} list @returns {T} */
function pick(list) {
    return list[Math.floor(Math.random() * list.length)];
}

/** @param {Skin} skin @param {boolean} light */
function burst(skin, light) {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const width = window.innerWidth;
    const height = window.innerHeight;
    const scale = (width < 600 ? 0.6 : 1) * (light ? 0.5 : 1);
    const count = Math.round(skin.count * scale);
    const duration = skin.duration * (light ? 0.7 : 1);
    const total = skin.kinds.reduce((sum, k) => sum + k.weight, 0);

    const canvas = document.createElement('canvas');
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    canvas.setAttribute('aria-hidden', 'true');
    Object.assign(canvas.style, { position: 'fixed', inset: '0', width: '100%', height: '100%', pointerEvents: 'none', zIndex: '9999' });
    document.body.append(canvas);

    const g = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'));
    g.scale(dpr, dpr);

    /** One particle: where it starts, how it moves, which sprite. @param {Kind} k */
    const spawn = (k) => {
        const delay = Math.random() * duration * 0.35;

        return {
            k,
            sprite: spriteOf(k, pick(k.colors), dpr),
            delay,
            x: Math.random() * width,
            y: k.motion === 'rise' ? height + 20 : k.motion === 'twinkle' ? Math.random() * height * 0.7 : -20,
            speed: 120 + Math.random() * 140,
            amp: 12 + Math.random() * 28,
            freq: 1.5 + Math.random() * 2.5,
            phase: Math.random() * Math.PI * 2,
            spin: (Math.random() - 0.5) * 4,
            size: (k.size ?? 0.8) * (0.7 + Math.random() * 0.6),
        };
    };

    const particles = [];

    for (let i = 0; i < count; i++) {
        let roll = Math.random() * total;
        const k = skin.kinds.find((kd) => (roll -= kd.weight) < 0) ?? skin.kinds[0];
        particles.push(spawn(k));
    }

    if (skin.door !== undefined) {
        const tile = { draw: door(skin.door), colors: ['#b3262e'], weight: 0, motion: /** @type {Motion} */ ('fall'), size: 2.6, res: 2.6 };
        // big, upright, slow, on top of the others and there from the start
        particles.push({ ...spawn(tile), delay: 0, x: width * (0.3 + Math.random() * 0.4), speed: 100, amp: 0, spin: 0 });
    }

    const start = performance.now();
    const fadeOut = 0.7;

    /** @param {number} now */
    const frame = (now) => {
        const t = (now - start) / 1000;

        if (t >= duration) {
            canvas.remove();

            return;
        }

        g.clearRect(0, 0, width, height);

        const global = t > duration - fadeOut ? (duration - t) / fadeOut : 1;

        for (const p of particles) {
            const life = t - p.delay;

            if (life < 0) continue;

            let x = p.x + Math.sin(life * p.freq + p.phase) * p.amp;
            let y = p.y;
            let rot = p.spin * life;
            let size = p.size;

            switch (p.k.motion) {
                case 'rise':
                    y -= p.speed * 1.1 * life;
                    rot = Math.sin(life * p.freq + p.phase) * 0.25;
                    break;
                case 'flutter':
                    y += p.speed * life;
                    x = p.x + Math.sin(life * p.freq * 2 + p.phase) * p.amp * 2;
                    rot = Math.sin(life * p.freq * 2 + p.phase) * 0.5;
                    size *= 0.7 + 0.3 * Math.abs(Math.sin(life * 14 + p.phase));
                    break;
                case 'twinkle':
                    y += p.speed * 0.25 * life;
                    size *= 0.3 + 0.7 * Math.abs(Math.sin(life * p.freq * 1.5 + p.phase));
                    break;
                default:
                    y += p.speed * life;
            }

            g.globalAlpha = global * Math.min(1, life / 0.15);
            g.setTransform(dpr, 0, 0, dpr, x * dpr, y * dpr);
            g.rotate(rot);

            const d = SPRITE * 2 * size;
            g.drawImage(p.sprite, -d / 2, -d / 2, d, d);
        }

        requestAnimationFrame(frame);
    };

    requestAnimationFrame(frame);
}

function run() {
    const date = parseDate(params.get('glitter-date') ?? '') ?? new Date();
    const forced = params.get('glitter');
    const light = mode === 'light';

    if (document.hidden || document.querySelector('.error')) return;
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    if (light && !forced && !dailyOnce(isoDate(date))) return;

    const skin = forced && forced in SKINS ? SKINS[forced](date.getMonth() === 11 ? date.getDate() : undefined) : skinFor(date);

    burst(skin, light);
}

/** Never delay the page or the form scripts: after load, when idle. */
function schedule() {
    if ('requestIdleCallback' in window) {
        requestIdleCallback(run, { timeout: 1500 });
    } else {
        setTimeout(run, 100);
    }
}

if (document.readyState === 'complete') {
    schedule();
} else {
    addEventListener('load', schedule);
}
