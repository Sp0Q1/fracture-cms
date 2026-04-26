//! `SeaORM` Entity, hand-written to mirror the codegen style used elsewhere in
//! this crate. Schema is owned by
//! `migration/src/m20260426_000001_create_resource_assignments.rs`.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "resource_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub pid: Uuid,
    pub user_id: i32,
    pub org_id: i32,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub resource_type: String,
    pub resource_id: i32,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub role_key: String,
    pub granted_by: Option<i32>,
    pub granted_at: DateTimeWithTimeZone,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub revoked_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id"
    )]
    Organization,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
