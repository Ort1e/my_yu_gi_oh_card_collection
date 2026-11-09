// Built decks page: every `button[data-copy="<textarea id>"]` copies that
// textarea's text (a Cardmarket wants list); the button says "Copied!" for 2 s.

(function () {
    /**
     * Fallback without the clipboard API (plain HTTP): select the text and
     * let the browser copy it.
     * @param {HTMLTextAreaElement} area
     * @returns {boolean}
     */
    function selectAndCopy(area) {
        area.focus();
        area.select();

        try {
            return document.execCommand('copy');
        } catch {
            return false;
        }
    }

    for (const button of document.querySelectorAll('button[data-copy]')) {
        button.addEventListener('click', async () => {
            const area = /** @type {HTMLTextAreaElement | null} */ (
                document.getElementById(button.getAttribute('data-copy') || '')
            );

            if (!area) {
                return;
            }

            let copied = false;

            try {
                await navigator.clipboard.writeText(area.value);
                copied = true;
            } catch {
                copied = selectAndCopy(area);
            }

            if (!copied) {
                area.select();
                return;
            }

            const label = button.textContent;
            button.textContent = 'Copied!';
            setTimeout(() => {
                button.textContent = label;
            }, 2000);
        });
    }

    // The ticked cards rebuild the wants list and its estimate.
    const picks = /** @type {HTMLInputElement[]} */ (
        Array.from(document.querySelectorAll('input.want-pick'))
    );
    const wantsArea = /** @type {HTMLTextAreaElement | null} */ (
        document.getElementById('wants-all')
    );
    const total = document.getElementById('wants-total');
    const unpriced = document.getElementById('wants-unpriced');

    function refreshWants() {
        if (!wantsArea) {
            return;
        }

        /** @type {Map<string, {count: number, price: number | null, label: string}>} */
        const lines = new Map();

        for (const pick of picks) {
            if (!pick.checked) {
                continue;
            }

            const name = pick.dataset.name || '';
            const count = Number(pick.dataset.count);
            const price = pick.dataset.price ? Number(pick.dataset.price) : null;
            const line = lines.get(name) || { count: 0, price, label: pick.dataset.label || name };
            line.count += count;
            lines.set(name, line);
        }

        // sorted like the page, by the name on the card
        const names = [...lines.keys()].sort((a, b) =>
            (lines.get(a)?.label || a).toLowerCase().localeCompare((lines.get(b)?.label || b).toLowerCase()),
        );
        wantsArea.value = names.map((name) => `${lines.get(name)?.count} ${name}`).join('\n');
        wantsArea.rows = names.length + 1;

        let sum = 0;
        let missing = 0;

        for (const line of lines.values()) {
            if (line.price === null) {
                missing += 1;
            } else {
                sum += line.price * line.count;
            }
        }

        if (total) {
            total.textContent = sum.toFixed(2);
        }

        if (unpriced) {
            unpriced.textContent = missing > 0 ? `(${missing} card(s) without a price)` : '';
        }
    }

    for (const pick of picks) {
        pick.addEventListener('change', refreshWants);
    }

    for (const toggle of document.querySelectorAll('input.want-deck')) {
        toggle.addEventListener('change', () => {
            const box = /** @type {HTMLInputElement} */ (toggle);
            const deck = box.closest('.card');

            for (const pick of deck ? deck.querySelectorAll('input.want-pick') : []) {
                /** @type {HTMLInputElement} */ (pick).checked = box.checked;
            }

            refreshWants();
        });
    }
})();
