pub mod captcha;
pub mod controllers;
pub mod entity_registry;
pub mod initializers;
pub mod jobs;
pub mod listing;
pub mod mailers;
pub mod models;
pub mod permissions;
pub mod upload;
pub mod views;

use include_dir::{include_dir, Dir};

static TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Register embedded core templates with Tera.
///
/// Templates are only added if the app hasn't already provided an override
/// (i.e., the app's filesystem templates take precedence).
///
/// # Errors
///
/// Returns an error if a template cannot be parsed.
pub fn register_templates(tera: &mut tera::Tera) -> Result<(), tera::Error> {
    let Ok(entries) = TEMPLATES.find("**/*.html") else {
        return Ok(());
    };
    // Parents must be registered before the templates that extend them
    // (add_raw_template validates inheritance immediately), so register
    // root-level layouts like public_base.html first.
    let mut files: Vec<_> = entries.filter_map(|e| e.as_file()).collect();
    files.sort_by_key(|f| f.path().components().count());
    for file in files {
        let Some(path) = file.path().to_str() else {
            continue;
        };
        if tera.get_template(path).is_err() {
            let Some(contents) = file.contents_utf8() else {
                continue;
            };
            tera.add_raw_template(path, contents)?;
        }
    }
    Ok(())
}
