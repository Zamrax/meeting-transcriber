use std::fs;
use std::path::{Path, PathBuf};

use crate::schema::MeetingAnalysis;

use super::markdown::{get_filename, to_markdown};

/// Write meeting notes into the Obsidian vault under `Meeting Notes/`.
///
/// Returns the absolute path of the written file.
pub fn export_to_obsidian(analysis: &MeetingAnalysis, vault_path: &str) -> Result<String, String> {
    let vault = Path::new(vault_path);
    if !vault.exists() {
        return Err(format!("Obsidian vault path does not exist: {vault_path}"));
    }

    let notes_dir = vault.join("Meeting Notes");
    fs::create_dir_all(&notes_dir)
        .map_err(|e| format!("Failed to create Meeting Notes directory: {e}"))?;

    let filename = get_filename(analysis);
    let target = notes_dir.join(&filename);

    // Guard against path traversal
    let canonical_notes = notes_dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve notes directory: {e}"))?;

    // Check if the target path would escape the vault
    let target_parent = target
        .parent()
        .unwrap_or(&notes_dir);
    let canonical_target_parent = target_parent
        .canonicalize()
        .map_err(|e| format!("Failed to resolve target directory: {e}"))?;

    if !canonical_target_parent.starts_with(&canonical_notes) {
        return Err("Filename would escape the vault directory".into());
    }

    // Avoid overwriting: add numeric suffix if file exists
    let final_path = find_available_path(&target);

    let content = to_markdown(analysis);
    fs::write(&final_path, content.as_bytes())
        .map_err(|e| format!("Failed to write file: {e}"))?;

    Ok(final_path.to_string_lossy().to_string())
}

/// Find an available file path, adding numeric suffixes to avoid collisions.
/// e.g., "file.md" -> "file (1).md" -> "file (2).md"
fn find_available_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..1000 {
        let candidate = parent.join(format!("{stem} ({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Extremely unlikely fallback
    parent.join(format!("{stem} (overflow){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ActionItem;
    use std::collections::HashMap;

    fn sample_analysis() -> MeetingAnalysis {
        MeetingAnalysis {
            meeting_title: "Sprint Planning".into(),
            meeting_date: "2026-03-15".into(),
            transcript: "Speaker 1: Hello".into(),
            summary: "Discussed sprint goals.".into(),
            responsibilities: HashMap::new(),
            action_items: vec![ActionItem {
                owner: "Alice".into(),
                description: "Deploy".into(),
                deadline: None,
            }],
        }
    }

    #[test]
    fn test_export_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_str().unwrap();

        let result = export_to_obsidian(&sample_analysis(), vault);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(Path::new(&path).exists());
        assert!(path.contains("Meeting Notes"));
        assert!(path.ends_with(".md"));
    }

    #[test]
    fn test_export_content_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_str().unwrap();

        let path = export_to_obsidian(&sample_analysis(), vault).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Sprint Planning"));
        assert!(content.contains("Speaker 1: Hello"));
    }

    #[test]
    fn test_export_nonexistent_vault() {
        let result = export_to_obsidian(&sample_analysis(), "/nonexistent/vault");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_export_collision_avoidance() {
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().to_str().unwrap();

        let path1 = export_to_obsidian(&sample_analysis(), vault).unwrap();
        let path2 = export_to_obsidian(&sample_analysis(), vault).unwrap();
        assert_ne!(path1, path2);
        assert!(path2.contains("(1)"));
    }

    #[test]
    fn test_find_available_path_no_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.md");
        assert_eq!(find_available_path(&path), path);
    }

    #[test]
    fn test_find_available_path_with_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.md");
        fs::write(&path, "existing").unwrap();

        let result = find_available_path(&path);
        assert!(result.to_string_lossy().contains("test (1).md"));
    }
}
