// Copying text. `navigator.clipboard` only exists on a secure page (https,
// localhost): over plain http (the NAS by its address) the browser's older
// copy command on a selected, off-screen textarea is used instead.

/** Copy `text`; false when the browser refused both ways. */
export async function copyText(text: string): Promise<boolean> {
    if (navigator.clipboard) {
        try {
            await navigator.clipboard.writeText(text);

            return true;
        } catch {
            // refused: try the old way
        }
    }

    const area = document.createElement('textarea');

    area.value = text;
    area.readOnly = true;
    area.style.position = 'fixed';
    area.style.left = '-9999px';
    document.body.appendChild(area);
    area.select();

    try {
        return document.execCommand('copy');
    } catch {
        return false;
    } finally {
        area.remove();
    }
}
