//! New cards (a lot saved, a card renamed) linked to their card data in the
//! background, then named in the language of their code, then given their
//! printing's picture (Yugipedia, when there is no doubt). The decks need
//! nothing: a list sees a new copy as owned at once (`store::lists`).

use std::collections::BTreeMap;

use crate::store::yugipedia::find_pictures;
use crate::web::error::AppResult;
use crate::AppState;

use super::{get_or_fetch, plan_translations};

/// In the background, so form submissions don't wait on the API: link the
/// given cards to their `card_data` (fetching it if needed), put their names
/// in the language of their code, then look up their printing's picture.
pub fn spawn_link_cards(state: AppState, card_ids: Vec<i64>) {
    if card_ids.is_empty() {
        return;
    }
    tokio::spawn(async move {
        if let Err(e) = link_cards(&state, &card_ids).await {
            tracing::warn!("linking card data: {e:?}");
        }
        if let Err(e) = translate_names(&state, &card_ids).await {
            tracing::warn!("translating card names: {e:?}");
        }

        // last: Yugipedia is slow (a request a second)
        if let Err(e) = find_pictures(&state, Some(&card_ids), false, usize::MAX).await {
            tracing::warn!("finding pictures: {e:?}");
        }
    });
}

/// Put the names of these cards in the language of their code.
async fn translate_names(state: &AppState, card_ids: &[i64]) -> AppResult<()> {
    // network first, then the writes (each one on its own: no transaction
    // held while waiting on ygoprodeck)
    let (translations, untranslated) =
        plan_translations(&state.pool, &state.http, Some(card_ids)).await?;
    for t in &translations {
        sqlx::query("UPDATE card SET name = ? WHERE id = ?")
            .bind(&t.to)
            .bind(t.card_id)
            .execute(&state.pool)
            .await?;
    }
    if !translations.is_empty() || !untranslated.is_empty() {
        // a recent set may have no translation at ygoprodeck yet: clean-cards
        // tries again later
        tracing::info!(
            cards = translations.len(),
            without_translation = untranslated.len(),
            "names put in the language of their code"
        );
    }
    Ok(())
}

async fn link_cards(state: &AppState, card_ids: &[i64]) -> AppResult<()> {
    let mut by_name: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for &id in card_ids {
        let en_name: Option<String> =
            sqlx::query_scalar("SELECT en_name FROM card WHERE id = ? AND card_data_id IS NULL")
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .flatten();
        if let Some(en_name) = en_name {
            by_name.entry(en_name).or_default().push(id);
        }
    }
    for (en_name, ids) in by_name {
        match get_or_fetch(&state.pool, &state.http, &en_name).await? {
            Some(data_id) => {
                for id in ids {
                    sqlx::query("UPDATE card SET card_data_id = ? WHERE id = ?")
                        .bind(data_id)
                        .bind(id)
                        .execute(&state.pool)
                        .await?;
                }
            }
            None => tracing::warn!("no card data found for {en_name:?}"),
        }
    }
    Ok(())
}
