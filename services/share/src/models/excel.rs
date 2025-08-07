use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
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

impl RarityRank {
    pub fn to_numeric(&self) -> i32 {
        match self {
            RarityRank::Tier1 => 1,
            RarityRank::Tier2 => 2,
            RarityRank::Tier3 => 3,
            RarityRank::Tier4 => 4,
            RarityRank::Tier5 => 5,
            RarityRank::Tier6 => 6,
            RarityRank::ENum => 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
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

impl CharacterInfo {
    pub fn matches_rarities(&self, rarities: &[RarityRank]) -> bool {
        rarities.contains(&self.rarity)
    }

    pub fn matches_professions(&self, professions: &[ProfessionCategory]) -> bool {
        professions.contains(&self.profession)
    }

    pub fn matches_sub_professions(&self, sub_professions: &[String]) -> bool {
        sub_professions.contains(&self.sub_profession_id)
    }

    pub fn rarity_in_range(&self, min: Option<RarityRank>, max: Option<RarityRank>) -> bool {
        let rarity_value = self.rarity.to_numeric();

        if let Some(min_rarity) = min
            && rarity_value < min_rarity.to_numeric()
        {
            return false;
        }

        if let Some(max_rarity) = max
            && rarity_value > max_rarity.to_numeric()
        {
            return false;
        }

        true
    }
}
