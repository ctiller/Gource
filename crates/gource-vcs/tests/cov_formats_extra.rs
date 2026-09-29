use gource_vcs::commit::Commit;
use gource_vcs::formats;
use gource_vcs::options::VcsOptions;

fn make_reader(lines: Vec<String>) -> impl FnMut(&mut String) -> bool {
    let mut idx = 0;
    move |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].clone();
            idx += 1;
            true
        } else {
            false
        }
    }
}

#[test]
fn test_cvs_exp_branch_syntax_variants() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. Valid commit number followed by branch line, but EOF right after branch line
    let lines_branch_eof = vec!["000001:".to_string(), "BRANCH [main]".to_string()];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_eof),
        &mut commit,
        &opts
    ));

    // 2. Valid commit number followed by branch line, then non-empty line (expected empty line)
    let lines_branch_nonempty = vec![
        "000001:".to_string(),
        "BRANCH [main]".to_string(),
        "non-empty-line".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_nonempty),
        &mut commit,
        &opts
    ));

    // 3. Valid commit number followed by branch line, empty line, then EOF
    let lines_branch_empty_eof = vec![
        "000001:".to_string(),
        "BRANCH [main]".to_string(),
        "".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_empty_eof),
        &mut commit,
        &opts
    ));

    // 4. Valid commit with BRANCH [tag] followed by empty line, then date, entry, blank, msg, end
    let lines_branch_full = vec![
        "000001:".to_string(),
        "BRANCH [vendor-branch]".to_string(),
        "".to_string(),
        "(date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1)".to_string(),
        "| file.txt,v:1.1".to_string(),
        "".to_string(),
        "commit message".to_string(),
        "".to_string(),
        "=============================================================================".to_string(),
    ];
    assert!(formats::cvs_exp::parse_commit(
        make_reader(lines_branch_full),
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "alice");
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/file.txt");
}

#[test]
fn test_gitraw_commit_add_file_covered() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = vec![
        "commit 0123456789abcdef".to_string(),
        "tree fedcba9876543210".to_string(),
        "parent 1111222233334444".to_string(),
        "author Alice <alice@example.com> 1577836800 +0000".to_string(),
        "committer Alice <alice@example.com> 1577836800 +0000".to_string(),
        "".to_string(),
        "Initial commit message".to_string(),
        "".to_string(),
        ":100644 100644 0000000 1234567 A\tsrc/main.rs".to_string(),
        ":100644 100644 1234567 2345678 M\tsrc/lib.rs".to_string(),
        ":100644 000000 2345678 0000000 D\tREADME.md".to_string(),
        "".to_string(),
    ];

    assert!(formats::gitraw::parse_commit(
        make_reader(lines),
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Alice");
    assert_eq!(commit.timestamp, 1577836800);
    assert_eq!(commit.files.len(), 3);
    assert_eq!(commit.files[0].filename, "/src/main.rs");
    assert_eq!(commit.files[0].action, gource_vcs::FileAction::Add);
    assert_eq!(commit.files[1].filename, "/src/lib.rs");
    assert_eq!(commit.files[1].action, gource_vcs::FileAction::Modify);
    assert_eq!(commit.files[2].filename, "/README.md");
    assert_eq!(commit.files[2].action, gource_vcs::FileAction::Delete);
}
