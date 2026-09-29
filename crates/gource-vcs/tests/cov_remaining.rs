use gource_vcs::commit::Commit;
use gource_vcs::options::VcsOptions;
use std::fs::File;
use std::io::Write;
use std::path::Path;

#[test]
fn cvs2cl_entry_nested_in_changelog_root_is_found() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // XML fragment starting with <?xml to satisfy CVS2CL_XML_TAG
    let lines = [
        "<?xml version=\"1.0\"?>",
        "<changelog>",
        "<entry>",
        "<date>2020-01-01</date>",
        "<time>12:00:00</time>",
        "<isoDate>2020-01-01T12:00:00Z</isoDate>",
        "<author>Alice</author>",
        "<file>",
        "<name>file.txt</name>",
        "<cvsstate>Exp</cvsstate>",
        "</file>",
        "</entry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::cvs2cl::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Alice");
    assert_eq!(commit.files.len(), 1);
}

#[test]
fn cvs2cl_non_entry_root_without_entry_returns_false() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // XML root is <changelog> but has no <entry> children inside before </entry>
    let lines = ["<changelog>", "<other>foo</other>", "</entry>"];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs2cl::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
}

#[test]
fn cvs2cl_invalid_time_returns_false() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // Valid date, invalid hour 25
    let lines = [
        "<entry>",
        "<date>2020-01-01</date>",
        "<time>25:00:00</time>",
        "<isoDate>2020-01-01T25:00:00Z</isoDate>",
        "<author>Alice</author>",
        "</entry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs2cl::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
}

#[test]
fn cvs2cl_empty_author_falls_back_to_unknown() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = [
        "<entry>",
        "<date>2020-01-01</date>",
        "<time>12:00:00</time>",
        "<isoDate>2020-01-01T12:00:00Z</isoDate>",
        "<author></author>",
        "<file>",
        "<name>file.txt</name>",
        "<cvsstate>Exp</cvsstate>",
        "</file>",
        "</entry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::cvs2cl::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.username, "Unknown");
}

#[test]
fn cvs2cl_file_with_empty_name_or_empty_cvsstate_is_skipped() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = [
        "<entry>",
        "<date>2020-01-01</date>",
        "<time>12:00:00</time>",
        "<isoDate>2020-01-01T12:00:00Z</isoDate>",
        "<author>Alice</author>",
        "<file>",
        "<name></name>",
        "<cvsstate>Exp</cvsstate>",
        "</file>",
        "<file>",
        "<name>test.txt</name>",
        "<cvsstate></cvsstate>",
        "</file>",
        "<file>",
        "<name>valid.txt</name>",
        "<cvsstate>Exp</cvsstate>",
        "</file>",
        "</entry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::cvs2cl::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/valid.txt");
}

#[test]
fn cvs_exp_branch_line_branches() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. Branch line followed by EOF
    let lines1 = ["000001:", "branch: main;"];
    let mut idx1 = 0;
    let get_line1 = |l: &mut String| {
        if idx1 < lines1.len() {
            *l = lines1[idx1].to_string();
            idx1 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line1,
        &mut commit,
        &opts
    ));

    // 2. Branch line followed by non-empty line
    let lines2 = ["000001:", "branch: main;", "non empty"];
    let mut idx2 = 0;
    let get_line2 = |l: &mut String| {
        if idx2 < lines2.len() {
            *l = lines2[idx2].to_string();
            idx2 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line2,
        &mut commit,
        &opts
    ));

    // 3. Branch line followed by blank line, then EOF
    let lines3 = ["000001:", "branch: main;", ""];
    let mut idx3 = 0;
    let get_line3 = |l: &mut String| {
        if idx3 < lines3.len() {
            *l = lines3[idx3].to_string();
            idx3 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line3,
        &mut commit,
        &opts
    ));
}

#[test]
fn cvs_exp_invalid_time_returns_false() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = [
        "000001:",
        "(date: 2020/01/01 25:61:00;  author: alice;  state: Exp;  lines: +1 -1)",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
}

#[test]
fn cvs_exp_log_ends_at_various_stages() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. Log ends right after date line (line 118)
    let lines1 = [
        "000001:",
        "(date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1)",
    ];
    let mut idx1 = 0;
    let get_line1 = |l: &mut String| {
        if idx1 < lines1.len() {
            *l = lines1[idx1].to_string();
            idx1 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line1,
        &mut commit,
        &opts
    ));

    // 2. Log ends inside file list (line 129)
    let lines2 = [
        "000001:",
        "(date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1)",
        "| file.txt,v:1.1",
    ];
    let mut idx2 = 0;
    let get_line2 = |l: &mut String| {
        if idx2 < lines2.len() {
            *l = lines2[idx2].to_string();
            idx2 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line2,
        &mut commit,
        &opts
    ));

    // 3. Log ends at the blank line after file list (line 138)
    let lines3 = [
        "000001:",
        "(date: 2020/01/01 12:00:00;  author: alice;  state: Exp;  lines: +1 -1)",
        "| file.txt,v:1.1",
        "",
    ];
    let mut idx3 = 0;
    let get_line3 = |l: &mut String| {
        if idx3 < lines3.len() {
            *l = lines3[idx3].to_string();
            idx3 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::cvs_exp::parse_commit(
        get_line3,
        &mut commit,
        &opts
    ));
}

#[test]
fn git_file_line_without_tab_continues() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // After user: and timestamp lines, provide a line without a tab, followed by a valid file line
    let lines = [
        "user:Alice",
        "1577836800",
        "line_without_tab",
        ":100644 100644 1 2 M\tvalid.txt",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::git::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/valid.txt");
}

#[test]
fn gitraw_line_94_file_action_and_options() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // gitraw parse_commit with files
    let lines = [
        "commit 123",
        "tree 456",
        "author Alice <a@b> 1577836800 +0000",
        "committer Alice <a@b> 1577836800 +0000",
        "",
        "Commit msg",
        "",
        ":000000 100644 0000000 1234567 A\tfile.txt",
        "",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::gitraw::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/file.txt");
}

#[test]
fn hg_timestamp_overflow_fails() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // Overflowing i64 timestamp
    let line = "99999999999999999999 Alice\tM file.txt";
    assert!(
        gource_vcs::formats::hg::HgParser::parse_commit_entry(line, &mut commit, &opts).is_err()
    );
}

#[test]
fn svn_root_not_logentry_searches_children() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    // 1. Root element is <log>, containing <logentry>
    let lines1 = [
        "<?xml version=\"1.0\"?>",
        "<log>",
        "<logentry revision=\"1\">",
        "<date>2020-01-01T12:00:00.000000Z</date>",
        "<paths>",
        "<path action=\"M\">/main.rs</path>",
        "</paths>",
        "</logentry>",
    ];
    let mut idx1 = 0;
    let get_line1 = |l: &mut String| {
        if idx1 < lines1.len() {
            *l = lines1[idx1].to_string();
            idx1 += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::svn::parse_commit(
        get_line1,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);

    // 2. Root element is <log>, but has no <logentry> children before </logentry>
    let lines2 = ["<log>", "<other>test</other>", "</logentry>"];
    let mut idx2 = 0;
    let get_line2 = |l: &mut String| {
        if idx2 < lines2.len() {
            *l = lines2[idx2].to_string();
            idx2 += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::svn::parse_commit(
        get_line2,
        &mut commit,
        &opts
    ));
}

#[test]
fn svn_invalid_time_returns_false() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = [
        "<logentry revision=\"1\">",
        "<date>2020-01-01T25:61:00.000000Z</date>",
        "</logentry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(!gource_vcs::formats::svn::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
}

#[test]
fn svn_empty_path_or_action_skipped() {
    let opts = VcsOptions::default();
    let mut commit = Commit::default();

    let lines = [
        "<logentry revision=\"1\">",
        "<date>2020-01-01T12:00:00.000000Z</date>",
        "<paths>",
        "<path action=\"\">/empty_action.txt</path>",
        "<path action=\"M\"></path>",
        "<path action=\"M\">/valid.txt</path>",
        "</paths>",
        "</logentry>",
    ];
    let mut idx = 0;
    let get_line = |l: &mut String| {
        if idx < lines.len() {
            *l = lines[idx].to_string();
            idx += 1;
            true
        } else {
            false
        }
    };
    assert!(gource_vcs::formats::svn::parse_commit(
        get_line,
        &mut commit,
        &opts
    ));
    assert_eq!(commit.files.len(), 1);
    assert_eq!(commit.files[0].filename, "/valid.txt");
}

#[test]
fn write_custom_log_error_on_dev_full() {
    // Only run on Linux where /dev/full exists and produces ENOSPC on write
    if Path::new("/dev/full").exists() {
        let temp_in = tempfile::NamedTempFile::new().unwrap();
        let mut in_file = File::create(temp_in.path()).unwrap();
        writeln!(in_file, "1000|Alice|A|/a.txt").unwrap();
        drop(in_file);

        let opts = VcsOptions {
            log_format: "custom".to_string(),
            ..VcsOptions::default()
        };

        let res =
            gource_vcs::write_custom_log(temp_in.path().to_str().unwrap(), "/dev/full", &opts);
        assert!(res.is_err());
    }
}
