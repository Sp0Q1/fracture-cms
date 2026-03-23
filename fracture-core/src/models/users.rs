use async_trait::async_trait;
use chrono::offset::Local;
use loco_rs::{auth::jwt, hash, prelude::*};
use serde::Deserialize;
use serde_json::Map;
use uuid::Uuid;

pub use super::_entities::users::{self, ActiveModel, Column, Entity, Model};

#[derive(Debug, Clone)]
pub struct OidcUserInfo {
    pub provider: String,
    pub subject: String,
    pub email: String,
    pub name: Option<String>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct Validator {
    #[validate(length(min = 2, message = "Name must be at least 2 characters long."))]
    pub name: String,
    #[validate(email(message = "invalid email"))]
    pub email: String,
}

impl Validatable for ActiveModel {
    fn validator(&self) -> Box<dyn Validate> {
        Box::new(Validator {
            name: self.name.as_ref().to_owned(),
            email: self.email.as_ref().to_owned(),
        })
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for super::_entities::users::ActiveModel {
    async fn before_save<C>(self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        self.validate()?;
        if insert {
            let mut this = self;
            this.pid = ActiveValue::Set(Uuid::new_v4().to_string());
            this.api_key = ActiveValue::Set(format!("lo-{}", Uuid::new_v4()));
            Ok(this)
        } else {
            Ok(self)
        }
    }
}

#[async_trait]
impl Authenticable for Model {
    async fn find_by_api_key(db: &DatabaseConnection, api_key: &str) -> ModelResult<Self> {
        let user = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(users::Column::ApiKey, api_key)
                    .build(),
            )
            .one(db)
            .await?;
        user.ok_or_else(|| ModelError::EntityNotFound)
    }

    async fn find_by_claims_key(db: &DatabaseConnection, claims_key: &str) -> ModelResult<Self> {
        Self::find_by_pid(db, claims_key).await
    }
}

impl Model {
    /// finds a user by the provided email
    ///
    /// # Errors
    ///
    /// When could not find user by the given token or DB query error
    pub async fn find_by_email(db: &DatabaseConnection, email: &str) -> ModelResult<Self> {
        let user = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(users::Column::Email, email)
                    .build(),
            )
            .one(db)
            .await?;
        user.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// finds a user by the provided pid
    ///
    /// # Errors
    ///
    /// When could not find user  or DB query error
    pub async fn find_by_pid(db: &DatabaseConnection, pid: &str) -> ModelResult<Self> {
        Uuid::parse_str(pid).map_err(|e| ModelError::Any(e.into()))?;
        let user = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(users::Column::Pid, pid)
                    .build(),
            )
            .one(db)
            .await?;
        user.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// finds a user by the provided api key
    ///
    /// # Errors
    ///
    /// When could not find user by the given token or DB query error
    pub async fn find_by_api_key(db: &DatabaseConnection, api_key: &str) -> ModelResult<Self> {
        let user = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(users::Column::ApiKey, api_key)
                    .build(),
            )
            .one(db)
            .await?;
        user.ok_or_else(|| ModelError::EntityNotFound)
    }

    /// Finds or creates a user from OIDC authentication info.
    ///
    /// Lookup order:
    /// 1. By OIDC provider + subject (existing OIDC link)
    /// 2. By email (link existing account to OIDC)
    /// 3. Create new user with random password and auto-verified email
    ///
    /// # Errors
    ///
    /// When DB query fails or user creation fails
    pub async fn find_or_create_from_oidc(
        db: &DatabaseConnection,
        info: &OidcUserInfo,
    ) -> ModelResult<Self> {
        // 1. Find by OIDC provider + subject
        if let Some(user) = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(Column::OidcProvider, &info.provider)
                    .eq(Column::OidcSubject, &info.subject)
                    .build(),
            )
            .one(db)
            .await?
        {
            if user.session_invalidated_at.is_some() {
                let mut active: ActiveModel = user.into();
                active.session_invalidated_at = ActiveValue::Set(None);
                let updated = active.update(db).await?;
                return Ok(updated);
            }
            return Ok(user);
        }

        // 2. Find by email and link OIDC (only if the existing account has a verified email)
        if let Some(user) = users::Entity::find()
            .filter(
                model::query::condition()
                    .eq(Column::Email, &info.email)
                    .build(),
            )
            .one(db)
            .await?
        {
            if user.email_verified_at.is_none() {
                return Err(ModelError::msg(
                    "cannot link OIDC to an account with unverified email",
                ));
            }
            let mut active: ActiveModel = user.into();
            active.oidc_provider = ActiveValue::Set(Some(info.provider.clone()));
            active.oidc_subject = ActiveValue::Set(Some(info.subject.clone()));
            active.session_invalidated_at = ActiveValue::Set(None);
            let updated = active.update(db).await?;
            return Ok(updated);
        }

        // 3. Create new user
        let password_hash = hash::hash_password(&Uuid::new_v4().to_string())
            .map_err(|e| ModelError::Any(e.into()))?;
        let name = info
            .name
            .clone()
            .unwrap_or_else(|| info.email.split('@').next().unwrap_or("user").to_string());

        let user = users::ActiveModel {
            email: ActiveValue::Set(info.email.clone()),
            password: ActiveValue::Set(password_hash),
            name: ActiveValue::Set(name),
            oidc_provider: ActiveValue::Set(Some(info.provider.clone())),
            oidc_subject: ActiveValue::Set(Some(info.subject.clone())),
            email_verified_at: ActiveValue::Set(Some(chrono::offset::Local::now().into())),
            ..Default::default()
        }
        .insert(db)
        .await?;

        // Create personal organization for the new user
        super::organizations::Model::create_personal_org(db, &user)
            .await
            .map_err(|e| ModelError::Any(e.into()))?;

        // Auto-accept any pending invites for this email
        let pending_invites =
            super::org_invites::Model::find_pending_by_email(db, &info.email).await;
        for invite in pending_invites {
            let _ = super::org_invites::Model::accept_invite(db, invite, user.id).await;
        }

        Ok(user)
    }

    /// Creates a JWT
    ///
    /// # Errors
    ///
    /// when could not convert user claims to jwt token
    pub fn generate_jwt(&self, secret: &str, expiration: u64) -> ModelResult<String> {
        jwt::JWT::new(secret)
            .generate_token(expiration, self.pid.clone(), Map::new())
            .map_err(ModelError::from)
    }
}

impl ActiveModel {
    /// Records the verification time when a user verifies their
    /// email and updates it in the database.
    ///
    /// # Errors
    ///
    /// when has DB query error
    pub async fn verified(mut self, db: &DatabaseConnection) -> ModelResult<Model> {
        self.email_verified_at = ActiveValue::set(Some(Local::now().into()));
        self.update(db).await.map_err(ModelError::from)
    }
}
