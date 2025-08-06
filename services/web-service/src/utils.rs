use std::{collections::HashMap, fs, io::Read as _};

use share::models::excel::CharacterData;

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
