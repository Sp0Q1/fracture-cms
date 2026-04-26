//! Subresource Integrity (SRI) helpers — computed once at boot, exposed to
//! Tera as the `sri()` function so templates never carry hand-maintained
//! hash literals.
//!
//! The Content Security Policy in this codebase forbids `unsafe-inline`
//! and `unsafe-eval`, but it does not stop a malicious / accidental
//! same-origin asset swap (e.g. a corrupted build artifact). SRI on every
//! `<link>` and `<script>` closes that gap. The painful part of SRI is
//! keeping the hash in the template in sync with the file on disk; this
//! module removes that pain by computing the hash at boot and giving the
//! Tera template a function call instead of a literal.
//!
//! ```ignore
//! // base.html
//! <link rel="stylesheet"
//!       href="/static/app.css"
//!       integrity="{{ sri(path='/static/app.css') }}"
//!       crossorigin="anonymous">
//! ```
//!
//! Wiring (in your app's `view_engine` initializer):
//! ```ignore
//! use fracture_core::views::sri::{SriIndex, register_sri_function};
//!
//! let index = SriIndex::from_directory("assets/static", "/static")?;
//! tera_engine = engines::TeraView::build()?.post_process(move |tera| {
//!     register_sri_function(tera, index.clone());
//!     fracture_core::register_templates(tera).map_err(...)?;
//!     Ok(())
//! })?;
//! ```

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use sha2::{Digest, Sha384};

/// Precomputed SHA-384 SRI digests keyed by URL path. Cheap to clone
/// (the inner map is wrapped in `Arc`).
#[derive(Debug, Clone, Default)]
pub struct SriIndex {
    by_url: Arc<HashMap<String, String>>,
}

impl SriIndex {
    /// An empty index — every lookup returns `None`. Used in tests and as
    /// a fallback when the static directory does not exist.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Walk a directory, compute SHA-384 for every `.css` and `.js` file,
    /// and key the result by `<url_prefix>/<relative-path>`. Subdirectories
    /// are followed.
    ///
    /// `url_prefix` is the URL the consuming web server serves the
    /// directory under, e.g. `"/static"`.
    ///
    /// Missing directories are not an error — `from_directory` returns an
    /// empty index, so apps without static assets still boot.
    ///
    /// # Errors
    /// Returns an error if `read_dir` or `read` fails on a path that exists.
    pub fn from_directory(static_dir: &Path, url_prefix: &str) -> io::Result<Self> {
        let mut by_url = HashMap::new();
        if static_dir.is_dir() {
            walk(static_dir, static_dir, url_prefix, &mut by_url)?;
        }
        Ok(Self {
            by_url: Arc::new(by_url),
        })
    }

    /// Construct an index from an explicit set of (url, content) pairs.
    /// Useful for tests and embedded assets.
    #[must_use]
    pub fn from_pairs<I, U, B>(items: I) -> Self
    where
        I: IntoIterator<Item = (U, B)>,
        U: Into<String>,
        B: AsRef<[u8]>,
    {
        let by_url = items
            .into_iter()
            .map(|(url, bytes)| (url.into(), digest(bytes.as_ref())))
            .collect();
        Self {
            by_url: Arc::new(by_url),
        }
    }

    /// Return `"sha384-<base64>"` for `url`, or `None` if not registered.
    #[must_use]
    pub fn get(&self, url: &str) -> Option<&str> {
        self.by_url.get(url).map(String::as_str)
    }

    /// Number of entries; for diagnostics/logging.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_url.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_url.is_empty()
    }
}

fn walk(
    root: &Path,
    dir: &Path,
    url_prefix: &str,
    by_url: &mut HashMap<String, String>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, url_prefix, by_url)?;
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("css" | "js")
        ) {
            // strip_prefix can only fail if `root` isn't actually a prefix of `path`,
            // which can't happen — we walked from `root`. The map_err is defensive.
            let rel = path.strip_prefix(root).map_err(io::Error::other)?;
            let url = format!(
                "{}/{}",
                url_prefix.trim_end_matches('/'),
                rel.to_string_lossy()
            );
            let bytes = fs::read(&path)?;
            by_url.insert(url, digest(&bytes));
        }
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha384::new();
    hasher.update(bytes);
    let out = hasher.finalize();
    format!("sha384-{}", B64.encode(out))
}

/// Register `sri(path="/static/foo.css")` as a Tera function.
///
/// The function returns the precomputed SRI hash string (e.g.
/// `"sha384-..."`). It is an *error* (Tera reports it at render time)
/// to ask for an unregistered URL — that points at a typo or a missing
/// asset, both of which deserve a loud failure.
pub fn register_sri_function(tera: &mut tera::Tera, index: SriIndex) {
    tera.register_function(
        "sri",
        move |args: &HashMap<String, tera::Value>| -> tera::Result<tera::Value> {
            let url = args
                .get("path")
                .and_then(tera::Value::as_str)
                .ok_or_else(|| tera::Error::msg("sri(): missing required `path` argument"))?;
            index.get(url).map_or_else(
                || {
                    Err(tera::Error::msg(format!(
                        "sri(): no asset registered at `{url}`. Check the path; the static \
                         directory was scanned at boot and only css/js files are indexed."
                    )))
                },
                |hash| Ok(tera::Value::String(hash.to_string())),
            )
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_pairs_computes_correct_hash() {
        // Known vector: SHA-384("hello") = base64-encoded 59...
        // Verified: echo -n "hello" | openssl dgst -sha384 -binary | openssl base64 -A
        let idx = SriIndex::from_pairs([("/static/h.css", "hello")]);
        let v = idx.get("/static/h.css").expect("must be present");
        assert!(v.starts_with("sha384-"), "must be prefixed with sha384-");
        assert_eq!(
            v,
            "sha384-WeF0h3dEjGnea4ANejO7+5/xtGPkQ1TDVTvNucZm+pASWjx5+QOXvfX2oT3oKGhP"
        );
    }

    #[test]
    fn empty_lookup_returns_none() {
        let idx = SriIndex::empty();
        assert!(idx.get("/static/missing.css").is_none());
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    #[test]
    fn from_pairs_preserves_count() {
        let idx = SriIndex::from_pairs([
            ("/static/a.css", "x"),
            ("/static/b.js", "y"),
            ("/static/c.css", "z"),
        ]);
        assert_eq!(idx.len(), 3);
        assert!(idx.get("/static/a.css").is_some());
        assert!(idx.get("/static/b.js").is_some());
        assert!(idx.get("/static/c.css").is_some());
    }

    #[test]
    fn from_directory_missing_dir_yields_empty() {
        let idx = SriIndex::from_directory(Path::new("/no/such/dir"), "/static")
            .expect("missing directory must not error");
        assert!(idx.is_empty());
    }

    #[test]
    fn from_directory_indexes_css_and_js() {
        let tmp = tempfile_dir();
        std::fs::write(tmp.join("a.css"), b"body{color:red}").unwrap();
        std::fs::write(tmp.join("b.js"), b"window.x=1").unwrap();
        std::fs::write(tmp.join("ignore.png"), b"png-bytes").unwrap();

        let idx = SriIndex::from_directory(&tmp, "/static").unwrap();
        assert_eq!(idx.len(), 2, "css + js indexed; png ignored");
        assert!(idx.get("/static/a.css").unwrap().starts_with("sha384-"));
        assert!(idx.get("/static/b.js").unwrap().starts_with("sha384-"));
        assert!(idx.get("/static/ignore.png").is_none());
    }

    #[test]
    fn from_directory_walks_subdirs() {
        let tmp = tempfile_dir();
        let sub = tmp.join("vendor");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("lib.js"), b"var lib").unwrap();

        let idx = SriIndex::from_directory(&tmp, "/static").unwrap();
        assert!(idx.get("/static/vendor/lib.js").is_some());
    }

    #[test]
    fn tera_function_returns_registered_hash() {
        let mut tera = tera::Tera::default();
        let idx = SriIndex::from_pairs([("/static/a.css", "hello")]);
        register_sri_function(&mut tera, idx);

        tera.add_raw_template("t", r#"{{ sri(path='/static/a.css') }}"#)
            .unwrap();
        let rendered = tera.render("t", &tera::Context::new()).unwrap();
        assert!(rendered.starts_with("sha384-"));
    }

    #[test]
    fn tera_function_errors_on_missing_path() {
        let mut tera = tera::Tera::default();
        register_sri_function(&mut tera, SriIndex::empty());
        tera.add_raw_template("t", r#"{{ sri(path='/static/missing.css') }}"#)
            .unwrap();
        let result = tera.render("t", &tera::Context::new());
        assert!(result.is_err(), "missing asset must be a render-time error");
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!(
            "fracture-sri-test-{}-{}",
            std::process::id(),
            // Wall-clock nanos are unique enough for serial test runs.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        base
    }
}
