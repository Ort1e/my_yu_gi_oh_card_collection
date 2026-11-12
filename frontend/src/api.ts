// Client of the backend deck builder API (crates/yugioh/src/routes/decks/api/).
// The answers' and inputs' types are generated from its Rust structs
// (api-types/): `satisfies` ties each body to its input type.

import type { ApiCard } from './api-types/ApiCard';
import type { ApiChange } from './api-types/ApiChange';
import type { ApiCategory } from './api-types/ApiCategory';
import type { ApiCondition } from './api-types/ApiCondition';
import type { AssignInput } from './api-types/AssignInput';
import type { CardCategory } from './api-types/CardCategory';
import type { CardInput } from './api-types/CardInput';
import type { CategoryInput } from './api-types/CategoryInput';
import type { Check } from './api-types/Check';
import type { CompareInput } from './api-types/CompareInput';
import type { Created } from './api-types/Created';
import type { DeckVersion } from './api-types/DeckVersion';
import type { Deleted } from './api-types/Deleted';
import type { LookupInput } from './api-types/LookupInput';
import type { NameInput } from './api-types/NameInput';
import type { ReserveQuery } from './api-types/ReserveQuery';
import type { SimulationInput } from './api-types/SimulationInput';
import type { SimulationResult } from './api-types/SimulationResult';
import type { Snapshot } from './api-types/Snapshot';
import type { SnapshotNameInput } from './api-types/SnapshotNameInput';
import type { Taken } from './api-types/Taken';
import type { VersionUpdate } from './api-types/VersionUpdate';
import type { YdkeInput } from './api-types/YdkeInput';

/** The answer's JSON as `T`; `null` for an empty answer (204). */
async function request<T>(method: string, url: string, body?: unknown): Promise<T> {
    const init: RequestInit & { headers: Record<string, string> } = { method, headers: {} };

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

    return res.status === 204 ? (null as T) : res.json();
}

export function deckApi(id: number) {
    const base = `/api/deck-versions/${id}`;

    return {
        get: () => request<DeckVersion>('GET', base),
        update: (changes: VersionUpdate) => request<DeckVersion>('PATCH', base, changes),
        remove: () => request<Deleted>('DELETE', base),
        clone: (name: string) => request<Created>('POST', `${base}/clone`, { name } satisfies NameInput),
        /** A tile of the card named `name` ("+ Missing card"); nothing is saved. */
        lookup: (name: string) => request<ApiCard>('POST', `${base}/lookup`, { name } satisfies LookupInput),
        /** A built version's empty slots take the free copies of their cards. */
        takeFree: () => request<Taken>('POST', `${base}/take-free`),
        /** The cards matching the filters, with what the collection holds of each. */
        reserve: (filters: ReserveQuery) => request<ApiCard[]>('POST', `${base}/reserve`, filters),
        /** The saved version's problems in its format. */
        check: () => request<Check>('GET', `${base}/check`),
        categories: () => request<ApiCategory[]>('GET', `${base}/categories`),
        /** `condition`: null for a plain category. */
        addCategory: (name: string, condition: ApiCondition | null = null) =>
            request<ApiCategory>('POST', `${base}/categories`, { name, condition } satisfies CategoryInput),
        deleteCategory: (categoryId: number) => request<null>('DELETE', `${base}/categories/${categoryId}`),
        /** A card's categories: by its card data (all its copies). */
        cardCategories: (cardDataId: number) => request<CardCategory[]>('GET', `${base}/cards/${cardDataId}/categories`),
        assignCategory: (cardDataId: number, categoryId: number, assigned: boolean) =>
            request<null>('PUT', `${base}/cards/${cardDataId}/categories/${categoryId}`, { assigned } satisfies AssignInput),
        /** `main`: the Main Deck's tiles as they are on the screen (saved or not): a card data id each, null for a name-only card. */
        simulate: (handSize: number, simulations: number, main: (number | null)[]) =>
            request<SimulationResult>('POST', `${base}/monte-carlo`, {
                hand_size: handSize,
                simulations,
                main,
            } satisfies SimulationInput),
        /** The deck's saved lists, newest first. */
        history: () => request<Snapshot[]>('GET', `${base}/history`),
        /** What changed from saved list `snapshotId` to these zones (their tiles, saved or not). */
        compare: (snapshotId: number, main: CardInput[], extra: CardInput[], side: CardInput[]) =>
            request<ApiChange[]>('POST', `${base}/compare`, {
                snapshot_id: snapshotId,
                main,
                extra,
                side,
            } satisfies CompareInput),
        /** Name saved list `snapshotId` of the deck (`null`: an automatic one again). */
        nameSnapshot: (snapshotId: number, name: string | null) =>
            request<null>('POST', `${base}/history/${snapshotId}/name`, { name } satisfies SnapshotNameInput),
    };
}

/** The client of one version, as the components receive it. */
export type DeckApi = ReturnType<typeof deckApi>;

export const importYdke = (deckId: number, ydke: string, name: string | null) =>
    request<Created>('POST', `/api/decks/${deckId}/import-ydke`, { ydke, name } satisfies YdkeInput);
