// =============================================================================
// API error type with axum IntoResponse implementation.
//
// Maps ServiceError variants to HTTP status codes and JSON error bodies.
// Internal details (database queries, file paths, stack traces) are logged
// server-side but never exposed to the client.
// =============================================================================

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use neuronprompter_application::ServiceError;
use neuronprompter_core::CoreError;
use serde::Serialize;

/// Structured JSON error body returned to API clients.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

/// API error type that converts domain errors to HTTP responses.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub body: ErrorBody,
}

impl ApiError {
    pub(crate) fn new(status: StatusCode, code: &str, message: String) -> Self {
        Self {
            status,
            body: ErrorBody {
                code: code.to_owned(),
                message,
            },
        }
    }

    /// Creates an internal server error.
    pub fn internal(message: String) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", message)
    }

    /// Creates a clipboard-specific error.
    pub fn clipboard(message: String) -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CLIPBOARD_ERROR",
            message,
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::to_string(&self.body).unwrap_or_else(|_| {
            r#"{"code":"INTERNAL_ERROR","message":"failed to serialize error"}"#.to_owned()
        });
        (
            self.status,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            body,
        )
            .into_response()
    }
}

impl From<ServiceError> for ApiError {
    fn from(err: ServiceError) -> Self {
        match &err {
            ServiceError::Core(core_err) => Self::from_core_error(core_err),
            ServiceError::Database(db_err) => {
                // Unwrap domain errors wrapped in DbError::Core so they receive
                // the correct HTTP status (e.g. 404 NOT_FOUND) instead of a
                // generic 500 DATABASE_ERROR.
                if let neuronprompter_db::DbError::Core(core_err) = db_err {
                    return Self::from_core_error(core_err);
                }
                tracing::error!(error = %db_err, "database error");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    "An internal database error occurred".to_owned(),
                )
            }
            ServiceError::OllamaUnavailable(detail) => {
                // Log at warn level. Ollama being unavailable is not a server error
                // but a configuration/environment issue the operator should be aware of.
                tracing::warn!(error = %detail, "Ollama service unavailable");
                Self::new(
                    StatusCode::BAD_GATEWAY,
                    "OLLAMA_UNAVAILABLE",
                    "The Ollama service is currently unavailable".to_owned(),
                )
            }
            ServiceError::OllamaError(detail) => {
                tracing::warn!(error = %detail, "Ollama operation failed");
                Self::new(
                    StatusCode::BAD_GATEWAY,
                    "OLLAMA_ERROR",
                    "Ollama operation failed".to_owned(),
                )
            }
            ServiceError::IoError(io_err) => {
                tracing::error!(error = %io_err, "I/O error");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "IO_ERROR",
                    "An internal I/O error occurred".to_owned(),
                )
            }
            ServiceError::SerializationError(msg) => {
                tracing::error!(error = %msg, "serialization error");
                Self::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "SERIALIZATION_ERROR",
                    "A serialization error occurred".to_owned(),
                )
            }
        }
    }
}

impl ApiError {
    fn from_core_error(err: &CoreError) -> Self {
        match err {
            CoreError::Validation { .. } => {
                Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", err.to_string())
            }
            CoreError::NotFound { .. } => {
                Self::new(StatusCode::NOT_FOUND, "NOT_FOUND", err.to_string())
            }
            CoreError::Duplicate { .. } => {
                Self::new(StatusCode::CONFLICT, "DUPLICATE", err.to_string())
            }
            CoreError::EntityInUse { entity_type, .. } => {
                let code = format!("{}_IN_USE", entity_type.to_uppercase());
                Self::new(StatusCode::CONFLICT, &code, err.to_string())
            }
            CoreError::Authorization { .. } => Self::new(
                StatusCode::FORBIDDEN,
                "AUTHORIZATION_ERROR",
                err.to_string(),
            ),
            CoreError::PathTraversal { .. } => {
                tracing::warn!(error = %err, "path traversal attempt blocked");
                Self::new(
                    StatusCode::FORBIDDEN,
                    "PATH_TRAVERSAL",
                    "Path access denied".to_owned(),
                )
            }
            CoreError::Conflict { .. } => {
                // Log the conflict details server-side; return a generic message
                // to avoid leaking version identifiers or internal state.
                tracing::warn!(error = %err, "version conflict detected");
                Self::new(
                    StatusCode::CONFLICT,
                    "VERSION_CONFLICT",
                    "A version conflict occurred; please reload and retry".to_owned(),
                )
            }
        }
    }
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        Self::from_core_error(&err)
    }
}

impl From<neuronprompter_db::DbError> for ApiError {
    fn from(err: neuronprompter_db::DbError) -> Self {
        // Route through ServiceError::Database so the same mapping logic applies.
        Self::from(ServiceError::from(err))
    }
}

impl From<axum::extract::rejection::JsonRejection> for ApiError {
    fn from(rejection: axum::extract::rejection::JsonRejection) -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            rejection.body_text(),
        )
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.body.code, self.body.message)
    }
}

/// JSON extractor that maps deserialization errors to 400 VALIDATION_ERROR
/// instead of axum's default 422 response.
pub struct ValidatedJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for ValidatedJson<T>
where
    axum::Json<T>:
        axum::extract::FromRequest<S, Rejection = axum::extract::rejection::JsonRejection>,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(req, state).await?;
        Ok(Self(value))
    }
}

/// Runs a blocking closure on the tokio blocking threadpool with a 30-second
/// timeout. Maps `JoinError` (task panic) and `Elapsed` (timeout) to
/// `ApiError`. The timeout prevents runaway database operations from blocking
/// the threadpool indefinitely.
///
/// # Errors
///
/// Returns `ApiError` with status 500 (INTERNAL_ERROR) if the blocking task
/// panics, or status 504 (TIMEOUT) if the operation exceeds 30 seconds.
/// Also propagates any `ApiError` returned by the closure itself.
pub async fn run_blocking<F, T>(f: F) -> Result<T, ApiError>
where
    F: FnOnce() -> Result<T, ApiError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(f),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => {
            tracing::error!("blocking task panicked: {join_err}");
            Err(ApiError::internal("internal processing error".to_owned()))
        }
        Err(_elapsed) => {
            tracing::error!("blocking database operation timed out after 30 seconds");
            Err(ApiError::new(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                "TIMEOUT",
                "The operation timed out after 30 seconds".to_owned(),
            ))
        }
    }
}
