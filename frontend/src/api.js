// Client of the backend deck builder API (backend/src/routes/deck_api/).

async function request(method, url, body) {
    const init = { method, headers: {} };

    if (body !== undefined) {
        init.headers['Content-Type'] = 'application/json';
        init.body = JSON.stringify(body);
    }

    const res = await fetch(url, init);

    if (!res.ok) {
        let message = `${res.status} ${res.statusText}`;

        try {
            const json = await res.json();

            if (json.error) message = json.error;
        } catch {
            // not JSON
        }

        throw new Error(message);
    }

    return res.status === 204 ? null : res.json();
}

export function deckApi(id) {
    const base = `/api/deck-versions/${id}`;

    return {
        get: () => request('GET', base),
        update: (changes) => request('PATCH', base, changes),
        remove: () => request('DELETE', base),
        clone: (name) => request('POST', `${base}/clone`, { name }),
        addProxy: (name, zone) => request('POST', `${base}/proxies`, { name, zone }),
        deleteProxy: (cardId) => request('DELETE', `${base}/proxies/${cardId}`),
        reserve: (filters) => request('POST', `${base}/reserve`, filters),
        categories: () => request('GET', `${base}/categories`),
        // `condition`: null for a plain category, else { description, if_any_of, then, otherwise }
        addCategory: (name, condition = null) => request('POST', `${base}/categories`, { name, condition }),
        deleteCategory: (categoryId) => request('DELETE', `${base}/categories/${categoryId}`),
        cardCategories: (cardId) => request('GET', `${base}/cards/${cardId}/categories`),
        assignCategory: (cardId, categoryId, assigned) =>
            request('PUT', `${base}/cards/${cardId}/categories/${categoryId}`, { assigned }),
        simulate: (handSize, simulations) =>
            request('POST', `${base}/monte-carlo`, { hand_size: handSize, simulations }),
        saveTournament: (tournament) => request('PUT', `${base}/tournament`, tournament),
    };
}

export const importYdke = (deckId, ydke, name) =>
    request('POST', `/api/decks/${deckId}/import-ydke`, { ydke, name });
