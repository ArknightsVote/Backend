use std::{collections::HashMap, fs, io::Read as _, sync::Arc};

use dashmap::DashMap;
use futures::StreamExt as _;
use mongodb::bson::doc;
use share::models::{database::VotingTopic, excel::CharacterData};

use crate::AppError;

const CHARACTER_TABLE_FILE: &str = "character_table.json";

pub fn load_character_table() -> Result<HashMap<String, CharacterData>, AppError> {
    if fs::metadata(CHARACTER_TABLE_FILE).is_err() {
        tracing::error!("Missing character_table.json file");
        return Err(AppError::MissingCharacterTableJson);
    }

    let mut file = fs::File::open(CHARACTER_TABLE_FILE)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let data: HashMap<String, CharacterData> = serde_json::from_slice(&buf)?;

    Ok(data)
}

pub async fn spawn_pool_updater(
    db: mongodb::Database,
    cache: Arc<DashMap<String, VotingTopic>>,
) -> Result<(), AppError> {
    let collection = db.collection::<VotingTopic>("topics");

    tokio::spawn(async move {
        loop {
            match collection.find(doc! {}).await {
                Ok(mut cursor) => {
                    while let Some(result) = cursor.next().await {
                        match result {
                            Ok(doc) => {
                                cache
                                    .entry(doc.id.clone())
                                    .and_modify(|entry| {
                                        if entry.updated_at != doc.updated_at {
                                            tracing::info!("Updating voting topic: {}", doc.id);
                                            *entry = doc.clone();
                                        }
                                    })
                                    .or_insert_with(|| {
                                        tracing::info!("Inserting new voting topic: {}", doc.id);
                                        doc
                                    });
                            }
                            Err(e) => {
                                tracing::error!("Error reading document from voting_pools: {}", e);
                            }
                        }
                    }

                    tracing::info!("Voting pools cache refreshed.");
                }
                Err(e) => {
                    tracing::error!("Error fetching voting pools: {}", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
        }
    });

    Ok(())
}
