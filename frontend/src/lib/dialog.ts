// Promise helpers on top of the one <dialog> mounted in App.svelte
// (Dialog.svelte): they replace the browser's prompt, confirm and alert.

import { writable } from 'svelte/store';

export type AskOptions = {
    title: string;
    message?: string;
    /** a text field (`true`), or a multi-line one (`'multiline'`) */
    input?: boolean | 'multiline';
    /** the field's starting text */
    default?: string;
    placeholder?: string;
    confirmLabel?: string;
    /** the confirm button looks dangerous (a deletion) */
    danger?: boolean;
};

export type InformOptions = {
    title: string;
    lines: string[];
};

export type Request =
    | { kind: 'ask'; options: AskOptions; resolve: (answer: string | boolean | null) => void }
    | { kind: 'inform'; options: InformOptions; resolve: () => void };

/** The request being shown, null when the dialog is closed. */
export const current = writable<Request | null>(null);

/** Requests wait their turn: a second one never replaces the first. */
const queue: Request[] = [];

let showing = false;

function enqueue(request: Request): void {
    if (showing) {
        queue.push(request);

        return;
    }

    showing = true;
    current.set(request);
}

/** Called by the dialog when the request is answered. */
export function done(): void {
    const following = queue.shift();

    showing = following !== undefined;
    current.set(following ?? null);
}

/**
 * Ask something: the typed text (or `true` without a field) when confirmed,
 * `null` / `false` when cancelled.
 */
export function ask(options: AskOptions): Promise<string | boolean | null> {
    return new Promise((resolve) => enqueue({ kind: 'ask', options, resolve }));
}

/** A yes / no question. */
export async function askYes(options: Omit<AskOptions, 'input' | 'default'>): Promise<boolean> {
    return (await ask(options)) === true;
}

/** A question answered by text: null when cancelled, the (trimmed) text otherwise. */
export async function askText(options: AskOptions): Promise<string | null> {
    const answer = await ask({ ...options, input: options.input ?? true });

    return typeof answer === 'string' ? answer.trim() : null;
}

/** Tell something (a build report): resolves when the dialog is closed. */
export function inform(options: InformOptions): Promise<void> {
    return new Promise((resolve) => enqueue({ kind: 'inform', options, resolve }));
}
