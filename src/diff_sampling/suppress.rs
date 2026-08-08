use super::notices::DIFF_SUPPRESSED_CONTENT_NOTICE;
use crate::diff_parse::DiffFile;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SuppressionResult {
    pub files: Vec<DiffFile>,
    pub suppressed_count: usize,
}

pub fn suppress_generated_content(files: &[DiffFile], dirs: &[String]) -> SuppressionResult {
    if dirs.is_empty() {
        return SuppressionResult {
            files: files.to_vec(),
            suppressed_count: 0,
        };
    }

    let mut files = files.to_vec();
    let mut suppressed_count = 0usize;
    for file in &mut files {
        if !path_in_dirs(&file.path, dirs) || file.hunks.is_empty() {
            continue;
        }
        file.hunks.clear();
        if !file.header.is_empty() && !file.header.ends_with('\n') {
            file.header.push('\n');
        }
        file.header.push_str(DIFF_SUPPRESSED_CONTENT_NOTICE);
        suppressed_count += 1;
    }

    SuppressionResult {
        files,
        suppressed_count,
    }
}

pub fn rebuild_patch_from_files(files: &[DiffFile]) -> String {
    let mut patch = String::new();
    for file in files {
        patch.push_str(&file.header);
        for hunk in &file.hunks {
            patch.push_str(hunk);
        }
    }
    patch
}

fn path_in_dirs(path: &str, dirs: &[String]) -> bool {
    if path.is_empty() {
        return false;
    }
    dirs.iter().any(|dir| {
        path == dir
            || path
                .strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::{rebuild_patch_from_files, suppress_generated_content};
    use crate::diff_parse::parse_diff_files;
    use crate::diff_sampling::notices::DIFF_SUPPRESSED_CONTENT_NOTICE;

    fn suppressed_diff() -> String {
        vec![
            "diff --git a/.sqlx/query-aaa.json b/.sqlx/query-aaa.json",
            "index 111..222 100644",
            "--- a/.sqlx/query-aaa.json",
            "+++ b/.sqlx/query-aaa.json",
            "@@ -1 +1 @@",
            "-{\"query\": \"SELECT 1\"}",
            "+{\"query\": \"SELECT 2\"}",
            "diff --git a/src/app.rs b/src/app.rs",
            "index 333..444 100644",
            "--- a/src/app.rs",
            "+++ b/src/app.rs",
            "@@ -1 +1 @@",
            "-old-app",
            "+new-app",
            "",
        ]
        .join("\n")
    }

    #[test]
    fn strips_hunks_and_keeps_header_for_matching_dirs() {
        let files = parse_diff_files(&suppressed_diff());
        let result = suppress_generated_content(&files, &[".sqlx".to_string()]);

        assert_eq!(result.suppressed_count, 1);
        assert_eq!(result.files[0].path, ".sqlx/query-aaa.json");
        assert!(result.files[0].hunks.is_empty());
        assert!(
            result.files[0]
                .header
                .contains(DIFF_SUPPRESSED_CONTENT_NOTICE.trim())
        );
        assert_eq!(result.files[1].hunks.len(), 1);
        assert!(
            !result.files[1]
                .header
                .contains(DIFF_SUPPRESSED_CONTENT_NOTICE.trim())
        );
    }

    #[test]
    fn matches_nested_paths_but_not_prefix_siblings() {
        let diff = [
            "diff --git a/.sqlx/query-aaa.json b/.sqlx/query-aaa.json",
            "@@ -1 +1 @@",
            "-a",
            "+b",
            "diff --git a/.sqlx_extra/x.json b/.sqlx_extra/x.json",
            "@@ -1 +1 @@",
            "-x",
            "+y",
            "diff --git a/other/.sqlx/y.json b/other/.sqlx/y.json",
            "@@ -1 +1 @@",
            "-y",
            "+z",
            "",
        ]
        .join("\n");
        let files = parse_diff_files(&diff);

        let result = suppress_generated_content(&files, &[".sqlx".to_string()]);

        assert_eq!(result.suppressed_count, 1);
        assert_eq!(result.files[0].path, ".sqlx/query-aaa.json");
        assert_eq!(result.files[1].hunks.len(), 1);
        assert_eq!(result.files[2].hunks.len(), 1);
    }

    #[test]
    fn empty_dirs_list_disables_suppression() {
        let files = parse_diff_files(&suppressed_diff());
        let result = suppress_generated_content(&files, &[]);

        assert_eq!(result.suppressed_count, 0);
        assert_eq!(result.files, files);
    }

    #[test]
    fn header_only_matched_files_are_not_counted_as_suppressed() {
        let diff = [
            "diff --git a/.sqlx/old.json b/.sqlx/new.json",
            "similarity index 100%",
            "rename from .sqlx/old.json",
            "rename to .sqlx/new.json",
            "",
        ]
        .join("\n");
        let files = parse_diff_files(&diff);

        let result = suppress_generated_content(&files, &[".sqlx".to_string()]);

        assert_eq!(result.suppressed_count, 0);
        assert!(
            !result.files[0]
                .header
                .contains(DIFF_SUPPRESSED_CONTENT_NOTICE.trim())
        );
    }

    #[test]
    fn rebuilt_patch_keeps_headers_and_drops_suppressed_hunks() {
        let files = parse_diff_files(&suppressed_diff());
        let result = suppress_generated_content(&files, &[".sqlx".to_string()]);

        let rebuilt = rebuild_patch_from_files(&result.files);

        assert!(rebuilt.contains("diff --git a/.sqlx/query-aaa.json"));
        assert!(rebuilt.contains(DIFF_SUPPRESSED_CONTENT_NOTICE.trim()));
        assert!(!rebuilt.contains("SELECT 1"));
        assert!(!rebuilt.contains("SELECT 2"));
        assert!(rebuilt.contains("+new-app"));
    }

    #[test]
    fn rebuild_is_lossless_without_suppression() {
        let diff = suppressed_diff();
        let files = parse_diff_files(&diff);
        let result = suppress_generated_content(&files, &[]);

        assert_eq!(rebuild_patch_from_files(&result.files), diff);
    }

    #[test]
    fn suppression_round_trip_rebuild_matches_raw_shape() {
        let diff = suppressed_diff();
        let files = parse_diff_files(&diff);
        let result = suppress_generated_content(&files, &[".sqlx".to_string()]);

        let rebuilt = rebuild_patch_from_files(&result.files);

        assert!(rebuilt.starts_with("diff --git a/.sqlx/query-aaa.json"));
        assert_eq!(
            rebuilt
                .lines()
                .filter(|line| line.starts_with("diff --git "))
                .count(),
            2
        );
    }
}
