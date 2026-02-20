pub mod controllers;
pub mod initializers;
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
    for entry in TEMPLATES.find("**/*.html").unwrap() {
        if let Some(file) = entry.as_file() {
            let path = file.path().to_str().unwrap();
            if tera.get_template(path).is_err() {
                tera.add_raw_template(path, file.contents_utf8().unwrap())?;
            }
        }
    }
    Ok(())
}
