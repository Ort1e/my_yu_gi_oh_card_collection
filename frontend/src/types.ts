// The deck builder's own types. The API's JSON is typed in api-types/,
// generated from the Rust structs (ts-rs, `cargo test`): never edit those.

import type { ApiCard } from './api-types/ApiCard';
import type { Zone } from './api-types/Zone';

/**
 * One copy of a card on the screen: the API's tile with an id of the
 * builder's own (`deck.ts`, `tiles`), for selection and drag and drop. A line
 * of 3 is 3 tiles; they are grouped back by card when saved.
 */
export type Tile = ApiCard & { id: number };

/** Where a card can be: a deck zone, or the reserve's search results. */
export type Place = Zone | 'reserve';

/** Cards by zone, the reserve included. */
export type Zones = Record<Place, Tile[]>;

/** The deck zones only (YDKE, the deck's price). */
export type DeckZones = Record<Zone, Tile[]>;

/**
 * What a version plays: `key` its ban list's id (`"12"`) or its format's
 * (`"f3"`), `cutoff` the format's (`"2010-03-01"`, null for a ban list).
 */
export type Rules = {
    key: string;
    cutoff: string | null;
};

/** A card's status in the rules played. */
export type BanStatus = 'Banned' | 'Limited' | 'Semi-Limited';

/** What a dragged card carries (`application/json`). */
export type DragData = {
    cardId: number;
    from: Place;
};

/** A card dropped somewhere: in a zone, or outside them all. */
export type Move = DragData & {
    to: Place | 'outside';
};
