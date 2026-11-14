# User guide

What the app is for and how to do the usual things in it, by goal. For how
the code works, see [tour.md](tour.md); for how to change it,
[recipes.md](recipes.md).

The app has four goals, in this order of importance:

1. **[Manage the card collection](#1-manage-the-card-collection)** (the
   main one): cards, lots, sellers, budget, value, statistics, advent
   calendars, Pokémon. (Menus **Collection** and **Pokémon**.)
2. **[Play competitively](#2-play-competitively)**: decks, the deck builder,
   built decks, formats, tournaments and their stats. (Menus **Decks**, the
   bridge: the builder uses the collection and plans what to buy, and
   **Tournaments**.)
3. **[A public copy](#3-a-public-copy)** to share: read-only static pages,
   no private data. Not a menu: a build (`export-public`).
4. **[The books](#4-the-books)** (the owner's wife's novels). (Menu **Books**.)

The Yu-Gi-Oh! menu is organised the same way:

| Menu | Pages |
|---|---|
| **Collection** | Cards, Lots, Sellers, Budget, Value, Statistics, Advent, Rarities |
| **Decks** (the bridge between the two worlds) | Decks, Built decks, Shopping list (planned) |
| **Tournaments** | Tournaments, Stats, Formats |
| **Pokémon**, **Books** | their own menus |

## 1. Manage the card collection

The home page (`/`) starts there: the cards owned, the collection's value
and its move over 4 weeks, this month's budget left (a link to `/budget`),
the last lots.

### Cards
`/cards`: every copy owned, as pictures or a table (`?view=table`), with
filters (set, rarity, condition, in a deck, sold…). `/cards/:id` is one copy:
its lot, its price history, the decks using it (each links to the deck's
page, which lists its versions), its picture. **Edit** fixes
the name, rarity, condition, printing picture (the edit page asks Yugipedia
for the gallery) and the Cardmarket product. Cards are never deleted, only
marked sold.

**A card showing the wrong art** (an Extended Art, a French card shown in
English): open its edit page, "Picture". The pictures of its printing come
from Yugipedia (looked up when the card was added; otherwise "Look on
Yugipedia"); click the right one, tick "The same for every copy…" if they
all are, and save. `cargo run -p backend -- find-pictures` does it for the
whole collection (the cards added before, the scans Yugipedia adds later) and
lists the copies with several artworks to choose from.

### Lots and the shipment PDF
A **lot** is one purchase, sale or gift: a seller, a date, a price (minus a
refund), its cards. **Lots → Add**:

- from a Cardmarket order: upload the **shipment PDF**; it is read in
  memory and the form comes back filled (date, seller, price, one line per
  card). Check it, then save. The PDF is kept as the lot's file (private);
- by hand: type the cards, or use the quick add (a set code, then tick the
  cards of the set).

Cards forgotten or arrived later: the lot's page → **Add cards** (the same
rows and quick add, on their own page; the lot page itself only reads).

New cards are linked to ygoprodeck's card data in the background; every
deck that lists them sees them as owned at once. **Sellers** lists who you bought
from, with what you spent.

### Budget
`/budget`: the monthly allowance, what lots cost each month, one-off
contributions (`/budget/manage`), and the charts (spending and shipping per
month with the cards bought, the cards by unit price). The deck builder's *To buy* tab shows what
is left, so you see what a deck would cost before buying.

### Value and statistics
- `/collection/value`: what the collection is worth (Cardmarket prices, with
  ygoprodeck's as a fallback) and the gain over what the lots cost, with the
  chart; the top copies, the biggest movers and the lots are folded sections.
  The value of each deck is on *Built decks*. Prices are fetched weekly.
- `/collection/stats`: the collection counted by rarity, kind, set, language
  and condition, the latest added. No prices paid, so it is also in the public
  copy (which also keeps there the most valuable cards and the movers).

### Pack openings
- **Yu-Gi-Oh! advent calendar** (*Advent*, behind the login): a gift lot
  holds the cards, each day shows one.
  1. *Advent → Add calendar* → the year.
  2. *Create its lot* (top of the edit page), add the cards in the lot page.
  3. Back on the edit page, for each day its card (suggested from the lot, or
     its code `MAMO-FR023`, or `#id`) and its photo; ↺ ↻ if a photo is
     sideways. Days can be filled one at a time through December.

  Photos go into `media/advent_calendar_photos/<year>/` and into git with the
  data.
- **Pokémon boosters**: see below.

### Pokémon
`/pokemon` (header **Pokémon**): mostly booster openings, French, English
and Japanese, with a wishlist and Cardmarket prices (weekly).

- **A booster opening** (*Add → A booster opening*): the set code printed at
  the bottom of the cards, next to the number (**FNE** 067/064 on a French
  card, SFA on an English one, the set id like SV6a on a Japanese one), the
  language, the date, then the numbers: `12 34 45r 67x2` (`r` reverse holo,
  `h` holo, `x2` two copies; a card numbered by a letter: `B`). 30th
  Celebration (**30C**) shares its code with its Classic Collection: type the
  set size after the number (`1/30`) or pick the card on the next step. A
  Trainer Gallery card prints its main set's code: `ASR TG29`. A Japanese
  card: its set id and number, rarity letters fine (`SV3a 071/062 AR`). A
  Sword & Shield card prints no set code: type `F 034/189` (the set is found
  by its size and regulation mark). Then **Find the cards**: check each
  picture, variant, condition; a count of 0 leaves a card out. **Save**.
- **The openings** list each booster with its cards and value; forgotten
  cards: the opening's page → *Add cards by number*; wrong language: *Date,
  language, note*.
- **One card** (*Add → One card*: a single, a gift, a graded card): find it by
  printed code (`FNE 067`) or name, pick it, choose its variant. The language select has "Any language" (guess it from the code). The code tells its language (`FNE` French, `SFA` English, `SV6a` Japanese): the form switches to it by itself.
- **The wishlist**: *Wishlist → Add a wish*; its row turns green when the
  trend is at or under your price. *I got it* opens the add form pre-filled.
  Above the wishes, *Boosters to open*: the 3 boosters with the most wished
  cards, and the 3 cheapest with at least one (Cardmarket's price, every
  language together, weekly; a Trainer Gallery card counts for its main
  set's booster, a promo for none). Also in the public copy.
- A card sold or given away: its page → *Edit or mark sold*. Never deleted.

Pictures are a cache (`media/pokemon_card_images/`), downloaded again when
missing. A copy's page shows the card's text (HP, attacks, abilities,
effect…) from TCGdex's data, also in the public copy.

### From a phone
On the home Wi-Fi open **`http://192.168.1.202:3000`** and log in with your
own password; bookmark it or "Add to home screen". From anywhere, with the
Tailscale app logged in to our account: the app's HTTPS address
`https://lahochenas3.<name>.ts.net` (better: HTTPS, so the live book scan
works). The NAS is on from 7:00 to 22:00.

**Live book scan at the plain-HTTP address** (Chrome, once per phone): open
`chrome://flags`, search "Insecure origins treated as secure", enable it,
type `http://192.168.1.202:3000` in its box, "Relaunch". Without it, "Scan"
opens the camera app (a phone short of memory may close Chrome meanwhile).

**If the page doesn't load** on one phone: it must be on the home Wi-Fi (not
guest, not mobile data); type `http://` in full (browsers set to "HTTPS only"
try `https://`); no VPN, ad-blocker or private-DNS app; try another browser
and `/login` directly. Port 3000 is plain HTTP: fine at home; never forward
it on the box.

To try a development build on a phone, see the recipes' "The development
server from a phone".

## 2. Play competitively

The path: **deck → versions → deck builder → built deck → shopping list →
format → tournament → list played → stats.**

### Decks and versions
`/decks` lists the decks; `/decks/:id` is the deck's hub: its versions, its
history, its tournaments. A **version** is one list of 40-60 cards plus extra
and side deck. `/deck-versions/:id` is a version's **read-only** page: the
cards as a grid with ban badges, the format or ban list it plays, opening
hands (5 and 6 cards), a link to the tournament it was played in (the
rounds are on the tournament's page; the format's problems in the builder).
Its *Open in the deck builder* button leads to
the editor. New version: from the deck's hub (empty, a copy of another
version, or a YDKE).

### The deck builder
`/deck-builder/:id` does **all the editing**. It is where the two worlds meet:
it uses the collection (the copies you own, the ones built decks hold)
and plans competitive play (the next tournament, what to buy). Five tabs
(kept in the address: `#build`, `#to-buy`, `#draw`, `#history`,
`#settings`):

| Tab | For |
|---|---|
| **Build** | the zones (main, extra, side), drag and drop, the collection search, categories, the format banner. **Save** is sticky in the header |
| **To buy** | the missing cards (not owned, or owned but held by another built deck) with Cardmarket link and price, the cost, errata warnings, "Take free copies" (a built deck), this version's Cardmarket wants list |
| **Test draw** | Monte Carlo opening hands on the zones as they are now |
| **History** | the deck's saved lists, what changed, *Name this save*, *Rename* (the only place lists are named) |
| **Settings** | three blocks: **Rules** (ban list or format), **Physical deck** (built or planned, deck box, sleeves), **Sharing** (public) |

The header carries the owned/missing counts, this month's budget left, a
*Played at …* link when a list of the deck was played, and a **⋯** menu:
Rename, Export / Play (copy a YDKE, download a `.ydk`, DuelingBook,
YGOPRODeck), *+ Tournament with this version*, Delete.

Two paths through the builder:

A version is a **list**: cards and counts. Whether you own them is worked
out from the collection; only a **built** version holds copies.

**"What do I own for this deck?"**
1. Search (*Add cards*): your cards, and by name the other cards the app
   knows, each with how many you own and how many are free. Drop one in a
   zone as many times as you play it. A card nobody has seen yet:
   *+ Card by name* (ygoprodeck looks it up).
2. The tiles you can't cover with your copies are **missing** (red). Two
   plain lists may count on the same copies.
3. *Settings → Physical deck → Built* **builds** the deck: it takes your free
   copies (best condition first), out of the collection for every other
   deck; what you lack stays missing. The other decks lose nothing they
   hold (only built decks hold copies).
4. A card bought later is free, not in the built deck yet: *Take free
   copies* (To buy tab, or the selected card) puts it in. Saving the built
   deck does it too.

**"Prepare the next tournament"**
1. *Settings → Rules*: pick the **format** (Edison…) or a ban list; the
   builder warns about forbidden cards and out-of-format ones.
2. Test the hands in *Test draw*.
3. *Settings → Physical deck → Planned*: the deck goes on the **shopping
   list** (`/decks/shopping`) with the built ones. *To buy*: its missing
   cards and their cost, against the budget left; copy the Cardmarket wants
   list to place the order; the cards you buy are owned at once.
4. *History* keeps every save; *Name this save* marks the list you will play.
5. After the event, *+ Tournament with this version* (see below).

### A deck's history

Every **Save** in the deck builder keeps the list in the deck's history
(several saves within 30 minutes count as one); nothing to do by hand.

1. **Before an event, or a list worth keeping**: builder → **History** tab →
   **Name this save** (save the deck first), or **Rename** on any list of
   the table (blank: an automatic one again). Named lists are never replaced.
2. **A tournament**: choose the version in the tournament form; its list is
   saved with the tournament, so editing the deck afterwards doesn't change
   what was played. Editing the tournament keeps that list unless you
   choose a version again.
3. **What changed**: the History tab compares a saved list with the zones on
   the screen (saved or not); a list's page (`/deck-snapshots/:id`, from the
   deck page's History) compares it with the previous one, another list, or
   a version now.
4. **Go back to an old list**: its page → **New version from this list**
   (its cards and counts; the collection says what you own). Old versions you no longer
   play can be deleted: their lists stay in the history.

### Built decks
`/decks/built`: the decks you physically built. One card per deck (box,
sleeves, its cards and the copies it holds, how many are missing), then the
value of each deck.

### Shopping list
`/decks/shopping` (Decks menu): what the built decks, then the planned ones,
still lack. They share the collection: a copy you own once counts for one
deck only, built decks first. One card per deck (a folded list of what is
missing with its price, errata warnings and a Buy link), then the Cardmarket
wants list of all of it with an **estimated price**, and this month's budget
left. Untick a card, or a whole deck, and the list and the estimate follow
(copy it from there). A version's own list is in the builder's *To buy* tab.

### Formats
`/formats`: the rules a deck is played under (a Konami ban list copied, with
a cutoff date, e.g. Edison). `/formats/:id` reads its rules and list;
*Edit* (`/formats/:id/edit`) changes its name, cutoff, notes and list, makes
a *new version*, deletes it while unused. A version plays a ban list **or** a format, never both.

**Play a format (Edison…)**

1. **Tournaments → Formats → Add a format**: its name, its cutoff (a date, or
   the set it ends with: the date comes from ygoprodeck's list of sets),
   the Konami list it starts from (its cards are copied).
2. On the format's page: add the cards of its own list (English name, as
   on ygoprodeck), change a status, take one off. Check the cutoff: the
   sets' dates are ygoprodeck's.
3. In the deck builder, **Ban list or format** → the format, **Save**: the
   problems show under it (too many copies, cards released after the
   cutoff: greyed with "Out" on their tiles), and the card search can keep only
   the legal cards. The read-only page of the version shows them too.
4. The format's list changes after decks were built with it (a community
   update): **New version** on its page, then edit the copy. The decks
   keep the version they play until you switch them.


### Tournaments
- `/tournaments`: the table of tournaments with record and placement; a
  filter by date and deck. `/tournaments/add` (`?version=` pre-selects a
  version), in parts: the event, the list played, the rounds (opponent deck
  labels, result, and games: won, lost, draw, unfinished), the conclusion;
  *Save* stays at the bottom of the screen, and on the edit page the games of
  the rounds already noted are folded.
- **The list played** is a snapshot of the deck's history, frozen when the
  tournament is saved. It shows on the tournament page and on the deck.
- `/tournaments/stats` (`?since=`, `?deck=`): win rates by version, your
  deck's labels, matchups, die, first/second. *Labels* (`/tournaments/labels`)
  renames or merges archetype names.

## 3. A public copy

Every morning the NAS writes a **read-only static site** and uploads it to a
web hosting. It contains: the Yu-Gi-Oh! and Pokémon cards, the statistics,
the market values, the public decks (those a version of which is marked
public, or played in a tournament), the tournaments and their stats (no old
notes), the Pokémon wishlist and the books (except private ones) with
their covers. The pictures are small (360 px wide): the cards' text is on
their pages.

It **never** contains: sellers, lots, shipment PDFs, prices paid, the budget,
notes, ISBNs, tournaments' old notes, forms, private decks and books.

A version is made public in the builder (*Settings → Sharing*); a built
version is public by default (building it turns the switch on); a tournament
makes its list public. A book is kept out with the 🔒 switch of its form.
Generate it by hand with `cargo run -p backend -- export-public`; the
publishing (SFTP) is in [deploy/README.md](../deploy/README.md).

## 4. The books

Menu **Books** (`/books`). The book collection is the owner's wife's novels.

- **Add** (`/books/add`): type the ISBN (13 digits, hyphens fine), or
  **📷 Scan** on a phone (photo close, flat, well lit), or a USB scanner.
  **Look up** fills the title, authors, publisher, year, pages, language,
  often the series (check it). "Already in the list" = this edition exists;
  another edition has its own ISBN. Fix, **Add the book**: the empty form
  comes back. The lookup asks Open Library, Google Books (needs
  `GOOGLE_BOOKS_API_KEY`, stops at 800 a day) and the BnF (first for French
  ISBNs).
- **Read or not**: the *Read* switch in the lists or on the book's page.
  **🎲 Random** picks a book to read (unread, can start now; a language or
  format if you like).
- A book without an ISBN: fill by hand. A series page lists the missing
  volumes, each with a link to add it. A book given away: its edit page →
  *Delete this book* (refused while behind an advent door).
- **Book advent calendar**: *Books → Advent → Add calendar*, then each day's
  book (ISBN, looked up and added if new, or `#id`) and photo. Photos go into
  `media/book_advent_calendar_photos/<year>/`.
- 🔒 keeps a book off the public copy.
