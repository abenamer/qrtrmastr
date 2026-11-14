//! Writes a redacted body to the local fixtures directory, which git ignores. The file name is
//! the message key with everything but letters, digits, dots and dashes replaced. A relative
//! directory is resolved against the repository root, not the working directory, so the file
//! lands under the ignored path from any subdirectory.

use std::path::{Path, PathBuf};

use chrono::Datelike;

use crate::application::ports::StoredBody;

/// The nearest ancestor of `from` that holds a `.git` entry, when there is one.
pub fn repository_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// `dir` as given when absolute; else under the repository root that contains `cwd`, or under
/// `cwd` itself outside a repository.
pub fn anchor(dir: &Path, cwd: &Path) -> PathBuf {
    if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        repository_root(cwd)
            .unwrap_or_else(|| cwd.to_path_buf())
            .join(dir)
    }
}

pub fn write_fixture(dir: &Path, body: &StoredBody) -> std::io::Result<Option<PathBuf>> {
    let Some(html) = &body.body_html else {
        return Ok(None);
    };
    let safe: String = body
        .key
        .as_str()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let target = dir
        .join(body.platform.as_str())
        .join(body.received_at.year().to_string())
        .join(format!("{safe}.html"));
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, html)?;
    Ok(Some(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::MessageKey;
    use crate::domain::states::Platform;

    #[test]
    fn the_file_lands_under_platform_and_year_with_a_safe_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body = StoredBody {
            platform: Platform::Freshdirect,
            key: MessageKey::from_message_id("<a/b@mail.example>").expect("key"),
            received_at: "2026-03-03T11:15:00Z".parse().expect("date"),
            body_html: Some("<p>x</p>".to_owned()),
        };
        let path = write_fixture(dir.path(), &body)
            .expect("write")
            .expect("has html");
        assert!(
            path.ends_with("freshdirect/2026/a_b_mail.example.html"),
            "{}",
            path.display()
        );
        assert_eq!(std::fs::read_to_string(path).expect("read"), "<p>x</p>");
        let without = StoredBody {
            body_html: None,
            ..body
        };
        assert_eq!(write_fixture(dir.path(), &without).expect("write"), None);
    }

    #[test]
    fn a_relative_directory_is_anchored_to_the_repository_root() {
        let repo = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(repo.path().join(".git")).expect("git dir");
        let sub = repo.path().join("docs").join("deep");
        std::fs::create_dir_all(&sub).expect("subdir");
        assert_eq!(
            anchor(Path::new("tests/fixtures/local"), &sub),
            repo.path().join("tests/fixtures/local")
        );
        assert_eq!(
            anchor(Path::new("/abs/dir"), &sub),
            PathBuf::from("/abs/dir")
        );
        let outside = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            anchor(Path::new("x"), outside.path()),
            outside.path().join("x")
        );
    }
}
