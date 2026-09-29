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
fn test_cvs2cl_edge_cases() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. XML root without entry and no child entry (line 44)
    let lines2 = vec![
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>".to_string(),
        "<changelog>".to_string(),
    ];
    assert!(!formats::cvs2cl::parse_commit(
        &mut make_reader(lines2),
        &mut commit,
        &opts
    ));

    // 2. Invalid XML syntax after entry start (line 68)
    let lines3 = vec![
        "<entry>".to_string(),
        "<invalid unclosed xml".to_string(),
        "</entry>".to_string(),
    ];
    assert!(!formats::cvs2cl::parse_commit(
        &mut make_reader(lines3),
        &mut commit,
        &opts
    ));

    // 3. XML with empty author and dead file
    let xml = r#"<entry>
<isoDate>2020-01-01T12:00:00Z</isoDate>
<author></author>
<file>
  <name>valid.txt</name>
  <cvsstate>dead</cvsstate>
</file>
</entry>"#;
    let lines: Vec<String> = xml.lines().map(|s| s.to_string()).collect();
    assert!(formats::cvs2cl::parse_commit(
        &mut make_reader(lines),
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Unknown");
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/valid.txt");
}

#[test]
fn test_cvs_exp_edge_cases() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. EOF immediately after empty line (line 40)
    let lines_eof_empty = vec!["".to_string()];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_eof_empty),
        &mut commit,
        &opts
    ));

    // 2. Branch line followed by EOF (line 55)
    let lines_branch_eof = vec![
        "----------------------------".to_string(),
        "revision 1.1".to_string(),
        "branch: 1.1.1;".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_eof),
        &mut commit,
        &opts
    ));

    // 3. Branch line followed by non-empty line (line 58)
    let lines_branch_nonempty = vec![
        "----------------------------".to_string(),
        "revision 1.1".to_string(),
        "branch: 1.1.1;".to_string(),
        "not-empty-line".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_nonempty),
        &mut commit,
        &opts
    ));

    // 4. Branch line followed by empty line, then EOF (line 61)
    let lines_branch_empty_eof = vec![
        "----------------------------".to_string(),
        "revision 1.1".to_string(),
        "branch: 1.1.1;".to_string(),
        "".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_empty_eof),
        &mut commit,
        &opts
    ));

    // 5. EOF after detail line (line 123)
    let lines_detail_eof = vec![
        "----------------------------".to_string(),
        "revision 1.1".to_string(),
        "date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1;".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_detail_eof),
        &mut commit,
        &opts
    ));

    // 6. Branch line valid followed by date and entries, but EOF before commit message ends (line 143)
    let lines_branch_valid = vec![
        "----------------------------".to_string(),
        "revision 1.1".to_string(),
        "branch: 1.1.1;".to_string(),
        "".to_string(),
        "date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1;".to_string(),
        "file1.txt".to_string(),
    ];
    assert!(!formats::cvs_exp::parse_commit(
        make_reader(lines_branch_valid),
        &mut commit,
        &opts
    ));
}

#[test]
fn test_svn_edge_cases() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. XML tag present, but EOF before SVN_LOGENTRY_START (<logentry) (line 66)
    let lines2 = vec![
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>".to_string(),
        "<log>".to_string(),
    ];
    assert!(!formats::svn::parse_commit(
        &mut make_reader(lines2),
        &mut commit,
        &opts
    ));

    // 2. Invalid XML syntax after logentry start (line 90)
    let lines3 = vec![
        "<logentry revision=\"1\">".to_string(),
        "<unclosed tag".to_string(),
        "</logentry>".to_string(),
    ];
    assert!(!formats::svn::parse_commit(
        &mut make_reader(lines3),
        &mut commit,
        &opts
    ));

    // 3. XML with empty author and deleted directory path ending with /
    let xml = r#"<logentry revision="123">
  <author></author>
  <date>2020-01-01T12:00:00.000000Z</date>
  <paths>
    <path action="D" kind="dir">/trunk/dir/</path>
  </paths>
  <msg>log message</msg>
</logentry>"#;
    let lines: Vec<String> = xml.lines().map(|s| s.to_string()).collect();
    assert!(formats::svn::parse_commit(
        &mut make_reader(lines),
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Unknown");
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/trunk/dir/");
}

#[test]
fn test_git_and_gitraw_edge_cases() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. gitraw EOF after commit line (line 44)
    let lines_gitraw_tree_eof = vec!["commit 0123456789abcdef".to_string()];
    assert!(!formats::gitraw::parse_commit(
        make_reader(lines_gitraw_tree_eof),
        &mut commit,
        &opts
    ));

    // 2. git parser quoted file with length <= 2 e.g. "" (line 131)
    let git_quoted_empty = vec![
        "user:Alice".to_string(),
        "1577836800".to_string(),
        "M\t\"\"".to_string(),
        "M\t\"valid.txt\"".to_string(),
        "".to_string(),
    ];
    assert!(formats::git::parse_commit(
        make_reader(git_quoted_empty),
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/valid.txt");
}

#[test]
fn test_custom_parser_edge_cases() {
    // atoll with whitespace and +/- signs (lines 45, 46, 53, 54)
    assert_eq!(formats::custom::atoll("  \t\n\r +123"), 123);
    assert_eq!(formats::custom::atoll("  -456"), -456);

    // parse_date_time with missing hour/min (lines 80, 84)
    let ts = formats::custom::parse_date_time("2020-01-01");
    assert!(ts.is_some());
}
