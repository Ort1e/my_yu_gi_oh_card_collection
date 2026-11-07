# Browsing: what is still mixed

A review (2026-11-07) of pages that do several things, or actions that exist
in several places, against the rule in CLAUDE.md ("one function per page, one
place per action, reads separate from data entry"). Goals and menus:
[../guide.md](../guide.md). Priority: **H** = confuses daily use, **M** =
annoying, **L** = cosmetic.

## Being done (removed from here when shipped)

1. Menus by goal (Collection · Decks · Tournaments, then Pokémon and Books).
2. The builder's Settings tab (about 8 unrelated parts; Rename, Export and
   Delete each appeared twice): tabs Build · To buy · Test draw · History ·
   Settings, the rest in the header.
3. The Cardmarket wants list was in 3 places: one per version (To buy tab)
   and one for all built decks (`/decks/built`).
4. `/tournaments` was a list and a stats dashboard: stats move to
   `/tournaments/stats`.
5. `/collection/value` was very long and repeated parts of
   `/collection/stats`: folded sections, "by deck" moved.

## To do

| # | Overlap | Suggestion | Priority |
|---|---|---|---|
| 1 | **The version page** (`/deck-versions/:id`) shows the tournament round by round, besides the tournament pages, and repeats panels the builder has (ban list, wants list). | Keep the version page to the list and the opening hands; link to the tournament page ("Played at …") instead of repeating the rounds. | M |
| 2 | **The dashboard is mostly budget**, though the main goal is the collection. | Lead with the collection (cards owned, value and its move, last lots); keep one budget line linking to `/budget`. | M |
| 3 | **The add-cards form sits on the lot page** (reading a lot and entering its cards are one page). | A separate `/lots/:id/add-cards` (the quick add), the lot page read-only with an "Add cards" button. | M |
| 4 | **"New version"** is offered in 3 places (the decks list, the deck hub, the builder). | Keep it on the deck hub and in the builder's header; the list links to the hub. | L |
| 5 | **Naming a deck history save** exists on the snapshot page and in the builder's History tab. | Name in the builder (History); the snapshot page only displays it, with a link back. | L |
| 6 | **Proxy counts** are shown in 4 places (builder header, To buy, `/decks/built`, the version page), computed by different queries. | One helper that counts them (`store`), used by all; each page shows the number and links to To buy. | M |
| 7 | **The card page lists the decks using the copy**, with versions, which the deck pages already show. | Show the decks only (not every version), each linking to the deck hub. | L |
| 8 | **`/formats/:id` is half reference, half edit form.** | Read page for the rules (what the format forbids), the edit form on `/formats/:id/edit`, like the other entities. | M |
| 9 | **The tournament form is long** (the tournament, the list played, rounds, games, labels in one form). | Section the form (the event, the list, the rounds) with a sticky "Save"; fold the games of finished rounds. | L |

## Larger: versions as lists

Many of the overlaps above (proxy counts, the wants list, the reserve) come
from every deck version holding real `card` rows and proxies. The redesign is
in [deck-lists.md](deck-lists.md): lists for plain versions, copies only for
built ones, and a shopping list for the cards missing from the built and
planned versions.
