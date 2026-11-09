# Browsing: what is still mixed

A review (2026-11-07) of pages that do several things, or actions that exist
in several places, against the rule in CLAUDE.md ("one function per page, one
place per action, reads separate from data entry"). Goals and menus:
[../guide.md](../guide.md). Priority: **H** = confuses daily use, **M** =
annoying, **L** = cosmetic.

## Done (2026-11-07)

Menus by goal; the builder's tabs (Build · To buy · Test draw · History ·
Settings) and header; one place per wants list; `/tournaments/stats`; a
shorter `/collection/value` (folded sections, the decks' values on
`/decks/built`); the cards added to a lot on their own page
(`/lots/:id/add-cards`); "New version" only on the deck hub and in the
builder's header; saved lists named only in the builder's History tab
(Rename on each row); the card page lists the decks using the copy, not
their versions; the version page keeps to the list and the opening hands
(the format or ban list named, a link to the tournament). They are described in [../guide.md](../guide.md).

## To do

| # | Overlap | Suggestion | Priority |
|---|---|---|---|
| 2 | **The dashboard is mostly budget**, though the main goal is the collection. | Lead with the collection (cards owned, value and its move, last lots); keep one budget line linking to `/budget`. | M |
| ~~6~~ | **Proxy counts** (skipped: the [deck-lists](deck-lists.md) redesign removes proxies) are shown in 4 places (builder header, To buy, `/decks/built`, the version page), computed by different queries. | One helper that counts them (`store`), used by all; each page shows the number and links to To buy. | M |
| 8 | **`/formats/:id` is half reference, half edit form.** | Read page for the rules (what the format forbids), the edit form on `/formats/:id/edit`, like the other entities. | M |
| 9 | **The tournament form is long** (the tournament, the list played, rounds, games, labels in one form). | Section the form (the event, the list, the rounds) with a sticky "Save"; fold the games of finished rounds. | L |

## Larger: versions as lists

Many of the overlaps above (proxy counts, the wants list, the reserve) come
from every deck version holding real `card` rows and proxies. The redesign is
in [deck-lists.md](deck-lists.md): lists for plain versions, copies only for
built ones, and a shopping list for the cards missing from the built and
planned versions.
