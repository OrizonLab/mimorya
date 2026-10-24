//! The company's documents (ADR-095): listed, deposited (the file as the
//! request's body, its name and space in a strict query string), read,
//! downloaded and deleted, each where the person's rights reach; anything
//! beyond answers as absent. A deposit's space is checked before its file
//! is read, and a file announced or found beyond its bound is not read on.

use axum::body::Body as RawBody;
use axum::extract::State;
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use mimorya_core::DomainError;
use mimorya_core::authz::Sensitivity;
use mimorya_core::documents::limits::MAX_FILE_BYTES;
use mimorya_core::documents::{DocumentSpace, Refusal};

use crate::app::AppState;
use crate::contract::{DocumentCreated, DocumentItem, DocumentList, DocumentUpload, Done};
use crate::http::{ApiError, ApiResult, PathId, Query};
use crate::plugins::Auth;
use crate::services::documents::{Deposit, DepositFailure};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/documents", get(list).post(deposit))
        .route("/documents/{id}", get(read).delete(remove))
        .route("/documents/{id}/file", get(download))
}

fn absent() -> ApiError {
    DomainError::not_found("Document introuvable").into()
}

async fn list(
    State(state): State<AppState>,
    Auth(identity): Auth,
) -> ApiResult<Json<DocumentList>> {
    let access = state.access.context(identity).await?;
    Ok(Json(state.documents.list(&access).await?))
}

async fn deposit(
    State(state): State<AppState>,
    Auth(identity): Auth,
    Query(upload): Query<DocumentUpload>,
    headers: HeaderMap,
    body: RawBody,
) -> ApiResult<(StatusCode, Json<DocumentCreated>)> {
    let access = state.access.context(identity).await?;
    let space = upload
        .unit_id
        .map_or(DocumentSpace::Company, DocumentSpace::Unit);
    let sensitivity = upload
        .sensitivity
        .map_or(Sensitivity::Internal, Sensitivity::from);
    state.documents.check_deposit(&access, space, sensitivity)?;
    let announced = headers
        .get(CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok())
        .and_then(|length| length.parse::<u64>().ok());
    if announced.is_some_and(|length| length > MAX_FILE_BYTES as u64) {
        return Err(refused(Refusal::TooLarge));
    }
    let bytes = axum::body::to_bytes(body, MAX_FILE_BYTES)
        .await
        .map_err(|_| refused(Refusal::TooLarge))?;
    let deposit = Deposit {
        name: &upload.name,
        space,
        sensitivity,
        bytes: bytes.to_vec(),
    };
    match state.documents.deposit(&access, deposit).await {
        Ok(created) => Ok((StatusCode::CREATED, Json(created))),
        Err(DepositFailure::Refused(refusal)) => Err(refused(refusal)),
        Err(DepositFailure::Domain(error)) => Err(error.into()),
        Err(DepositFailure::Internal(error)) => Err(error.into()),
    }
}

/// A file refused, with its reason: too large (413), not a kind taken
/// (415), or not readable as it is (422).
fn refused(refusal: Refusal) -> ApiError {
    let status = match refusal {
        Refusal::TooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        Refusal::NotAccepted | Refusal::Mismatch => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    ApiError::request(status, refusal.to_string())
}

async fn read(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<DocumentItem>> {
    let access = state.access.context(identity).await?;
    let item = state.documents.get(&access, id).await?;
    item.map(Json).ok_or_else(absent)
}

async fn download(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Response> {
    let access = state.access.context(identity).await?;
    let file = state
        .documents
        .file(&access, id)
        .await?
        .ok_or_else(absent)?;
    let disposition = HeaderValue::from_str(&attachment(&file.file_name))
        .map_err(|error| anyhow::anyhow!("content disposition: {error}"))?;
    let headers = [
        (CONTENT_TYPE, HeaderValue::from_static(file.media_type)),
        (CONTENT_DISPOSITION, disposition),
    ];
    Ok((headers, file.content).into_response())
}

async fn remove(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
) -> ApiResult<Json<Done>> {
    let access = state.access.context(identity).await?;
    if state.documents.delete(&access, id).await? {
        Ok(Json(Done::OK))
    } else {
        Err(absent())
    }
}

/// `Content-Disposition` downloading as `name`: an ASCII fallback, and the
/// name itself encoded (RFC 6266, RFC 8187).
fn attachment(name: &str) -> String {
    let fallback: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_names_its_file_in_ascii_and_in_full() {
        assert_eq!(
            attachment("Procédure \"purge\".pdf"),
            "attachment; filename=\"Proc_dure _purge_.pdf\"; \
             filename*=UTF-8''Proc%C3%A9dure%20%22purge%22.pdf"
        );
    }
}
