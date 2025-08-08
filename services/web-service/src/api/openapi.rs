use utoipa::OpenApi;

use share::models::api::{
    ApiMsg, AuditTopicsListResponse, BallotCreateRequest, BallotCreateResponse, BallotSaveRequest,
    BallotSaveResponse, Results1v1MatrixResponse, ResultsFinalOrderRequest,
    ResultsFinalOrderResponse, TopicCreateRequest, TopicCreateResponse, TopicInfoRequest,
    TopicInfoResponse, TopicListActiveResponse,
};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Ark Vote Backend",
        description = "Backend for the Ark Vote application"
    ),
    tags(
        (name = "Audit", description = "Topic audit related endpoints"),
        (name = "Ballot", description = "Voting ballot related endpoints"),
        (name = "Results", description = "Voting results related endpoints"),
        (name = "Topic", description = "Topic info related endpoints"),
    ),
    paths(
        crate::api::audit::audit_topic::audit_topic,
        crate::api::audit::audit_topics_list::audit_topics_list,
        crate::api::ballot::ballot_create::ballot_create,
        crate::api::ballot::ballot_save::ballot_save,
        crate::api::results::results_1v1_matrix::results_1v1_matrix,
        crate::api::results::results_final_order::results_final_order,
        crate::api::topic::topic_candidate_pool::topic_candidate_pool,
        crate::api::topic::topic_create::topic_create,
        crate::api::topic::topic_info::topic_info,
        crate::api::topic::topic_list_active::topic_list_active,
    ),
    components(schemas(
        TopicListActiveResponse,
        TopicCreateRequest,
        TopicCreateResponse,
        TopicInfoRequest,
        TopicInfoResponse,
        BallotCreateRequest,
        BallotCreateResponse,
        Results1v1MatrixResponse,
        BallotSaveRequest,
        BallotSaveResponse,
        ResultsFinalOrderRequest,
        ResultsFinalOrderResponse,
        AuditTopicsListResponse,
        ApiMsg
    ))
)]
pub struct ApiDoc;
