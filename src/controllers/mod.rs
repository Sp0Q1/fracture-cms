pub mod fallback;
pub mod home;
pub mod note;
pub mod project;
pub use fracture_core::controllers::{
    admin, blog, jobs, middleware, oidc, oidc_state, org, uploads,
};
