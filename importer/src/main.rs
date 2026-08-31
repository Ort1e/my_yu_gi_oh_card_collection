//! One-off importer: reads a Django `dumpdata` JSON export and inserts it
//! into the sqlx-managed SQLite schema (migrations/0002_ygo_schema.sql).
//!
//! Usage:
//!   DATABASE_URL=sqlite:app.db cargo run -p importer -- data.json
//!
//! Safe to re-run against an empty database; NOT idempotent against a
//! database that already has data (primary keys are taken verbatim from
//! the Django export, so re-running will hit UNIQUE constraint errors).

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Deserialize)]
struct Record {
    model: String,
    pk: i64,
    fields: Value,
}

type Tx<'a> = Transaction<'a, Sqlite>;

#[tokio::main]
async fn main() -> Result<()> {
    let json_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "data.json".to_string());
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:app.db".to_string());

    let raw =
        std::fs::read_to_string(&json_path).with_context(|| format!("reading {json_path}"))?;
    let records: Vec<Record> = serde_json::from_str(&raw).context("parsing dumpdata JSON")?;
    println!("loaded {} records from {json_path}", records.len());

    let pool = SqlitePoolOptions::new()
        .connect(&db_url)
        .await
        .with_context(|| format!("connecting to {db_url}"))?;

    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .context("running migrations")?;

    import(&pool, &records).await?;

    println!("import complete");
    Ok(())
}

async fn import(pool: &SqlitePool, records: &[Record]) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Delays FK constraint checking to COMMIT time, for this transaction
    // only. This means insertion order across tables doesn't matter —
    // important here because a few relationships are self-referential
    // (card_category -> card_category) or could appear in either order
    // in the export (e.g. deck_version before/after the cards it lists).
    sqlx::query("PRAGMA defer_foreign_keys = ON")
        .execute(&mut *tx)
        .await?;

    // First pass: everything except the "conditional category" subset,
    // which UPDATEs a row that must already exist (see below).
    for r in records {
        match r.model.as_str() {
            "my_ygo_cards.carddata" => insert_card_data(&mut tx, r).await?,
            "my_ygo_cards.card" => insert_card(&mut tx, r).await?,
            "my_ygo_cards.tournament" => insert_tournament(&mut tx, r).await?,
            "my_ygo_cards.deck" => insert_deck(&mut tx, r).await?,
            "my_ygo_cards.advancedbanlist" => insert_advanced_ban_list(&mut tx, r).await?,
            "my_ygo_cards.deckversion" => insert_deck_version(&mut tx, r).await?,
            "my_ygo_cards.sellersource" => insert_seller_source(&mut tx, r).await?,
            "my_ygo_cards.seller" => insert_seller(&mut tx, r).await?,
            "my_ygo_cards.lot" => insert_lot(&mut tx, r).await?,
            "my_ygo_cards.unite" => insert_unite(&mut tx, r).await?,
            "my_ygo_cards.monthlybudget" => insert_monthly_budget(&mut tx, r).await?,
            "my_ygo_cards.apport" => insert_apport(&mut tx, r).await?,
            "my_ygo_cards.cardcategory" => insert_card_category(&mut tx, r).await?,
            "my_ygo_cards.cardcategoryassignment" => {
                insert_card_category_assignment(&mut tx, r).await?
            }
            "my_ygo_cards.banlistentry" => insert_ban_list_entry(&mut tx, r).await?,
            "my_ygo_cards.adventcalendar" => insert_advent_calendar(&mut tx, r).await?,
            // handled in the second pass, after all card_category rows exist
            "my_ygo_cards.cardconditionalcategory" => {}
            other => eprintln!("skipping unknown model: {other}"),
        }
    }

    // Second pass: apply the conditional-category fields on top of the
    // base card_category rows inserted above (MTI child -> parent).
    for r in records {
        if r.model == "my_ygo_cards.cardconditionalcategory" {
            apply_card_conditional_category(&mut tx, r).await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

// ---- JSON extraction helpers ----

fn get_str(fields: &Value, key: &str) -> Option<String> {
    fields.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn req_str(fields: &Value, key: &str) -> String {
    get_str(fields, key).unwrap_or_default()
}

fn get_i64(fields: &Value, key: &str) -> Option<i64> {
    fields.get(key).and_then(|v| v.as_i64())
}

fn get_bool(fields: &Value, key: &str) -> bool {
    fields.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn get_i64_array(fields: &Value, key: &str) -> Vec<i64> {
    fields
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default()
}

// ---- per-table inserts ----

async fn insert_card_data(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    // json_data is a nested object in the export; store it back out as text
    let json_data = f.get("json_data").cloned().unwrap_or(Value::Null);
    sqlx::query(
        "INSERT INTO card_data (id, en_name, ygopro_id, card_type, json_data)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(req_str(f, "en_name"))
    .bind(get_i64(f, "ygopro_id"))
    .bind(req_str(f, "card_type"))
    .bind(json_data.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_card(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query(
        "INSERT INTO card (id, name, en_name, card_data_id, code, last_known_status, is_proxy)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(req_str(f, "name"))
    .bind(req_str(f, "en_name"))
    .bind(get_i64(f, "card_data"))
    .bind(get_str(f, "code"))
    .bind(get_str(f, "last_known_status"))
    .bind(get_bool(f, "is_proxy"))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_tournament(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO tournament (id, name, date, location, notes) VALUES (?, ?, ?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "name"))
        .bind(req_str(f, "date"))
        .bind(get_str(f, "location"))
        .bind(get_str(f, "notes"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_deck(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO deck (id, name, description) VALUES (?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "name"))
        .bind(get_str(f, "description"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_advanced_ban_list(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO advanced_ban_list (id, date) VALUES (?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "date"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_deck_version(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query(
        "INSERT INTO deck_version (id, deck_id, version_name, tournament_id, ban_list_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(get_i64(f, "deck"))
    .bind(req_str(f, "version_name"))
    .bind(get_i64(f, "tournament"))
    .bind(get_i64(f, "ban_list"))
    .execute(&mut **tx)
    .await?;

    for card_id in get_i64_array(f, "main_deck") {
        sqlx::query("INSERT INTO deck_version_main_deck (deck_version_id, card_id) VALUES (?, ?)")
            .bind(r.pk)
            .bind(card_id)
            .execute(&mut **tx)
            .await?;
    }
    for card_id in get_i64_array(f, "extra_deck") {
        sqlx::query("INSERT INTO deck_version_extra_deck (deck_version_id, card_id) VALUES (?, ?)")
            .bind(r.pk)
            .bind(card_id)
            .execute(&mut **tx)
            .await?;
    }
    for card_id in get_i64_array(f, "side_deck") {
        sqlx::query("INSERT INTO deck_version_side_deck (deck_version_id, card_id) VALUES (?, ?)")
            .bind(r.pk)
            .bind(card_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn insert_seller_source(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO seller_source (id, name, url) VALUES (?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "name"))
        .bind(get_str(f, "url"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_seller(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO seller (id, name, is_person, source_id) VALUES (?, ?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "name"))
        .bind(get_bool(f, "is_person"))
        .bind(get_i64(f, "source"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_lot(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query(
        "INSERT INTO lot
            (id, lot_type, price, buy_date, received_date, seller_id, is_cancelled, no_card_price, shipment_file)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(req_str(f, "lot_type"))
    .bind(req_str(f, "price"))
    .bind(get_str(f, "buy_date"))
    .bind(get_str(f, "received_date"))
    .bind(get_i64(f, "seller"))
    .bind(get_bool(f, "is_cancelled"))
    .bind(req_str(f, "no_card_price"))
    .bind(get_str(f, "shipment_file"))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_unite(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO unite (id, price, lot_id, card_id) VALUES (?, ?, ?, ?)")
        .bind(r.pk)
        .bind(get_str(f, "price"))
        .bind(get_i64(f, "lot"))
        .bind(get_i64(f, "card"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_monthly_budget(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO monthly_budget (id, month, amount) VALUES (?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "month"))
        .bind(req_str(f, "amount"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_apport(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query(
        "INSERT INTO apport (id, budget_id, description, amount, date) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(get_i64(f, "budget"))
    .bind(get_str(f, "description"))
    .bind(req_str(f, "amount"))
    .bind(req_str(f, "date"))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_card_category(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO card_category (id, name, deck_version_id) VALUES (?, ?, ?)")
        .bind(r.pk)
        .bind(req_str(f, "name"))
        .bind(get_i64(f, "deck_version"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// CardConditionalCategory is Django MTI: its pk (`cardcategory_ptr`)
/// equals the pk of the base CardCategory row inserted above, so this
/// is an UPDATE, not an INSERT — must run after all card_category rows
/// exist (see the two-pass loop in `import`).
async fn apply_card_conditional_category(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    let ptr_id = get_i64(f, "cardcategory_ptr").unwrap_or(r.pk);

    sqlx::query(
        "UPDATE card_category
         SET condition_description = ?, categorie_true_id = ?, categorie_false_id = ?
         WHERE id = ?",
    )
    .bind(get_str(f, "condition_description"))
    .bind(get_i64(f, "categorie_true"))
    .bind(get_i64(f, "categorie_false"))
    .bind(ptr_id)
    .execute(&mut **tx)
    .await?;

    for other_id in get_i64_array(f, "categories_or_conditions") {
        sqlx::query(
            "INSERT INTO card_category_condition (category_id, condition_category_id) VALUES (?, ?)",
        )
        .bind(ptr_id)
        .bind(other_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn insert_card_category_assignment(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO card_category_assignment (id, category_id, card_id) VALUES (?, ?, ?)")
        .bind(r.pk)
        .bind(get_i64(f, "category"))
        .bind(get_i64(f, "card"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_ban_list_entry(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query(
        "INSERT INTO ban_list_entry (id, ban_list_id, card_data_id, status) VALUES (?, ?, ?, ?)",
    )
    .bind(r.pk)
    .bind(get_i64(f, "ban_list"))
    .bind(get_i64(f, "card_data"))
    .bind(req_str(f, "status"))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_advent_calendar(tx: &mut Tx<'_>, r: &Record) -> Result<()> {
    let f = &r.fields;
    sqlx::query("INSERT INTO advent_calendar (id, year) VALUES (?, ?)")
        .bind(r.pk)
        .bind(get_i64(f, "year"))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
