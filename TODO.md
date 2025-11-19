# TODO


- [ ] add a functionnality to clean the shipment files 
    - [X] add a shipment_file_name field to the lot model (23/07/2025)
- [ ] deploy the site on a server (heroku, pythonanywhere, etc.)

- [ ] add a mean to handle selled items (and adding it to the budget)

## view

- [ ] add delete options
    - [ ] cards
    - [ ] lot (WARN : this will delete all cards in the lot, take care of that)
    - [ ] seller

### Decks



### Lists



### index

### Card view



### Lot view
 
# Done
- [X] ~~Put a ImageField inside card and if none, use the ygopro api to get the image~~ add a property into the card model to get the image url (28/07/2025)
- [X] get image from ygopro api (25/07/2025)
- [X] add a mean to tract budget (25/07/2025)
- [X] add the possibilities to make decks (i.e. cards collection) (11/09/2025)
    - [X] add an import ydke function (11/09/2025)

## view
- [X] add a seller view (23/07/2025)

### Lot view
- [X] add a total card price at the end of the array (23/07/2025)

### Card view
- [X] add price and lot informations if link to unite (25/07/2025)
- [X] add a code search (25/07/2025)
- [X] add a section to show all decks using this card (12/09/2025)

### index

- [X] add  recap of the month (25/07/2025)
- [X] add some global stats (total prices, total port prices, total cards, total lots, etc.) (25/07/2025)

### Lists

- [X] add pagination (25/07/2025)
- [X] add a sorting option (01/09/2025)

### Decks

- [X] moove import ydke button before the title (11/09/2025)
- [X] add a section "tournament" to handle tournament (11/09/2025)