use super::{
    DIFF_DELETED_FILE_NOTICE, DIFF_SAMPLING_NOTICE, DIFF_SUPPRESSED_CONTENT_NOTICE, DiffBudget,
    prepare_diff_for_prompt, rebuild_patch_from_files, resolve_diff_budget, sample_diff_patch,
    suppress_generated_content,
};
use crate::config::DiffBudgetConfig;
use crate::diff_parse::{DiffFileKind, parse_diff_files};
use crate::tokenizer::Tokenizer;

#[test]
fn splits_files_and_hunks() {
    let diff = [
        "diff --git a/a.txt b/a.txt",
        "index 111..222 100644",
        "--- a/a.txt",
        "+++ b/a.txt",
        "@@ -1 +1 @@",
        "-old-a",
        "+new-a",
        "@@ -10 +10 @@",
        "-old-a-2",
        "+new-a-2",
        "diff --git a/b.txt b/b.txt",
        "index 333..444 100644",
        "--- a/b.txt",
        "+++ b/b.txt",
        "@@ -1 +1 @@",
        "-old-b",
        "+new-b",
        "",
    ]
    .join("\n");

    let files = parse_diff_files(&diff);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].hunks.len(), 2);
    assert_eq!(files[1].hunks.len(), 1);
    assert!(files[0].header.contains("a/a.txt"));
    assert_eq!(files[0].kind, DiffFileKind::Modified);
}

#[test]
fn classifies_semantic_kinds() {
    let cases = vec![
        (
            "deleted",
            [
                "diff --git a/obsolete.txt b/obsolete.txt",
                "deleted file mode 100644",
                "index 1111111..0000000",
                "--- a/obsolete.txt",
                "+++ /dev/null",
                "@@ -1,2 +0,0 @@",
                "-old",
                "-data",
                "",
            ]
            .join("\n"),
            DiffFileKind::Deleted,
        ),
        (
            "added",
            [
                "diff --git a/new.txt b/new.txt",
                "new file mode 100644",
                "index 0000000..1111111",
                "--- /dev/null",
                "+++ b/new.txt",
                "@@ -0,0 +1 @@",
                "+hello",
                "",
            ]
            .join("\n"),
            DiffFileKind::Added,
        ),
        (
            "renamed",
            [
                "diff --git a/old.txt b/new.txt",
                "similarity index 100%",
                "rename from old.txt",
                "rename to new.txt",
                "",
            ]
            .join("\n"),
            DiffFileKind::Renamed,
        ),
        (
            "copied",
            [
                "diff --git a/original.txt b/copy.txt",
                "similarity index 100%",
                "copy from original.txt",
                "copy to copy.txt",
                "",
            ]
            .join("\n"),
            DiffFileKind::Copied,
        ),
        (
            "binary",
            [
                "diff --git a/logo.png b/logo.png",
                "index 1111111..2222222 100644",
                "GIT binary patch",
                "literal 12",
                "zcmYdHexamplepayload",
                "",
            ]
            .join("\n"),
            DiffFileKind::Binary,
        ),
        (
            "mode-only",
            [
                "diff --git a/script.sh b/script.sh",
                "old mode 100644",
                "new mode 100755",
                "",
            ]
            .join("\n"),
            DiffFileKind::ModeOnly,
        ),
        (
            "header-only",
            [
                "diff --git a/vendor/lib b/vendor/lib",
                "index 1111111..2222222 160000",
                "--- a/vendor/lib",
                "+++ b/vendor/lib",
                "Submodule vendor/lib 1111111..2222222",
                "",
            ]
            .join("\n"),
            DiffFileKind::SubmoduleOrOtherHeaderOnly,
        ),
    ];

    for (_name, diff, want) in cases {
        let files = parse_diff_files(&diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, want);
    }
}

#[test]
fn parses_target_paths_from_diff_headers() {
    let diff = [
        "diff --git a/src/app.rs b/src/app.rs",
        "index 111..222 100644",
        "--- a/src/app.rs",
        "+++ b/src/app.rs",
        "@@ -1 +1 @@",
        "-a",
        "+b",
        "diff --git a/old.json b/.sqlx/query-aaa.json",
        "similarity index 50%",
        "rename from old.json",
        "rename to .sqlx/query-aaa.json",
        "diff --git \"a/weird file.txt\" \"b/weird file.txt\"",
        "index 111..222 100644",
        "--- \"a/weird file.txt\"",
        "+++ \"b/weird file.txt\"",
        "@@ -1 +1 @@",
        "-x",
        "+y",
        "diff --git \"a/.sqlx/query-\\303\\251.json\" \"b/.sqlx/query-\\303\\251.json\"",
        "index 111..222 100644",
        "--- \"a/.sqlx/query-\\303\\251.json\"",
        "+++ \"b/.sqlx/query-\\303\\251.json\"",
        "@@ -1 +1 @@",
        "-old",
        "+new",
        "",
    ]
    .join("\n");

    let files = parse_diff_files(&diff);
    let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();

    assert_eq!(
        paths,
        vec![
            "src/app.rs",
            ".sqlx/query-aaa.json",
            "weird file.txt",
            ".sqlx/query-é.json"
        ]
    );
}

#[test]
fn represents_multiple_files() {
    let diff = build_multi_file_diff(&["alpha.txt", "beta.txt", "gamma.txt"], 20);
    let files = parse_diff_files(&diff);
    let (sampled, represented, truncated) = sample_diff_patch(&files, &diff, 900, None);

    assert!(truncated);
    assert!(represented >= 2);
    assert!(sampled.contains("alpha.txt"));
    assert!(sampled.contains("beta.txt"));
    assert!(sampled.contains(DIFF_SAMPLING_NOTICE.trim()));
}

#[test]
fn omits_deleted_file_hunks() {
    let diff = vec![
        "diff --git a/obsolete.txt b/obsolete.txt".to_string(),
        "deleted file mode 100644".to_string(),
        "index 1111111..0000000".to_string(),
        "--- a/obsolete.txt".to_string(),
        "+++ /dev/null".to_string(),
        "@@ -1,2 +0,0 @@".to_string(),
        "-legacy-line-1".to_string(),
        "-legacy-line-2".to_string(),
        repeat_patch_line("-legacy-context", 60),
        "diff --git a/active.txt b/active.txt".to_string(),
        "index 3333333..4444444 100644".to_string(),
        "--- a/active.txt".to_string(),
        "+++ b/active.txt".to_string(),
        "@@ -1 +1 @@".to_string(),
        "-old-active".to_string(),
        "+new-active".to_string(),
        repeat_patch_line("+active-context", 30),
        String::new(),
    ]
    .join("\n");

    let files = parse_diff_files(&diff);
    let (sampled, represented, truncated) = sample_diff_patch(&files, &diff, 900, None);

    assert!(truncated);
    assert_eq!(represented, 2);
    assert!(sampled.contains("obsolete.txt"));
    assert!(sampled.contains(DIFF_DELETED_FILE_NOTICE.trim()));
    assert!(!sampled.contains("-legacy-line-1"));
    assert!(sampled.contains("+new-active"));
}

#[test]
fn keeps_full_diff_when_under_budget() {
    let diff_stat = " alpha.txt | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n";
    let diff_patch = [
        "diff --git a/alpha.txt b/alpha.txt",
        "index 111..222 100644",
        "--- a/alpha.txt",
        "+++ b/alpha.txt",
        "@@ -1 +1 @@",
        "-old",
        "+new",
        "",
    ]
    .join("\n");

    let files = parse_diff_files(&diff_patch);
    let (trimmed_stat, sampled_patch, result) = prepare_diff_for_prompt(
        &files,
        diff_stat,
        &diff_patch,
        DiffBudget {
            configured_tokens: 6000,
            effective_tokens: 6000,
        },
    )
    .expect("diff prep");

    assert!(!result.sampled);
    assert_eq!(trimmed_stat.trim(), diff_stat.trim());
    assert_eq!(sampled_patch.trim(), diff_patch.trim());
    assert_eq!(result.represented_files, 1);
    assert_eq!(result.total_files, 1);
}

#[test]
fn full_diff_path_excludes_suppressed_hunks_but_keeps_headers() {
    let diff_patch = [
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
    .join("\n");
    let diff_stat = " .sqlx/query-aaa.json | 2 +-\n src/app.rs | 2 +-\n 2 files changed\n";
    let suppressed =
        suppress_generated_content(&parse_diff_files(&diff_patch), &[".sqlx".to_string()]);
    let filtered_patch = rebuild_patch_from_files(&suppressed.files);

    let (_trimmed_stat, sampled_patch, result) = prepare_diff_for_prompt(
        &suppressed.files,
        diff_stat,
        &filtered_patch,
        DiffBudget {
            configured_tokens: 6000,
            effective_tokens: 6000,
        },
    )
    .expect("diff prep");

    assert!(!result.sampled);
    assert_eq!(result.total_files, 2);
    assert!(sampled_patch.contains("diff --git a/.sqlx/query-aaa.json"));
    assert!(sampled_patch.contains(DIFF_SUPPRESSED_CONTENT_NOTICE.trim()));
    assert!(!sampled_patch.contains("SELECT 1"));
    assert!(!sampled_patch.contains("SELECT 2"));
    assert!(sampled_patch.contains("+new-app"));
}

#[test]
fn token_mode_samples_with_token_budget() {
    let diff = build_multi_file_diff(&["alpha.txt", "beta.txt", "gamma.txt"], 30);
    let files = parse_diff_files(&diff);
    let tokenizer = Tokenizer::new().expect("tokenizer");
    let budget = tokenizer.count(DIFF_SAMPLING_NOTICE) + 90;

    let (sampled, represented, truncated) =
        sample_diff_patch(&files, &diff, budget, Some(&tokenizer));

    assert!(truncated);
    assert!(represented >= 1);
    assert!(sampled.contains("alpha.txt"));
    assert!(sampled.contains(DIFF_SAMPLING_NOTICE.trim()));
}

#[test]
fn token_mode_falls_back_to_notice_when_context_leaves_no_diff_space() {
    let diff_stat = " alpha.txt | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n";
    let diff_patch = [
        "diff --git a/alpha.txt b/alpha.txt",
        "index 111..222 100644",
        "--- a/alpha.txt",
        "+++ b/alpha.txt",
        "@@ -1 +1 @@",
        "-old",
        "+new",
        "",
    ]
    .join("\n");
    let files = parse_diff_files(&diff_patch);

    let (_trimmed_stat, sampled_patch, result) = prepare_diff_for_prompt(
        &files,
        diff_stat,
        &diff_patch,
        DiffBudget {
            configured_tokens: 1000,
            effective_tokens: 0,
        },
    )
    .expect("diff prep");

    assert!(result.sampled);
    assert_eq!(sampled_patch, DIFF_SAMPLING_NOTICE.trim());
}

#[test]
fn token_budget_is_clamped_by_context() {
    let budget = resolve_diff_budget(
        DiffBudgetConfig {
            max_tokens: 100_000,
            model_context_tokens: Some(10_000),
        },
        "repo",
        "main",
        10,
    )
    .expect("resolved budget");

    assert_eq!(budget.configured_tokens, 100_000);
    assert!(budget.effective_tokens < budget.configured_tokens);
}

fn build_multi_file_diff(files: &[&str], repeat: usize) -> String {
    let mut sections = Vec::new();
    for file in files {
        sections.extend([
            format!("diff --git a/{file} b/{file}"),
            "index 111..222 100644".to_string(),
            format!("--- a/{file}"),
            format!("+++ b/{file}"),
            "@@ -1 +1 @@".to_string(),
            format!("-old-{file}"),
            format!("+new-{file}"),
            repeat_patch_line(&format!("+context-{file}"), repeat),
        ]);
    }
    sections.push(String::new());
    sections.join("\n")
}

fn repeat_patch_line(prefix: &str, repeat: usize) -> String {
    std::iter::repeat_n(prefix, repeat)
        .collect::<Vec<_>>()
        .join("\n")
}
