use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::models::excel::{CharacterInfo, ProfessionCategory, RarityRank};

#[derive(Default, Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CandidatePoolPresetFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarities: Option<Vec<RarityRank>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub professions: Option<Vec<ProfessionCategory>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_professions: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_ids: Option<Vec<i32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_ids: Option<Vec<i32>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rarity: Option<RarityRank>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rarity: Option<RarityRank>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "params")]
pub enum CandidatePoolPreset {
    #[serde(rename = "all")]
    All,

    #[serde(rename = "custom")]
    Custom { operator_ids: Vec<i32> },

    #[serde(rename = "by_rarity")]
    ByRarity { rarities: Vec<RarityRank> },

    #[serde(rename = "by_profession")]
    ByProfession {
        professions: Vec<ProfessionCategory>,
    },

    #[serde(rename = "by_sub_profession")]
    BySubProfession { sub_professions: Vec<String> },

    #[serde(rename = "filter")]
    Filter(CandidatePoolPresetFilter),

    #[serde(rename = "union")]
    #[schema(no_recursion)]
    Union { presets: Vec<CandidatePoolPreset> },

    #[serde(rename = "intersection")]
    #[schema(no_recursion)]
    Intersection { presets: Vec<CandidatePoolPreset> },

    #[serde(rename = "difference")]
    #[schema(no_recursion)]
    Difference {
        base: Box<CandidatePoolPreset>,
        exclude: Box<CandidatePoolPreset>,
    },
}

impl CandidatePoolPreset {
    pub fn generate_pool(&self, character_infos: &[CharacterInfo]) -> Vec<i32> {
        use std::collections::HashSet;

        match self {
            Self::All => character_infos.iter().map(|c| c.id).collect(),

            Self::Custom { operator_ids } => {
                let valid_ids: HashSet<i32> = character_infos.iter().map(|c| c.id).collect();
                operator_ids
                    .iter()
                    .filter(|&id| valid_ids.contains(id))
                    .copied()
                    .collect()
            }

            Self::ByRarity { rarities } => character_infos
                .iter()
                .filter(|c| c.matches_rarities(rarities))
                .map(|c| c.id)
                .collect(),

            Self::ByProfession { professions } => character_infos
                .iter()
                .filter(|c| c.matches_professions(professions))
                .map(|c| c.id)
                .collect(),

            Self::BySubProfession { sub_professions } => character_infos
                .iter()
                .filter(|c| c.matches_sub_professions(sub_professions))
                .map(|c| c.id)
                .collect(),

            Self::Filter(CandidatePoolPresetFilter {
                rarities,
                professions,
                sub_professions,
                exclude_ids,
                include_ids,
                min_rarity,
                max_rarity,
            }) => {
                let mut filtered: Vec<i32> = character_infos
                    .iter()
                    .filter(|c| {
                        if let Some(rarities) = rarities
                            && !c.matches_rarities(rarities)
                        {
                            return false;
                        }

                        if let Some(professions) = professions
                            && !c.matches_professions(professions)
                        {
                            return false;
                        }

                        if let Some(sub_professions) = sub_professions
                            && !c.matches_sub_professions(sub_professions)
                        {
                            return false;
                        }

                        if !c.rarity_in_range(*min_rarity, *max_rarity) {
                            return false;
                        }

                        true
                    })
                    .map(|c| c.id)
                    .collect();

                if let Some(exclude_ids) = exclude_ids {
                    let exclude_set: HashSet<i32> = exclude_ids.iter().copied().collect();
                    filtered.retain(|id| !exclude_set.contains(id));
                }

                if let Some(include_ids) = include_ids {
                    let valid_ids: HashSet<i32> = character_infos.iter().map(|c| c.id).collect();
                    let filtered_set: HashSet<i32> = filtered.iter().copied().collect();

                    for &id in include_ids {
                        if valid_ids.contains(&id) && !filtered_set.contains(&id) {
                            filtered.push(id);
                        }
                    }
                }

                filtered
            }

            Self::Union { presets } => {
                let mut result_set = HashSet::new();
                for preset in presets {
                    let pool = preset.generate_pool(character_infos);
                    result_set.extend(pool);
                }
                result_set.into_iter().collect()
            }

            Self::Intersection { presets } => {
                if presets.is_empty() {
                    return Vec::new();
                }

                let mut result_set: HashSet<i32> = presets[0]
                    .generate_pool(character_infos)
                    .into_iter()
                    .collect();

                for preset in &presets[1..] {
                    let pool: HashSet<i32> =
                        preset.generate_pool(character_infos).into_iter().collect();
                    result_set = result_set.intersection(&pool).copied().collect();
                }

                result_set.into_iter().collect()
            }

            Self::Difference { base, exclude } => {
                let base_pool: HashSet<i32> =
                    base.generate_pool(character_infos).into_iter().collect();
                let exclude_pool: HashSet<i32> =
                    exclude.generate_pool(character_infos).into_iter().collect();

                base_pool.difference(&exclude_pool).copied().collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::excel::{ProfessionCategory, RarityRank};

    fn create_test_characters() -> Vec<CharacterInfo> {
        vec![
            CharacterInfo {
                id: 1001,
                name: "AAAA".to_string(),
                rarity: RarityRank::Tier6,
                profession: ProfessionCategory::WARRIOR,
                sub_profession_id: "centurion".to_string(),
            },
            CharacterInfo {
                id: 1002,
                name: "BBBB".to_string(),
                rarity: RarityRank::Tier6,
                profession: ProfessionCategory::WARRIOR,
                sub_profession_id: "sword".to_string(),
            },
            CharacterInfo {
                id: 2001,
                name: "CCCC".to_string(),
                rarity: RarityRank::Tier6,
                profession: ProfessionCategory::CASTER,
                sub_profession_id: "aoedamage".to_string(),
            },
            CharacterInfo {
                id: 3001,
                name: "DDDD".to_string(),
                rarity: RarityRank::Tier6,
                profession: ProfessionCategory::PIONEER,
                sub_profession_id: "pioneer".to_string(),
            },
        ]
    }

    #[test]
    fn test_serialize_deserialize() {
        let preset = CandidatePoolPreset::Filter(CandidatePoolPresetFilter {
            rarities: Some(vec![RarityRank::Tier6]),
            professions: Some(vec![ProfessionCategory::WARRIOR]),
            exclude_ids: Some(vec![1001]),
            ..Default::default()
        });

        let json = serde_json::to_string_pretty(&preset).unwrap();
        println!("Serialized JSON:\n{}", json);
    }

    #[test]
    fn test_filter_preset() {
        let characters = create_test_characters();

        let six_star_guard = CandidatePoolPreset::Filter(CandidatePoolPresetFilter {
            rarities: Some(vec![RarityRank::Tier6]),
            professions: Some(vec![ProfessionCategory::WARRIOR]),
            ..Default::default()
        })
        .generate_pool(&characters);

        assert_eq!(six_star_guard.len(), 2);
        assert!(six_star_guard.contains(&1001));
        assert!(six_star_guard.contains(&1002));
    }

    #[test]
    fn test_set_operations() {
        let characters = create_test_characters();

        let union_pool = CandidatePoolPreset::Union {
            presets: vec![
                CandidatePoolPreset::ByRarity {
                    rarities: vec![RarityRank::Tier6],
                },
                CandidatePoolPreset::Custom {
                    operator_ids: vec![3001],
                },
            ],
        }
        .generate_pool(&characters);
        assert_eq!(union_pool.len(), 4);

        let intersection_pool = CandidatePoolPreset::Intersection {
            presets: vec![
                CandidatePoolPreset::ByRarity {
                    rarities: vec![RarityRank::Tier6],
                },
                CandidatePoolPreset::ByProfession {
                    professions: vec![ProfessionCategory::WARRIOR],
                },
            ],
        }
        .generate_pool(&characters);
        assert_eq!(intersection_pool.len(), 2);
    }
}
