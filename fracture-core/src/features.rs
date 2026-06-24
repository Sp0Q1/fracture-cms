//! Deployment feature flags — what's switched on for this instance.
//!
//! Flags are read once at startup from the app config (`settings.*`) into a
//! process-global, so both the Rust route guards and the templates resolve the
//! same value cheaply, with no per-request work. Templates read flags through
//! the `feature(name="…")` Tera function (registered via
//! [`register_feature_function`]); Rust reads them via [`features`] /
//! [`is_enabled`].

use std::collections::HashMap;
use std::sync::OnceLock;

/// The resolved feature flags for the running instance.
#[derive(Debug, Clone, Copy)]
pub struct Features {
    /// Whether the public blog is served and linked in the nav.
    pub blog_enabled: bool,
}

impl Default for Features {
    /// Everything on — the flags exist to *disable* features, so an absent
    /// config leaves the full product enabled.
    fn default() -> Self {
        Self { blog_enabled: true }
    }
}

static FEATURES: OnceLock<Features> = OnceLock::new();

/// Installs the resolved flags. First call wins (subsequent calls are ignored),
/// so call it once at startup.
pub fn init_features(features: Features) {
    let _ = FEATURES.set(features);
}

/// The active flags, or [`Features::default`] if not yet initialised.
#[must_use]
pub fn features() -> Features {
    FEATURES.get().copied().unwrap_or_default()
}

/// Parses flags from loco's `settings` value, e.g. `settings.blog.enabled`.
/// Missing keys default to enabled.
#[must_use]
pub fn from_settings(settings: Option<&serde_json::Value>) -> Features {
    let flag = |group: &str| {
        settings
            .and_then(|s| s.get(group))
            .and_then(|g| g.get("enabled"))
            .and_then(serde_json::Value::as_bool)
    };
    Features {
        blog_enabled: flag("blog").unwrap_or(true),
    }
}

/// Whether a named feature is enabled. Unknown names default to enabled, so a
/// template referencing a not-yet-defined flag fails open rather than hiding
/// working UI.
#[must_use]
pub fn is_enabled(name: &str) -> bool {
    match name {
        "blog" => features().blog_enabled,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn blog_disabled_when_explicitly_false() {
        let settings = json!({ "blog": { "enabled": false } });
        assert!(!from_settings(Some(&settings)).blog_enabled);
    }

    #[test]
    fn blog_enabled_when_true_absent_or_no_settings() {
        assert!(from_settings(Some(&json!({ "blog": { "enabled": true } }))).blog_enabled);
        // Missing blog group, missing enabled key, and no settings all default on.
        assert!(from_settings(Some(&json!({ "other": 1 }))).blog_enabled);
        assert!(from_settings(Some(&json!({ "blog": {} }))).blog_enabled);
        assert!(from_settings(None).blog_enabled);
    }

    #[test]
    fn unknown_feature_defaults_enabled() {
        assert!(is_enabled("something-new"));
    }
}

/// Registers the `feature(name="blog")` Tera function so templates can gate UI
/// on a flag: `{% if feature(name="blog") %}…{% endif %}`.
pub fn register_feature_function(tera: &mut tera::Tera) {
    tera.register_function(
        "feature",
        |args: &HashMap<String, tera::Value>| -> tera::Result<tera::Value> {
            let name = args
                .get("name")
                .and_then(tera::Value::as_str)
                .ok_or_else(|| tera::Error::msg("feature(): missing required `name` argument"))?;
            Ok(tera::Value::Bool(is_enabled(name)))
        },
    );
}
