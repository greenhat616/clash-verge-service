use std::borrow::Cow;

use axum::{Json, http::StatusCode};
use nyanpasu_ipc::api::{
    RBuilder,
    core::start::{CoreStartReq, CoreStartRes},
};

pub async fn start(
    Json(payload): Json<CoreStartReq<'_>>,
) -> (StatusCode, Json<CoreStartRes<'static>>) {
    let instance = crate::server::CoreManager::global();
    let res = instance
        .start(&payload.core_type, &payload.config_file)
        .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(RBuilder::success(()))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RBuilder::other_error(Cow::Owned(e.to_string()))),
        ),
    }
}
