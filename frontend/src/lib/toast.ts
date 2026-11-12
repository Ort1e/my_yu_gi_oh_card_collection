// Small messages in a corner: "Saved" closes by itself after 3 s, an error
// stays until it is clicked.

import { writable } from 'svelte/store';

export type ToastMessage = {
    id: number;
    text: string;
    error: boolean;
};

export const messages = writable<ToastMessage[]>([]);

let counter = 0;

export function dismiss(id: number): void {
    messages.update((list) => list.filter((m) => m.id !== id));
}

function show(text: string, error: boolean): void {
    const id = ++counter;

    messages.update((list) => [...list, { id, text, error }]);

    if (!error) setTimeout(() => dismiss(id), 3000);
}

export const toast = {
    ok: (text: string) => show(text, false),
    error: (text: string) => show(text, true),
};
