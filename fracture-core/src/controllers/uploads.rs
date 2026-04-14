use crate::controllers::middleware;
use crate::models::_entities::uploads as uploads_entity;
use crate::models::org_members::OrgRole;
use crate::models::uploads as upload_model;
use crate::require_user;
use crate::upload::config::UploadConfig;
use crate::upload::service::{UploadError, UploadService};
use axum::body::Body;
use axum::extract::Multipart;
use axum::http::header;
use axum_extra::extract::cookie::CookieJar;
use loco_rs::prelude::*;

/// Constructs an `UploadService` from the application settings.
async fn get_upload_service(ctx: &AppContext) -> Result<UploadService> {
    let config = UploadConfig::from_settings(ctx.config.settings.as_ref());
    UploadService::new(config)
        .await
        .map_err(|e| Error::Message(format!("Failed to initialize upload service: {e}")))
}

/// POST /api/uploads — upload a file via multipart form.
///
/// Requires authentication and an active org context.
/// The file is validated, stored, and a database record is created.
///
/// Returns JSON with the upload's public ID on success.
///
/// # Errors
///
/// Returns an error if auth fails, org context is missing, or upload processing fails.
#[debug_handler]
pub async fn create(
    State(ctx): State<AppContext>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);
    let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user)
        .await
        .ok_or_else(|| Error::NotFound)?;

    let service = get_upload_service(&ctx).await?;

    // Extract file from multipart form
    let mut file_data: Option<(String, String, Vec<u8>)> = None;
    let mut visibility = "org".to_string();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| Error::Message(format!("Multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let filename = field.file_name().unwrap_or("unnamed").to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| Error::Message(format!("Failed to read upload data: {e}")))?;
            file_data = Some((filename, content_type, data.to_vec()));
        } else if name == "visibility" {
            let val = field
                .text()
                .await
                .map_err(|e| Error::Message(format!("Failed to read visibility field: {e}")))?;
            if val == "public" || val == "org" {
                visibility = val;
            }
        }
    }

    let (filename, content_type, data) =
        file_data.ok_or_else(|| Error::Message("No file provided".to_string()))?;

    let result = service
        .upload(
            &ctx.db,
            org_ctx.org.id,
            user.id,
            &filename,
            &content_type,
            data,
            &visibility,
        )
        .await
        .map_err(|e| match e {
            UploadError::FileTooLarge { size, limit } => Error::Message(format!(
                "File too large: {size} bytes exceeds limit of {limit} bytes"
            )),
            UploadError::Validation(v) => Error::Message(format!("Validation failed: {v}")),
            UploadError::Storage(s) => Error::Message(format!("Storage error: {s}")),
            UploadError::Database(d) => Error::Message(format!("Database error: {d}")),
        })?;

    let body = serde_json::json!({
        "pid": result.pid.to_string(),
        "content_type": result.content_type,
        "size_bytes": result.size_bytes,
        "checksum_sha256": result.checksum_sha256,
    });

    format::json(body)
}

/// GET /api/uploads/{pid} — serve an uploaded file.
///
/// Access control:
/// - Public uploads are served to anyone.
/// - Org-scoped uploads require the requester to be a member of the org.
///
/// Returns 404 for missing files or access denied (to prevent enumeration).
///
/// # Errors
///
/// Returns an error if the file is not found or cannot be read.
#[debug_handler]
pub async fn show(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let upload = upload_model::Model::find_by_pid(&ctx.db, &pid)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Access control based on visibility
    let vis = upload_model::Visibility::parse(&upload.visibility);
    match vis {
        Some(upload_model::Visibility::Public) => {
            // Public files are served to everyone
        }
        Some(upload_model::Visibility::Org) | None => {
            // Org-scoped: require authenticated user who is either:
            // 1. A member of the upload's org, OR
            // 2. A platform admin
            //
            // Apps can extend this with additional checks (e.g. pentester
            // assignment) by overriding the upload routes.
            let user = middleware::get_current_user(&jar, &ctx).await;
            let Some(user) = user else {
                return Err(Error::NotFound);
            };

            let is_platform_admin =
                crate::models::organizations::Model::is_user_platform_admin(&ctx.db, user.id).await;

            if !is_platform_admin {
                let is_org_member = crate::models::_entities::org_members::Entity::find()
                    .filter(crate::models::_entities::org_members::Column::OrgId.eq(upload.org_id))
                    .filter(crate::models::_entities::org_members::Column::UserId.eq(user.id))
                    .one(&ctx.db)
                    .await
                    .ok()
                    .flatten()
                    .is_some();

                if !is_org_member {
                    return Err(Error::NotFound);
                }
            }
        }
    }

    let service = get_upload_service(&ctx).await?;
    let data = service
        .read_file(&upload)
        .await
        .map_err(|_| Error::NotFound)?;

    // Determine cache headers based on visibility
    let cache_control = match vis {
        Some(upload_model::Visibility::Public) => "public, max-age=86400, immutable",
        _ => "private, no-cache",
    };

    axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(header::CONTENT_TYPE, &upload.content_type)
        .header(header::CACHE_CONTROL, cache_control)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", upload.original_name),
        )
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(data))
        .map(axum::response::IntoResponse::into_response)
        .map_err(|e| Error::Message(format!("Response build error: {e}")))
}

/// DELETE /api/uploads/{pid} — delete an uploaded file.
///
/// Only the uploader or an org admin can delete a file.
/// Returns 404 for missing files or access denied (to prevent enumeration).
///
/// # Errors
///
/// Returns an error if the file is not found or the user lacks permission.
#[debug_handler]
pub async fn destroy(
    Path(pid): Path<String>,
    State(ctx): State<AppContext>,
    jar: CookieJar,
) -> Result<Response> {
    let user = middleware::get_current_user(&jar, &ctx).await;
    let user = require_user!(user);

    let upload = upload_model::Model::find_by_pid(&ctx.db, &pid)
        .await?
        .ok_or_else(|| Error::NotFound)?;

    // Authorization: uploader, org admin, or platform admin
    // Apps can extend with additional checks (e.g. pentester assignment)
    // by overriding the upload routes.
    let is_uploader = upload.uploaded_by == user.id;
    if !is_uploader {
        let is_platform_admin =
            crate::models::organizations::Model::is_user_platform_admin(&ctx.db, user.id).await;

        if !is_platform_admin {
            let org_ctx = middleware::get_org_context_or_default(&jar, &ctx.db, &user).await;
            let is_org_admin = org_ctx
                .as_ref()
                .is_some_and(|c| c.org.id == upload.org_id && c.role.at_least(OrgRole::Admin));

            if !is_org_admin {
                return Err(Error::NotFound);
            }
        }
    }

    // Delete the file from storage
    let service = get_upload_service(&ctx).await?;
    let _ = service.delete_file(&upload).await; // Best effort: DB record removal is the source of truth

    // Delete the database record
    let active: uploads_entity::ActiveModel = upload.into();
    active.delete(&ctx.db).await?;

    axum::response::Response::builder()
        .status(axum::http::StatusCode::NO_CONTENT)
        .body(Body::empty())
        .map(axum::response::IntoResponse::into_response)
        .map_err(|e| Error::Message(format!("Response build error: {e}")))
}

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/uploads")
        .add("/", post(create))
        .add("/{pid}", get(show))
        .add("/{pid}", delete(destroy))
}
