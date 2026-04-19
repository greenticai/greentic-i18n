use crate::json_map::{JsonMap, parse_json_map};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub updated: Vec<String>,
}

pub fn merge_base(repo_root: &Path, left: &str, right: &str) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("merge-base")
        .arg(left)
        .arg(right)
        .output()
        .map_err(|err| format!("failed to execute git merge-base: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git merge-base failed: {stderr}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn read_file_at_ref(repo_root: &Path, git_ref: &str, path: &Path) -> Result<String, String> {
    let spec = format!("{git_ref}:{}", to_git_path(repo_root, path)?);
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("show")
        .arg(spec)
        .output()
        .map_err(|err| format!("failed to execute git show: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git show failed: {stderr}"));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("git output was not UTF-8: {err}"))
}

pub fn diff_en_at_refs(
    repo_root: &Path,
    base_ref: &str,
    head_ref: &str,
    en_path: &Path,
) -> Result<DiffReport, String> {
    let base_raw = read_file_at_ref(repo_root, base_ref, en_path)?;
    let head_raw = read_file_at_ref(repo_root, head_ref, en_path)?;
    let base_map = parse_json_map(&base_raw)?;
    let head_map = parse_json_map(&head_raw)?;
    Ok(diff_en_maps(&base_map, &head_map))
}

pub fn diff_en_maps(base_map: &JsonMap, head_map: &JsonMap) -> DiffReport {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut updated = Vec::new();

    for (key, head_val) in head_map {
        match base_map.get(key) {
            None => added.push(key.clone()),
            Some(base_val) if base_val != head_val => updated.push(key.clone()),
            Some(_) => {}
        }
    }

    for key in base_map.keys() {
        if !head_map.contains_key(key) {
            removed.push(key.clone());
        }
    }

    added.sort();
    removed.sort();
    updated.sort();

    DiffReport {
        added,
        removed,
        updated,
    }
}

fn to_git_path(repo_root: &Path, path: &Path) -> Result<String, String> {
    let rel: PathBuf = if path.is_absolute() {
        path.strip_prefix(repo_root)
            .map_err(|_| {
                format!(
                    "path `{}` is outside repo root `{}`",
                    path.display(),
                    repo_root.display()
                )
            })?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::{DiffReport, diff_en_at_refs, diff_en_maps, merge_base, read_file_at_ref};
    use crate::json_map::JsonMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("gt-i18n-git-{name}-{stamp}"))
    }

    fn git(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .expect("git should execute");
        assert!(status.success(), "git {:?} should succeed", args);
    }

    fn git_output(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .expect("git should execute");
        assert!(output.status.success(), "git {:?} should succeed", args);
        String::from_utf8(output.stdout)
            .expect("git output should be utf8")
            .trim()
            .to_string()
    }

    #[test]
    fn diff_en_maps_computes_added_removed_updated() {
        let mut base = JsonMap::new();
        base.insert("kept".to_string(), "same".to_string());
        base.insert("removed".to_string(), "gone".to_string());
        base.insert("updated".to_string(), "before".to_string());

        let mut head = JsonMap::new();
        head.insert("kept".to_string(), "same".to_string());
        head.insert("updated".to_string(), "after".to_string());
        head.insert("added".to_string(), "new".to_string());

        let report = diff_en_maps(&base, &head);
        assert_eq!(
            report,
            DiffReport {
                added: vec!["added".to_string()],
                removed: vec!["removed".to_string()],
                updated: vec!["updated".to_string()],
            }
        );
    }

    #[test]
    fn diff_en_maps_reports_empty_when_identical() {
        let mut left = JsonMap::new();
        left.insert("k".to_string(), "v".to_string());
        let right = left.clone();

        let report = diff_en_maps(&left, &right);
        assert!(report.added.is_empty());
        assert!(report.removed.is_empty());
        assert!(report.updated.is_empty());
    }

    #[test]
    fn git_helpers_read_refs_and_compute_diff() {
        let repo = temp_repo("diff");
        fs::create_dir_all(repo.join("i18n")).expect("repo dir should exist");
        git(&repo, &["init"]);
        git(&repo, &["config", "user.name", "Codex"]);
        git(&repo, &["config", "user.email", "codex@example.com"]);

        let en_path = repo.join("i18n/en.json");
        fs::write(&en_path, "{\n  \"hello\": \"Hello\"\n}\n").expect("write should succeed");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "base"]);
        let base = git_output(&repo, &["rev-parse", "HEAD"]);

        git(&repo, &["checkout", "-b", "feature"]);
        fs::write(
            &en_path,
            "{\n  \"hello\": \"Hello there\",\n  \"bye\": \"Bye\"\n}\n",
        )
        .expect("write should succeed");
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "head"]);
        let head = git_output(&repo, &["rev-parse", "HEAD"]);

        assert_eq!(
            merge_base(&repo, &base, &head).expect("merge-base should work"),
            base
        );
        assert!(
            read_file_at_ref(&repo, &base, &en_path)
                .expect("git show should work")
                .contains("\"hello\": \"Hello\"")
        );

        let report = diff_en_at_refs(&repo, &base, &head, &en_path).expect("diff should work");
        assert_eq!(report.added, vec!["bye".to_string()]);
        assert_eq!(report.updated, vec!["hello".to_string()]);
        assert!(report.removed.is_empty());

        let _ = fs::remove_dir_all(repo);
    }
}
