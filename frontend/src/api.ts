// Client of the backend deck builder API (crates/yugioh/src/routes/decks/api/).
// The answers' and inputs' types are generated from its Rust structs
// (api-types/): `satisfies` ties each body to its input type.

import type { ApiCard } from './api-types/ApiCard';
import type { ApiCategory } from './api-types/ApiCategory';
import type { ApiCondition } from './api-types/ApiCondition';
import type { AssignInput } from './api-types/AssignInput';
import type { CardCategory } from './api-types/CardCategory';
import type { CategoryInput } from './api-types/CategoryInput';
import type { Check } from './api-types/Check';
import type { Created } from './api-types/Created';
import type { DeckVersion } from './api-types/DeckVersion';
import type { Deleted } from './api-types/Deleted';
import type { NameInput } from './api-types/NameInput';
import type { ProxyInput } from './api-types/ProxyInput';
import type { ReplacedProxy } from './api-types/ReplacedProxy';
import type { ReserveQuery } from './api-types/ReserveQuery';
import type { SimulationInput } from './api-types/SimulationInput';
import type { SimulationResult } from './api-types/SimulationResult';
import type { VersionUpdate } from './api-types/VersionUpdate';
import type { YdkeInput } from './api-types/YdkeInput';
import type { Zone } from './api-types/Zone';

/** The answer's JSON as `T`; `null` for an empty answer (204). */
async function request<T>(method: string, url: string, body?: unknown): Promise<T> {
    const init: RequestInit = { method };

    if (body !== undefined) {
        init.headers = { 'Content-Type': 'application/json' };
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

    return res.status === 204 ? (null as T) : res.json();
}

export function deckApi(id: number) {
    const base = `/api/deck-versions/${id}`;

    return {
        get: () => request<DeckVersion>('GET', base),
        update: (changes: VersionUpdate) => request<DeckVersion>('PATCH', base, changes),
        remove: () => request<Deleted>('DELETE', base),
        clone: (name: string) => request<Created>('POST', `${base}/clone`, { name } satisfies NameInput),
        addProxy: (name: string, zone: Zone) =>
            request<ApiCard>('POST', `${base}/proxies`, { name, zone } satisfies ProxyInput),
        deleteProxy: (cardId: number) => request<null>('DELETE', `${base}/proxies/${cardId}`),
        /** The owned copy now in place of the proxy, and the versions it stays a proxy in. */
        replaceProxy: (cardId: number) => request<ReplacedProxy>('POST', `${base}/proxies/${cardId}/replace`),
        reserve: (filters: ReserveQuery) => request<ApiCard[]>('POST', `${base}/reserve`, filters),
        /** The saved version's problems in its format. */
        check: () => request<Check>('GET', `${base}/check`),
        categories: () => request<ApiCategory[]>('GET', `${base}/categories`),
        /** `condition`: null for a plain category. */
        addCategory: (name: string, condition: ApiCondition | null = null) =>
            request<ApiCategory>('POST', `${base}/categories`, { name, condition } satisfies CategoryInput),
        deleteCategory: (categoryId: number) => request<null>('DELETE', `${base}/categories/${categoryId}`),
        cardCategories: (cardId: number) => request<CardCategory[]>('GET', `${base}/cards/${cardId}/categories`),
        assignCategory: (cardId: number, categoryId: number, assigned: boolean) =>
            request<null>('PUT', `${base}/cards/${cardId}/categories/${categoryId}`, { assigned } satisfies AssignInput),
        simulate: (handSize: number, simulations: number) =>
            request<SimulationResult>('POST', `${base}/monte-carlo`, {
                hand_size: handSize,
                simulations,
            } satisfies SimulationInput),
    };
}

/** The client of one version, as the components receive it. */
export type DeckApi = ReturnType<typeof deckApi>;

export const importYdke = (deckId: number, ydke: string, name: string | null) =>
    request<Created>('POST', `/api/decks/${deckId}/import-ydke`, { ydke, name } satisfies YdkeInput);
