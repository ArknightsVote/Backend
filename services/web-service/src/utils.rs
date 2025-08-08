use std::{collections::HashMap, fs, io::Read as _};

use serde::{Deserialize, Serialize};
use share::models::{api::CharacterPortrait, excel::CharacterData};

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

#[derive(Debug, Deserialize, Serialize)]
struct TorappuApiFileData {
    name: String,
    path: String,
    size: i32,
    create_at: String,
    modified_at: String,
    is_dir: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct TorappuApiFileStruct {
    dir: TorappuApiFileData,
    children: Vec<TorappuApiFileData>,
}

pub async fn fetch_portrait_image_url() -> Result<HashMap<i32, CharacterPortrait>, AppError> {
    const PORTRAIT_IMAGE_STORE_URL: &str =
        "https://torappu.prts.wiki/api/v1/files/raw%2Fchar_portrait";

    let response = reqwest::get(PORTRAIT_IMAGE_STORE_URL)
        .await?
        .json::<TorappuApiFileStruct>()
        .await?;

    let mut table = HashMap::new();
    let character_table = load_character_table()?;

    for data in response.children.iter() {
        if data.is_dir {
            continue;
        }

        let name = data.name.trim_end_matches(".png").to_string();
        let stripped_name = name.split('_').take(3).collect::<Vec<_>>().join("_");

        let char_id = stripped_name
            .split('_')
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1);

        let (name, id) = match stripped_name.as_str() {
            "char_1001_amiya2" => ("阿米娅-近卫".to_string(), char_id),
            "char_1037_amiya3" => ("阿米娅-医疗".to_string(), char_id),
            "char_4195_raidian" => ("Raidian".to_string(), 614),
            _ => {
                if let Some(character_data) = character_table.get(&stripped_name) {
                    (character_data.name.to_string(), char_id)
                } else {
                    tracing::warn!("Character {} not found in character table", stripped_name);
                    println!("Character {} not found in character table", stripped_name);
                    continue;
                }
            }
        };

        let avatar_url = format!(
            "https://torappu.prts.wiki/assets/char_portrait/{}",
            data.name
        );

        table
            .entry(id)
            .and_modify(|entry: &mut CharacterPortrait| {
                if !entry.avatar.contains(&avatar_url) {
                    entry.avatar.push(avatar_url.clone());
                }
            })
            .or_insert_with(|| CharacterPortrait {
                id,
                name: stripped_name.clone(),
                cn_name: name,
                avatar: vec![avatar_url],
            });
    }

    Ok(table)
}
