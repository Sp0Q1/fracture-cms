pub mod controllers;
pub mod entity_registry;
pub mod initializers;
pub mod jobs;
pub mod mailers;
pub mod models;
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
    for entry in entries {
        if let Some(file) = entry.as_file() {
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
    }
    Ok(())
}
