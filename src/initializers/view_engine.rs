use async_trait::async_trait;
use axum::{Extension, Router as AxumRouter};
use fluent_templates::{ArcLoader, FluentLoader};
use fracture_core::views::sri::{register_sri_function, SriIndex};
use loco_rs::{
    app::{AppContext, Initializer},
    controller::views::{engines, ViewEngine},
    Error, Result,
};
use tracing::info;

const I18N_DIR: &str = "assets/i18n";
const I18N_SHARED: &str = "assets/i18n-shared.ftl";
const STATIC_DIR: &str = "assets/static";
const STATIC_URL: &str = "/static";

pub struct TemplateInitializer;

#[async_trait]
impl Initializer for TemplateInitializer {
    fn name(&self) -> String {
        "view-engine".to_string()
    }

    async fn after_routes(&self, router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        // SHA-384 every static asset once at boot. Templates use
        // `{{ sri(path='/static/foo.css') }}` instead of carrying literal
        // hashes that can drift out of sync.
        let sri_index = SriIndex::from_directory(std::path::Path::new(STATIC_DIR), STATIC_URL)
            .map_err(|e| Error::string(&format!("SRI index build failed: {e}")))?;
        info!(entries = sri_index.len(), "SRI index built");

        let tera_engine = if std::path::Path::new(I18N_DIR).exists() {
            let arc = std::sync::Arc::new(
                ArcLoader::builder(&I18N_DIR, unic_langid::langid!("en-US"))
                    .shared_resources(Some(&[I18N_SHARED.into()]))
                    .customize(|bundle| bundle.set_use_isolating(false))
                    .build()
                    .map_err(|e| Error::string(&e.to_string()))?,
            );
            info!("locales loaded");

            let sri = sri_index;
            engines::TeraView::build()?.post_process(move |tera| {
                tera.register_function("t", FluentLoader::new(arc.clone()));
                register_sri_function(tera, sri.clone());
                fracture_core::features::register_feature_function(tera);
                fracture_core::register_templates(tera)
                    .map_err(|e| loco_rs::Error::string(&e.to_string()))?;
                Ok(())
            })?
        } else {
            let sri = sri_index;
            engines::TeraView::build()?.post_process(move |tera| {
                register_sri_function(tera, sri.clone());
                fracture_core::features::register_feature_function(tera);
                fracture_core::register_templates(tera)
                    .map_err(|e| loco_rs::Error::string(&e.to_string()))?;
                Ok(())
            })?
        };

        Ok(router.layer(Extension(ViewEngine::from(tera_engine))))
    }
}
