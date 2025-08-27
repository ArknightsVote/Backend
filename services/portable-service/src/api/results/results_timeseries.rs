use actix_web::{Responder, post, web};
use chrono::{DateTime, Utc};
use futures::TryStreamExt as _;
use mongodb::bson;
use serde::{Deserialize, Serialize};
use share::models::api::{ApiData, ApiMsg, ApiResponse};

use crate::{api::OperatorsInfo, error::AppError, state::AppState, timeseries::OperatorStatistics};

#[derive(Debug, Serialize)]
pub struct TimelineData {
    pub timeline: Vec<TimelinePoint>,
    pub operators: Vec<OperatorInfo>,
    pub summary: TimelineSummary,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub timestamp: DateTime<Utc>,
    pub data: Vec<OperatorSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct OperatorSnapshot {
    pub operator_id: i32,
    pub rate: f64,
}

#[derive(Debug, Serialize)]
pub struct OperatorInfo {
    pub operator_id: i32,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TimelineSummary {
    pub total_points: usize,
    pub time_range: TimeRange,
    pub operator_count: usize,
}

#[derive(Debug, Serialize)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

// 请求参数
#[derive(Debug, Deserialize)]
pub struct TimelineQuery {
    pub topic_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,

    pub operator_ids: Vec<i32>,

    pub limit: Option<i32>,
    pub granularity: Option<TimeGranularity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeGranularity {
    Second,
    Minute,
    Hour,
    Day,
}

#[post("/results/operator_timeline")]
pub async fn results_operator_timeline_fn(
    web::Json(params): web::Json<TimelineQuery>,
    state: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    let collection = state
        .database
        .mongo_database
        .collection::<OperatorStatistics>("operator_rates");

    // 构建查询条件
    let mut filter = bson::doc! {};

    // 时间范围过滤
    let start_time = bson::Bson::DateTime(bson::DateTime::from_millis(
        params.start_time.timestamp_millis(),
    ));
    let end_time = bson::Bson::DateTime(bson::DateTime::from_millis(
        params.end_time.timestamp_millis(),
    ));
    filter.insert(
        "ts",
        bson::doc! {
            "$gte": start_time,
            "$lte": end_time
        },
    );

    // 干员过滤
    filter.insert(
        "operator_id",
        bson::doc! {
            "$in": &params.operator_ids
        },
    );

    // 构建聚合管道
    let pipeline = build_timeline_pipeline(&params, &filter)?;

    // 执行查询
    let mut cursor: mongodb::Cursor<bson::Document> = match collection.aggregate(pipeline).await {
        Ok(cursor) => cursor,
        Err(_) => {
            return Ok(web::Json(ApiResponse {
                status: 500,
                data: ApiData::Empty,
                message: ApiMsg::InternalError,
            }));
        }
    };

    let mut timeline_points = Vec::new();

    while let Ok(Some(doc)) = cursor.try_next().await {
        let point = parse_timeline_point(doc)?;
        timeline_points.push(point);
    }

    let candidate_pool = match state
        .topic_service
        .get_candidate_pool(&params.topic_id, &state.character_infos)
        .await
    {
        Some(pool) => pool,
        None => {
            return Ok(web::Json(ApiResponse {
                status: 404,
                data: ApiData::Empty,
                message: ApiMsg::TargetTopicNotFound,
            }));
        }
    };
    let operators_info =
        crate::api::generate_operators_info(&candidate_pool, &state.character_infos);

    let operators = get_operator_info(&operators_info, &params.operator_ids);

    let summary = TimelineSummary {
        total_points: timeline_points.len(),
        time_range: calculate_time_range(&timeline_points),
        operator_count: operators.len(),
    };

    Ok(web::Json(ApiResponse {
        status: 200,
        data: ApiData::Data(TimelineData {
            timeline: timeline_points,
            operators,
            summary,
        }),
        message: ApiMsg::OK,
    }))
}

fn build_timeline_pipeline(
    params: &TimelineQuery,
    filter: &bson::Document,
) -> Result<Vec<bson::Document>, AppError> {
    let mut pipeline = Vec::new();

    if !filter.is_empty() {
        pipeline.push(bson::doc! { "$match": filter });
    }

    let group_by_time = match params
        .granularity
        .as_ref()
        .unwrap_or(&TimeGranularity::Second)
    {
        TimeGranularity::Second => bson::doc! {
            "year": { "$year": "$ts" },
            "month": { "$month": "$ts" },
            "day": { "$dayOfMonth": "$ts" },
            "hour": { "$hour": "$ts" },
            "minute": { "$minute": "$ts" },
            "second": { "$second": "$ts" }
        },
        TimeGranularity::Minute => bson::doc! {
            "year": { "$year": "$ts" },
            "month": { "$month": "$ts" },
            "day": { "$dayOfMonth": "$ts" },
            "hour": { "$hour": "$ts" },
            "minute": { "$minute": "$ts" }
        },
        TimeGranularity::Hour => bson::doc! {
            "year": { "$year": "$ts" },
            "month": { "$month": "$ts" },
            "day": { "$dayOfMonth": "$ts" },
            "hour": { "$hour": "$ts" }
        },
        TimeGranularity::Day => bson::doc! {
            "year": { "$year": "$ts" },
            "month": { "$month": "$ts" },
            "day": { "$dayOfMonth": "$ts" }
        },
    };

    // 第一次分组：按时间和干员分组
    pipeline.push(bson::doc! {
        "$group": {
            "_id": {
                "time": group_by_time,
                "operator_id": "$operator_id"
            },
            "avg_rate": { "$avg": "$rate" },
            "count": { "$sum": 1 },
            "timestamp": { "$first": "$ts" },
        }
    });

    // 第二次分组：按时间分组，聚合所有干员
    pipeline.push(bson::doc! {
        "$group": {
            "_id": "$_id.time",
            "timestamp": { "$first": "$timestamp" },
            "operators": {
                "$push": {
                    "operator_id": "$_id.operator_id",
                    "rate": "$avg_rate",
                    "sample_count": "$count"
                }
            }
        }
    });

    // 排序
    pipeline.push(bson::doc! {
        "$sort": { "timestamp": 1 }
    });

    // 限制结果数量
    if let Some(limit) = params.limit {
        pipeline.push(bson::doc! { "$limit": limit });
    }

    Ok(pipeline)
}

fn parse_timeline_point(doc: bson::Document) -> Result<TimelinePoint, AppError> {
    let timestamp = doc.get_datetime("timestamp")?;
    let operators_array = doc.get_array("operators")?;

    let mut data = Vec::new();
    for op_doc in operators_array {
        if let bson::Bson::Document(op) = op_doc {
            data.push(OperatorSnapshot {
                operator_id: op.get_i32("operator_id")?,
                rate: op.get_f64("rate")?,
            });
        }
    }

    Ok(TimelinePoint {
        timestamp: timestamp.to_system_time().into(),
        data,
    })
}

fn get_operator_info(operators_info: &OperatorsInfo, filter_ids: &[i32]) -> Vec<OperatorInfo> {
    filter_ids
        .iter()
        .filter_map(|id| {
            operators_info
                .reverse_operators_id_dict
                .get(id)
                .map(|name| OperatorInfo {
                    operator_id: *id,
                    name: name.clone(),
                })
        })
        .collect()
}

fn calculate_time_range(timeline_points: &[TimelinePoint]) -> TimeRange {
    let start = timeline_points
        .first()
        .map(|p| p.timestamp)
        .unwrap_or_else(Utc::now);

    let end = timeline_points
        .last()
        .map(|p| p.timestamp)
        .unwrap_or_else(Utc::now);

    TimeRange { start, end }
}
