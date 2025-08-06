use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RarityRank {
    #[serde(rename = "TIER_1")]
    Tier1,
    #[serde(rename = "TIER_2")]
    Tier2,
    #[serde(rename = "TIER_3")]
    Tier3,
    #[serde(rename = "TIER_4")]
    Tier4,
    #[serde(rename = "TIER_5")]
    Tier5,
    #[serde(rename = "TIER_6")]
    Tier6,
    #[serde(rename = "E_NUM")]
    ENum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProfessionCategory {
    NONE = 0,
    WARRIOR = 1,
    SNIPER = 2,
    TANK = 4,
    MEDIC = 8,
    SUPPORT = 16,
    CASTER = 32,
    SPECIAL = 64,
    TOKEN = 128,
    TRAP = 256,
    PIONEER = 512,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterData {
    pub name: String,
    pub rarity: RarityRank,
    pub profession: ProfessionCategory,
    pub sub_profession_id: String,
}

#[derive(Debug, Clone)]
pub struct CharacterInfo {
    pub id: i32,
    pub name: String,
    pub rarity: RarityRank,
    pub profession: ProfessionCategory,
    pub sub_profession_id: String,
}
