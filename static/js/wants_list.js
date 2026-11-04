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
})();
