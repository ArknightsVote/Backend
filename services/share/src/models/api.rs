use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::database::{CandidatePoolPreset, CreateTopicStatus, VotingTopicType};

#[derive(Default, Debug, Deserialize, Serialize, ToSchema)]
pub struct NewCompareRequest {
    pub topic_id: String,
    pub ballot_id: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "topic_type", rename_all = "snake_case")]
pub enum NewCompareResponse {
    Pairwise {
        topic_id: String,
        ballot_id: String,
        left: i32,
        right: i32,
    },
    Setwise {
        topic_id: String,
        ballot_id: String,
        left_set: Vec<i32>,
        right_set: Vec<i32>,
    },
    Groupwise {
        topic_id: String,
        ballot_id: String,
        left_group: Vec<i32>,
        right_group: Vec<i32>,
    },
    Plurality {
        topic_id: String,
        ballot_id: String,
        candidates: Vec<i32>,
    },
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub enum GroupwiseSelection {
    Left,
    Right,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PairwiseSaveScore {
    pub topic_id: String,
    pub ballot_id: String,
    pub winner: i32,
    pub loser: i32,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SetwiseSaveScore {
    pub topic_id: String,
    pub ballot_id: String,
    pub left_set: Vec<i32>,
    pub right_set: Vec<i32>,
    pub selected_left: Vec<i32>,
    pub selected_right: Vec<i32>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct GroupwiseSaveScore {
    pub topic_id: String,
    pub ballot_id: String,
    pub left_group: Vec<i32>,
    pub right_group: Vec<i32>,
    pub selected_group: GroupwiseSelection,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PluralitySaveScore {
    pub topic_id: String,
    pub ballot_id: String,
    pub candidates: Vec<i32>,
    pub selected: i32,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "topic_type", rename_all = "snake_case")]
pub enum SaveScoreRequest {
    Pairwise(PairwiseSaveScore),
    Setwise(SetwiseSaveScore),
    Groupwise(GroupwiseSaveScore),
    Plurality(PluralitySaveScore),
}

impl SaveScoreRequest {
    pub fn topic_id(&self) -> &String {
        match self {
            SaveScoreRequest::Pairwise(data) => &data.topic_id,
            SaveScoreRequest::Setwise(data) => &data.topic_id,
            SaveScoreRequest::Groupwise(data) => &data.topic_id,
            SaveScoreRequest::Plurality(data) => &data.topic_id,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SaveScoreResponse {
    pub code: i8,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct FinalOrderItem {
    pub name: String,
    pub id: i32,
    pub win: i64,
    pub lose: i64,
    pub score: String,
    pub rate: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ViewFinalOrderRequest {
    pub topic_id: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ViewFinalOrderResponse {
    pub topic_id: String,
    pub items: Vec<FinalOrderItem>,
    pub count: i64,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct Operators1v1MatrixRequest {
    pub topic_id: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct Operators1v1MatrixResponse {
    pub data: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTopicRequest {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub topic_type: VotingTopicType,
    pub candidate_pool: CandidatePoolPreset,

    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTopicResponse {
    pub id: String,
    pub is_active: bool,
    pub status: CreateTopicStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetTopicInfoRequest {
    pub topic_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetTopicInfoResponse {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub topic_type: VotingTopicType,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GetAllTopicsIdsResponse {
    pub topic_ids: Vec<String>,
}
