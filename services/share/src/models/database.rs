use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::{
    api::SaveScoreRequest,
    excel::{CharacterInfo, RarityRank},
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum VotingTopicType {
    Pairwise,  // 两两对比
    Setwise,   // 集合对比
    Groupwise, // 群组对比
    Plurality, // 多数表决
}

impl VotingTopicType {
    pub fn matches_request(&self, req: &SaveScoreRequest) -> bool {
        matches!(
            (self, req),
            (VotingTopicType::Pairwise, SaveScoreRequest::Pairwise(_))
                | (VotingTopicType::Setwise, SaveScoreRequest::Setwise(_))
                | (VotingTopicType::Groupwise, SaveScoreRequest::Groupwise(_))
                | (VotingTopicType::Plurality, SaveScoreRequest::Plurality(_))
        )
    }

    pub fn supports_final_order(&self) -> bool {
        matches!(self, VotingTopicType::Pairwise)
    }

    pub fn supports_1v1_matrix(&self) -> bool {
        matches!(self, VotingTopicType::Pairwise)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum CandidatePoolPreset {
    SixStarOperators, // 所有六星干员
    AllOperators,     // 所有干员
    Custom(Vec<i32>), // 自定义候选池
}

impl CandidatePoolPreset {
    pub fn generate_pool(&self, character_infos: &[CharacterInfo]) -> Vec<i32> {
        match self {
            CandidatePoolPreset::SixStarOperators => character_infos
                .iter()
                .filter(|op| op.rarity == RarityRank::Tier6)
                .map(|op| op.id)
                .collect(),
            CandidatePoolPreset::AllOperators => character_infos.iter().map(|op| op.id).collect(),
            CandidatePoolPreset::Custom(ops) => ops.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum AuditCategory {
    ContentCompliance,    // 内容合规
    PoliticalSensitive,   // 政治敏感
    InappropriateContent, // 不当内容
    Spam,                 // 垃圾信息
    Duplicate,            // 重复内容
    TechnicalIssue,       // 技术问题
    Other(String),        // 其他原因
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TopicAuditInfo {
    pub auditor_id: Uuid,              // 审核员ID
    pub auditor_name: String,          // 审核员姓名
    pub audit_time: DateTime<Utc>,     // 审核时间
    pub audit_reason: String,          // 审核原因/备注
    pub audit_category: AuditCategory, // 审核类别
}

impl TopicAuditInfo {
    pub fn is_approved(&self) -> bool {
        matches!(self.audit_category, AuditCategory::ContentCompliance)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum CreateTopicStatus {
    WaitingAudit,
    Approved(TopicAuditInfo),
    Rejected(TopicAuditInfo),
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VotingTopic {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub topic_type: VotingTopicType,
    pub candidate_pool: CandidatePoolPreset,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,

    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,

    pub is_active: bool,
    pub status: CreateTopicStatus,
}

impl VotingTopic {
    pub fn is_topic_active(&self) -> bool {
        self.is_active
            && self.open_time <= chrono::Utc::now()
            && self.close_time >= chrono::Utc::now()
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BallotInfo<'a> {
    pub topic_id: Cow<'a, str>,
    pub ballot_id: Cow<'a, str>,
    pub ip: Cow<'a, str>,
    pub user_agent: Cow<'a, str>,
    pub timestamp: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PairwiseBallot<'a> {
    pub info: BallotInfo<'a>,
    pub win: i32,
    pub lose: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SetwiseBallot<'a> {
    pub info: BallotInfo<'a>,
    pub left_set: Vec<i32>,
    pub right_set: Vec<i32>,
    pub selected_left: Vec<i32>,
    pub selected_right: Vec<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GroupwiseBallot<'a> {
    pub info: BallotInfo<'a>,
    pub left_group: Vec<i32>,
    pub right_group: Vec<i32>,
    pub selected_group: super::api::GroupwiseSelection,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PluralityBallot<'a> {
    pub info: BallotInfo<'a>,
    pub candidates: Vec<i32>,
    pub selected: i32,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "topic_type", rename_all = "snake_case")]
pub enum Ballot<'a> {
    Pairwise(PairwiseBallot<'a>),
    Setwise(SetwiseBallot<'a>),
    Groupwise(GroupwiseBallot<'a>),
    Plurality(PluralityBallot<'a>),
}

#[derive(Debug, Serialize)]
pub struct StoredBallot<'a> {
    #[serde(flatten)]
    pub ballot: Ballot<'a>,
    pub multiplier: i32,
}
