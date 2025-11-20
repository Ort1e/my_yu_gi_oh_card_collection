# my_yu_gi_oh_card_collection
my yu gi oh card collection

# Stack
Django + sqlite3

# Data folder

The data folder has this structure :
```
data/
├── media/
    ├── card_images/
    └── shipment_files/
└── db.sqlite3/
```

# Management

## launch the server

```bash
python manage.py runserver
```

## create a superuser

```bash
python manage.py createsuperuser
```

# database

## update
```bash
python manage.py makemigrations my_ygo_cards
python manage.py migrate
```
## dump
```bash
python ./manage.py dumpdata --exclude auth.permission --exclude contenttypes > db.json
```
## load
```bash
python ./manage.py loaddata db.json
```

# yu gi oh images

here : https://ygoprodeck.com/api-guide/

# NodeJS 

## install dependencies
```bash
npm install
```

## usage

I use NodeJS to generate automatically the js interface to the API. This library is used : https://www.npmjs.com/package/swagger-typescript-api

```bash
npx swagger-typescript-api generate \
  -p http://localhost:8000/my_ygo_cards/api/schema/ \
  -o my_ygo_cards/static/js/deck_builder/api/ \
  -t fetch \
  -n api.js \
  --js
```