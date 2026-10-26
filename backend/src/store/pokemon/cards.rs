//! TCGdex's cards in `pokemon_card` (only those owned or wished), and the
//! copy and wish writes.
//!
//! A card is asked again after [`RECHECK_DAYS`]. An international card is
//! asked in English (its category, types and rarity are the stored
//! labels) and in French (its French name, set name and picture); a
//! Japanese card in Japanese, then its species' French and English names
//! by its Pokédex number. Saving a card also saves today's prices
//! (`pokemon_price`), so a new card has a value at once.

use chrono::NaiveDate;
use domain::card::CardStatus;
use domain::pokemon::price::{to_json, variant_prices};
use domain::pokemon::wish::{CopyKey, Wish};
use domain::pokemon::{CardInfo, Catalogue, Source, Variant};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::SqliteConnection;

use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use super::{is_fresh, ptcgio, tcgdex};
use crate::store::db::{begin_write, today, Money, Pool};
use crate::web::error::AppResult;

const RECHECK_DAYS: i64 = 30;

/// The `pokemon_card` id of a TCGdex card, fetching it when unknown or old.
/// `None` when TCGdex has no such card. Call it **before** opening a write
/// transaction (it writes the cache through the pool).
pub async fn ensure(
    pool: &Pool,
    http: &reqwest::Client,
    catalogue: Catalogue,
    tcgdex_id: &str,
) -> AppResult<Option<i64>> {
    let tcgdex_id = tcgdex_id.trim();
    let known: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, fetched_at FROM pokemon_card WHERE catalogue = ? AND tcgdex_id = ?",
    )
    .bind(catalogue.code())
    .bind(tcgdex_id)
    .fetch_optional(pool)
    .await?;
    if let Some((id, fetched_at)) = &known {
        if is_fresh(fetched_at, RECHECK_DAYS) {
            return Ok(Some(*id));
        }
    }

    let fetched = match catalogue {
        Catalogue::International => fetch_international(http, tcgdex_id).await,
        Catalogue::Japanese => fetch_japanese(http, tcgdex_id).await,
    };
    let fetched = match fetched {
        Ok(fetched) => fetched,
        // an old copy keeps working while TCGdex is down
        Err(e) if known.is_some() => {
            tracing::warn!("{e}");
            None
        }
        Err(e) => return Err(e),
    };
    let Some(card) = fetched else {
        return Ok(known.map(|(id, _)| id));
    };

    save(pool, catalogue, &card).await.map(Some)
}

/// What is saved of a card.
struct Fetched {
    info: CardInfo,
    name_fr: Option<String>,
    name_en: Option<String>,
    name_ja: Option<String>,
    set_name: String,
    image: Option<String>,
    image_fr: Option<String>,
    json: Value,
}

async fn fetch_international(http: &reqwest::Client, id: &str) -> AppResult<Option<Fetched>> {
    let english = tcgdex::card(http, "en", id).await?;
    let french = tcgdex::card(http, "fr", id).await?;
    let read = |json: &Option<Value>| json.as_ref().and_then(CardInfo::from_tcgdex);
    let (en, fr) = (read(&english), read(&french));

    let Some((info, json)) = en
        .clone()
        .zip(english.clone())
        .or_else(|| fr.clone().zip(french.clone()))
    else {
        return Ok(None);
    };
    Ok(Some(Fetched {
        name_fr: fr.as_ref().map(|c| c.name.clone()),
        name_en: en.as_ref().map(|c| c.name.clone()),
        name_ja: None,
        set_name: fr.as_ref().unwrap_or(&info).set_name.clone(),
        image: en
            .as_ref()
            .and_then(|c| c.image.clone())
            .or_else(|| fr.as_ref().and_then(|c| c.image.clone())),
        image_fr: fr.as_ref().and_then(|c| c.image.clone()),
        info,
        json,
    }))
}

async fn fetch_japanese(http: &reqwest::Client, id: &str) -> AppResult<Option<Fetched>> {
    let Some(json) = tcgdex::card(http, "ja", id).await? else {
        return Ok(None);
    };
    let Some(info) = CardInfo::from_tcgdex(&json) else {
        return Ok(None);
    };

    let (mut name_fr, mut name_en) = (None, None);
    if let Some(dex_id) = info.dex_id {
        name_fr = species_name(http, "fr", dex_id).await?;
        name_en = species_name(http, "en", dex_id).await?;
    }
    Ok(Some(Fetched {
        name_fr,
        name_en,
        name_ja: Some(info.name.clone()),
        set_name: info.set_name.clone(),
        image: info.image.clone(),
        image_fr: None,
        info,
        json,
    }))
}

/// The name of a Pokémon in a language, from its first card there.
async fn species_name(
    http: &reqwest::Client,
    language: &str,
    dex_id: i64,
) -> AppResult<Option<String>> {
    let cards = tcgdex::species_cards(http, language, dex_id).await?;
    Ok(cards
        .first()
        .and_then(|c| c["name"].as_str())
        .map(str::to_string))
}

async fn save(pool: &Pool, catalogue: Catalogue, card: &Fetched) -> AppResult<i64> {
    let info = &card.info;
    let prices = variant_prices(&card.json);
    let types = (!info.types.is_empty()).then(|| info.types.join(","));
    let today = today().to_string();

    let mut tx = begin_write(pool).await?;
    sqlx::query(
        "INSERT INTO pokemon_card (catalogue, tcgdex_id, set_id, set_name, local_id, name_fr,
             name_en, name_ja, rarity, category, types, dex_id, variants, image, image_fr,
             json, fetched_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (catalogue, tcgdex_id) DO UPDATE SET set_id = excluded.set_id,
             set_name = excluded.set_name, local_id = excluded.local_id,
             name_fr = excluded.name_fr, name_en = excluded.name_en,
             name_ja = excluded.name_ja, rarity = excluded.rarity,
             category = excluded.category, types = excluded.types, dex_id = excluded.dex_id,
             variants = excluded.variants,
             -- a picture from elsewhere stays until TCGdex has the card's own
             image = coalesce(excluded.image, pokemon_card.image),
             -- (a picture looked for and not found, '', is looked for again)
             image_from = CASE WHEN excluded.image IS NULL AND pokemon_card.image_from <> ''
                               THEN pokemon_card.image_from END,
             image_fr = excluded.image_fr, json = excluded.json,
             fetched_at = excluded.fetched_at",
    )
    .bind(catalogue.code())
    .bind(&info.tcgdex_id)
    .bind(&info.set_id)
    .bind(&card.set_name)
    .bind(&info.local_id)
    .bind(&card.name_fr)
    .bind(&card.name_en)
    .bind(&card.name_ja)
    .bind(&info.rarity)
    .bind(info.category.code())
    .bind(types)
    .bind(info.dex_id)
    .bind(Variant::list_code(&info.variants))
    .bind(&card.image)
    .bind(&card.image_fr)
    .bind(card.json.to_string())
    .bind(&today)
    .execute(&mut *tx)
    .await?;
    let id: i64 =
        sqlx::query_scalar("SELECT id FROM pokemon_card WHERE catalogue = ? AND tcgdex_id = ?")
            .bind(catalogue.code())
            .bind(&info.tcgdex_id)
            .fetch_one(&mut *tx)
            .await?;
    if !prices.is_empty() {
        sqlx::query(
            "INSERT INTO pokemon_price (card_id, date, prices) VALUES (?, ?, ?)
             ON CONFLICT (card_id, date) DO UPDATE SET prices = excluded.prices",
        )
        .bind(id)
        .bind(&today)
        .bind(to_json(&prices))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    tracing::info!(card = %info.tcgdex_id, "Pokémon card data saved");
    Ok(id)
}

/// For a card TCGdex has no picture of (Trainer Galleries, some promos,
/// the Classic Collection reprints): the same print's picture from the
/// Pokémon TCG API (`store::pokemon::ptcgio`), saved in `image` (a full
/// URL) with `image_from = 'pokemontcg:<its id>'`; `''` when it has none
/// either. Never another print's: the picture would be wrong (a Trainer
/// Gallery's artwork is its own). A failure (that service answers 500 now
/// and then) is not saved: the card is tried again an hour later. The
/// picture's URL, if any. Asks the network: not inside a write transaction.
pub async fn find_image(
    pool: &Pool,
    http: &reqwest::Client,
    card_id: i64,
) -> AppResult<Option<String>> {
    static FAILED: Mutex<Vec<(i64, Instant)>> = Mutex::const_new(Vec::new());
    {
        let mut failed = FAILED.lock().await;
        failed.retain(|(_, at)| at.elapsed() < Duration::from_secs(3600));
        if failed.iter().any(|(id, _)| *id == card_id) {
            return Ok(None);
        }
    }

    let (catalogue, json): (String, String) =
        sqlx::query_as("SELECT catalogue, json FROM pokemon_card WHERE id = ?")
            .bind(card_id)
            .fetch_one(pool)
            .await?;
    let found = if catalogue == Catalogue::Japanese.code() {
        // the Pokémon TCG API has no Japanese cards
        Ok(None)
    } else {
        lookup_elsewhere(http, &serde_json::from_str(&json).unwrap_or_default()).await
    };
    let found = match found {
        Ok(found) => found,
        Err(e) => {
            // an outside service down, not the app's error: said so
            tracing::warn!(
                card_id,
                "no picture for now, tried again in an hour: {e} (the Pokémon TCG API often fails)"
            );
            FAILED.lock().await.push((card_id, Instant::now()));
            return Ok(None);
        }
    };

    let (from, image) = match &found {
        Some((id, url)) => (format!("pokemontcg:{id}"), Some(url.clone())),
        None => (String::new(), None),
    };
    let mut tx = begin_write(pool).await?;
    sqlx::query("UPDATE pokemon_card SET image = coalesce(?, image), image_from = ? WHERE id = ?")
        .bind(&image)
        .bind(&from)
        .bind(card_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tracing::info!(card_id, from, "Pokémon picture looked up elsewhere");
    Ok(image)
}

/// The Pokémon TCG API's card for TCGdex's English card JSON.
async fn lookup_elsewhere(
    http: &reqwest::Client,
    json: &Value,
) -> AppResult<Option<(String, String)>> {
    let (Some(name), Some(number), Some(set_id)) = (
        json["name"].as_str(),
        json["localId"].as_str(),
        json["set"]["id"].as_str(),
    ) else {
        return Ok(None);
    };
    // the printed code: TCGdex's set (`ASR:TG` → `ASR`)
    let set = tcgdex::set(http, "en", set_id).await?;
    let code = set
        .as_ref()
        .and_then(|set| set["abbreviation"]["official"].as_str())
        .map(|code| code.split(':').next().unwrap_or(code).to_string());
    let wanted = ptcgio::Wanted {
        name,
        artist: json["illustrator"].as_str(),
        number,
        set_id,
        set_name: json["set"]["name"].as_str().unwrap_or_default(),
        set_code: code.as_deref(),
        official: json["set"]["cardCount"]["official"].as_i64(),
    };
    ptcgio::find(http, &wanted).await
}

/// The cards nobody owns (or ever owned) nor wishes any more, with their
/// prices: the card finder caches every card picked, saved or not.
pub async fn delete_orphans(conn: &mut SqliteConnection) -> sqlx::Result<u64> {
    let done = sqlx::query(
        "DELETE FROM pokemon_card WHERE id NOT IN (SELECT card_id FROM pokemon_copy)
             AND id NOT IN (SELECT card_id FROM pokemon_wish)",
    )
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

// ---------- copies ----------

/// A copy's fields, validated.
#[derive(Debug, Clone)]
pub struct CopyFields {
    pub card_id: i64,
    pub language: String,
    pub variant: Variant,
    pub condition: Option<CardStatus>,
    pub full_art: bool,
    pub source: Source,
    pub obtained_on: Option<NaiveDate>,
    pub price: Option<Decimal>,
    pub grader: Option<String>,
    pub grade: Option<String>,
    pub cert_number: Option<String>,
    pub estimate: Option<Decimal>,
    pub sold_on: Option<NaiveDate>,
    pub sold_price: Option<Decimal>,
    pub note: Option<String>,
    /// The booster opening it came from.
    pub opening_id: Option<i64>,
}

fn date(d: Option<NaiveDate>) -> Option<String> {
    d.map(|d| d.to_string())
}

pub async fn insert_copy(conn: &mut SqliteConnection, copy: &CopyFields) -> sqlx::Result<i64> {
    let done = sqlx::query(
        "INSERT INTO pokemon_copy (card_id, language, variant, condition, full_art, source,
             obtained_on, price, grader, grade, cert_number, estimate, sold_on, sold_price,
             note, opening_id, added_on)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(copy.card_id)
    .bind(&copy.language)
    .bind(copy.variant.code())
    .bind(copy.condition.map(CardStatus::code))
    .bind(copy.full_art)
    .bind(copy.source.code())
    .bind(date(copy.obtained_on))
    .bind(copy.price.map(Money))
    .bind(&copy.grader)
    .bind(&copy.grade)
    .bind(&copy.cert_number)
    .bind(copy.estimate.map(Money))
    .bind(date(copy.sold_on))
    .bind(copy.sold_price.map(Money))
    .bind(&copy.note)
    .bind(copy.opening_id)
    .bind(today().to_string())
    .execute(conn)
    .await?;
    Ok(done.last_insert_rowid())
}

/// `false` when there is no such copy.
pub async fn update_copy(
    conn: &mut SqliteConnection,
    id: i64,
    copy: &CopyFields,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE pokemon_copy SET card_id = ?, language = ?, variant = ?, condition = ?,
             full_art = ?, source = ?, obtained_on = ?, price = ?, grader = ?, grade = ?,
             cert_number = ?, estimate = ?, sold_on = ?, sold_price = ?, note = ?,
             opening_id = ?
         WHERE id = ?",
    )
    .bind(copy.card_id)
    .bind(&copy.language)
    .bind(copy.variant.code())
    .bind(copy.condition.map(CardStatus::code))
    .bind(copy.full_art)
    .bind(copy.source.code())
    .bind(date(copy.obtained_on))
    .bind(copy.price.map(Money))
    .bind(&copy.grader)
    .bind(&copy.grade)
    .bind(&copy.cert_number)
    .bind(copy.estimate.map(Money))
    .bind(date(copy.sold_on))
    .bind(copy.sold_price.map(Money))
    .bind(&copy.note)
    .bind(copy.opening_id)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

// ---------- wishes ----------

#[derive(Debug, Clone)]
pub struct WishFields {
    pub card_id: i64,
    pub language: Option<String>,
    pub variant: Option<Variant>,
    pub max_price: Option<Decimal>,
    pub priority: i64,
    pub note: Option<String>,
}

pub async fn insert_wish(conn: &mut SqliteConnection, wish: &WishFields) -> sqlx::Result<i64> {
    let done = sqlx::query(
        "INSERT INTO pokemon_wish (card_id, language, variant, max_price, priority, note, added_on)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(wish.card_id)
    .bind(&wish.language)
    .bind(wish.variant.map(Variant::code))
    .bind(wish.max_price.map(Money))
    .bind(wish.priority)
    .bind(&wish.note)
    .bind(today().to_string())
    .execute(conn)
    .await?;
    Ok(done.last_insert_rowid())
}

pub async fn update_wish(
    conn: &mut SqliteConnection,
    id: i64,
    wish: &WishFields,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "UPDATE pokemon_wish SET card_id = ?, language = ?, variant = ?, max_price = ?,
             priority = ?, note = ? WHERE id = ?",
    )
    .bind(wish.card_id)
    .bind(&wish.language)
    .bind(wish.variant.map(Variant::code))
    .bind(wish.max_price.map(Money))
    .bind(wish.priority)
    .bind(&wish.note)
    .bind(id)
    .execute(conn)
    .await?;
    Ok(done.rows_affected() == 1)
}

pub async fn delete_wish(conn: &mut SqliteConnection, id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query("DELETE FROM pokemon_wish WHERE id = ?")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// Mark a wish got with this copy (`None`: wanted again).
pub async fn set_wish_got(
    conn: &mut SqliteConnection,
    id: i64,
    copy_id: Option<i64>,
) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE pokemon_wish SET got_copy_id = ? WHERE id = ?")
        .bind(copy_id)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

#[derive(sqlx::FromRow)]
struct OpenWish {
    id: i64,
    card_id: i64,
    language: Option<String>,
    variant: Option<String>,
}

/// The wishes still wanted that these copies fulfil: `(wish id, copy id)`,
/// one copy per wish.
pub async fn wishes_fulfilled(
    conn: &mut SqliteConnection,
    copies: &[(i64, CopyKey)],
) -> sqlx::Result<Vec<(i64, i64)>> {
    let wishes: Vec<OpenWish> = sqlx::query_as(
        "SELECT id, card_id, language, variant FROM pokemon_wish WHERE got_copy_id IS NULL
         ORDER BY priority, id",
    )
    .fetch_all(conn)
    .await?;

    let mut found = Vec::new();
    for wish in wishes {
        let rule = Wish {
            card_id: wish.card_id,
            language: wish.language,
            variant: wish.variant.as_deref().and_then(Variant::from_code),
            max_price: None,
        };
        if let Some((copy_id, _)) = copies.iter().find(|(_, key)| rule.fulfilled_by(key)) {
            found.push((wish.id, *copy_id));
        }
    }
    Ok(found)
}
