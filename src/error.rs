use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub(crate) type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
pub(crate) enum ApiError {
    NotFound(String),
    BadRequest(String),
    Database(sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(message) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),

            Self::BadRequest(message) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse { error: message }),
            )
                .into_response(),

            Self::Database(error) => {
                tracing::error!("Erro banco de dados: {error:?}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Erro interno do servidor".to_string(),
                    }),
                )
                    .into_response()
            }
        }
    }
}
