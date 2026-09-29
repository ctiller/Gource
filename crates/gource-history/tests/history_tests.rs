use gource_history::*;

#[test]
fn test_intern_tables_paths_users_cohorts() {
    let mut pt = PathTable::new();
    assert!(pt.is_empty());
    assert_eq!(pt.len(), 0);

    // Normalization
    assert_eq!(PathTable::normalize_path(""), "/");
    assert_eq!(PathTable::normalize_path("   "), "/");
    assert_eq!(PathTable::normalize_path("src/main.rs"), "/src/main.rs");
    assert_eq!(PathTable::normalize_path("///a//b///c.txt"), "/a/b/c.txt");

    let id1 = pt.intern("src/main.rs");
    let id2 = pt.intern("/src/main.rs");
    assert_eq!(id1, id2);
    assert_eq!(pt.len(), 1);
    assert!(!pt.is_empty());

    let entry1 = pt.get(id1).unwrap();
    assert_eq!(entry1.path, "/src/main.rs");
    assert_eq!(entry1.name, "main.rs");
    assert_eq!(entry1.dir, "/src/");
    assert_eq!(entry1.ext, "rs");
    assert!(entry1.colour.length() > 0.0);
    assert_eq!(pt.resolve(id1), Some("/src/main.rs"));
    assert_eq!(pt.find("src/main.rs"), Some(id1));
    assert_eq!(pt.find("nonexistent"), None);

    // Root-level file without dir
    let id_root = pt.intern("/Cargo.toml");
    let entry_root = pt.get(id_root).unwrap();
    assert_eq!(entry_root.dir, "/");
    assert_eq!(entry_root.name, "Cargo.toml");
    assert_eq!(entry_root.ext, "toml");

    // Dotfiles and files without extensions
    let id_dotfile = pt.intern("/.gitignore");
    assert_eq!(pt.get(id_dotfile).unwrap().ext, "");
    let id_noext = pt.intern("/Makefile");
    assert_eq!(pt.get(id_noext).unwrap().ext, "");
    let id_traildot = pt.intern("/test.");
    assert_eq!(pt.get(id_traildot).unwrap().ext, "");

    // UserTable
    let mut ut = UserTable::new();
    assert!(ut.is_empty());
    let u1 = ut.intern("Alice");
    let u2 = ut.intern("  Alice  ");
    let u3 = ut.intern("Bob");
    assert_eq!(u1, u2);
    assert_ne!(u1, u3);
    assert_eq!(ut.len(), 2);
    assert_eq!(ut.get(u1), Some("Alice"));
    assert_eq!(ut.find("Bob"), Some(u3));
    assert_eq!(ut.find("Charlie"), None);

    // CohortTable
    let mut ct = CohortTable::new();
    assert!(ct.is_empty());
    let c1 = ct.intern("2024");
    let c2 = ct.intern(" 2024 ");
    let c3 = ct.intern("2025");
    assert_eq!(c1, c2);
    assert_ne!(c1, c3);
    assert_eq!(ct.len(), 2);
    assert_eq!(ct.get(c1), Some("2024"));
    assert_eq!(ct.find("2025"), Some(c3));
    assert_eq!(ct.find("2026"), None);
}

#[test]
fn test_theseus_cohort_modes_and_decay_models() {
    let mode_year = CohortMode::Year;
    let mode_quarter = CohortMode::Quarter;
    let mode_author = CohortMode::Author;

    // 2024-02-15 (Q1): 1707955200
    let ts_q1 = 1707955200;
    // 2024-05-15 (Q2): 1715731200
    let ts_q2 = 1715731200;
    // 2024-08-15 (Q3): 1723680000
    let ts_q3 = 1723680000;
    // 2024-11-15 (Q4): 1731628800
    let ts_q4 = 1731628800;

    assert_eq!(mode_year.cohort_label(ts_q1, "Alice"), "2024");
    assert_eq!(mode_quarter.cohort_label(ts_q1, "Alice"), "2024-Q1");
    assert_eq!(mode_quarter.cohort_label(ts_q2, "Alice"), "2024-Q2");
    assert_eq!(mode_quarter.cohort_label(ts_q3, "Alice"), "2024-Q3");
    assert_eq!(mode_quarter.cohort_label(ts_q4, "Alice"), "2024-Q4");
    assert_eq!(mode_author.cohort_label(ts_q1, "Alice"), "Alice");

    // FileCohorts LifoYoungestFirst
    let mut fc_lifo = FileCohorts::new();
    assert_eq!(fc_lifo.total_lines(), 0);
    assert_eq!(fc_lifo.churn_temperature(), 0.0);

    fc_lifo.add_lines(CohortId(1), 100);
    fc_lifo.add_lines(CohortId(2), 50);
    fc_lifo.add_lines(CohortId(3), 25);
    assert_eq!(fc_lifo.total_lines(), 175);

    // Remove 0 lines
    let removed0 = fc_lifo.remove_lines(0, ChurnDecayModel::LifoYoungestFirst);
    assert!(removed0.is_empty());
    assert_eq!(fc_lifo.total_lines(), 175);

    // Remove 30 lines: drains all 25 of cohort 3 and 5 of cohort 2
    let removed = fc_lifo.remove_lines(30, ChurnDecayModel::LifoYoungestFirst);
    assert_eq!(removed, vec![(CohortId(3), 25), (CohortId(2), 5)]);
    assert_eq!(fc_lifo.total_lines(), 145);
    assert_eq!(fc_lifo.buckets, vec![(CohortId(1), 100), (CohortId(2), 45)]);

    // Churn temperature
    assert!(fc_lifo.churn_temperature() > 0.0);
    assert!(fc_lifo.churn_temperature() <= 1.0);

    // Remove all remaining
    let removed_all = fc_lifo.remove_lines(200, ChurnDecayModel::LifoYoungestFirst);
    assert_eq!(removed_all, vec![(CohortId(1), 100), (CohortId(2), 45)]);
    assert_eq!(fc_lifo.total_lines(), 0);
    assert!(fc_lifo.buckets.is_empty());

    // FileCohorts Proportional with remainder distribution
    let mut fc_prop = FileCohorts::new();
    fc_prop.add_lines(CohortId(1), 100);
    fc_prop.add_lines(CohortId(2), 50);
    fc_prop.add_lines(CohortId(3), 25);
    assert_eq!(fc_prop.total_lines(), 175);

    // Remove 35 lines -> 140 lines remaining
    let deducted = fc_prop.remove_lines(35, ChurnDecayModel::Proportional);
    let total_deducted: u32 = deducted.iter().map(|(_, l)| *l).sum();
    assert_eq!(total_deducted, 35);
    assert_eq!(fc_prop.total_lines(), 140);
    // Invariant: sum of remaining cohort lines equals exact total lines
    let sum_buckets: u32 = fc_prop.buckets.iter().map(|(_, l)| *l).sum();
    assert_eq!(sum_buckets, 140);

    // Survival points & Half-Life
    let sp1 = SurvivalPoint {
        age_seconds: 0,
        surviving_lines: 100,
        initial_lines: 100,
    };
    let sp2 = SurvivalPoint {
        age_seconds: 1000,
        surviving_lines: 75,
        initial_lines: 100,
    };
    let sp3 = SurvivalPoint {
        age_seconds: 2000,
        surviving_lines: 25,
        initial_lines: 100,
    };
    assert_eq!(sp1.survival_fraction(), 1.0);
    assert_eq!(sp2.survival_fraction(), 0.75);
    assert_eq!(sp3.survival_fraction(), 0.25);

    let half_life = estimate_half_life(&[sp1, sp2, sp3]).unwrap();
    // Linear interpolation between 1000s (0.75) and 2000s (0.25) -> exactly 1500s!
    assert!((half_life - 1500.0).abs() < 1e-4);

    // Never drops to 50%
    assert_eq!(estimate_half_life(&[sp1, sp2]), None);
    assert_eq!(estimate_half_life(&[sp1]), None);
}

#[test]
fn test_snapshot_operations() {
    let mut snap = TreeSnapshot::new();
    assert_eq!(snap.total_files(), 0);
    assert_eq!(snap.total_lines(), 0);
    assert_eq!(snap.total_bytes(), 0);
    assert!(snap.stacked_cohorts().is_empty());
    assert_eq!(snap.dominant_cohort(), None);

    let f1 = LiveFileState::new(PathId(5), 120, 4200, UserId(0), 1000, CohortId(1));
    let f2 = LiveFileState::new(PathId(2), 80, 2800, UserId(1), 1000, CohortId(2));
    let f3 = LiveFileState::new(PathId(9), 200, 7000, UserId(0), 1000, CohortId(1));

    snap.insert_or_replace(f1);
    snap.insert_or_replace(f2);
    snap.insert_or_replace(f3);

    // Sorted by PathId: 2, 5, 9
    assert_eq!(snap.files.len(), 3);
    assert_eq!(snap.files[0].path, PathId(2));
    assert_eq!(snap.files[1].path, PathId(5));
    assert_eq!(snap.files[2].path, PathId(9));

    assert_eq!(snap.total_files(), 3);
    assert_eq!(snap.total_lines(), 400);
    assert_eq!(snap.total_bytes(), 14000);

    // Binary search lookup
    assert_eq!(snap.file(PathId(5)).unwrap().lines, 120);
    assert!(snap.file(PathId(10)).is_none());

    // Mutable lookup & modify
    if let Some(f) = snap.file_mut(PathId(5)) {
        f.lines = 150;
    }
    assert_eq!(snap.file(PathId(5)).unwrap().lines, 150);

    // Cohort totals
    snap.add_cohort_lines(CohortId(1), 320);
    snap.add_cohort_lines(CohortId(2), 80);
    assert_eq!(snap.dominant_cohort(), Some(CohortId(1)));
    assert_eq!(
        snap.stacked_cohorts(),
        vec![(CohortId(1), 320), (CohortId(2), 80)]
    );

    snap.deduct_cohort_lines(CohortId(1), 20);
    assert_eq!(
        snap.stacked_cohorts(),
        vec![(CohortId(1), 300), (CohortId(2), 80)]
    );

    // Remove file
    let removed = snap.remove_file(PathId(5));
    assert!(removed.is_some());
    assert_eq!(snap.total_files(), 2);
    assert!(snap.file(PathId(5)).is_none());
    assert!(snap.remove_file(PathId(99)).is_none());
}

#[test]
fn test_history_building_replaying_and_property_verification() {
    let mut builder = HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst);

    // Generate 150 commits across 3 distinct years, touching multiple files
    let base_ts = 1609459200; // 2021-01-01
    let num_commits = 150;

    for i in 0..num_commits {
        let ts = base_ts + i * 86400 * 5; // spans 2021, 2022, 2023
        let user = match i % 3 {
            0 => "Alice",
            1 => "Bob",
            _ => "Charlie",
        };

        let file_num = (i % 10) + 1;
        let file_path = format!("src/module_{file_num}.rs");

        let files = if i < 15 {
            vec![FileChangeInput {
                path: file_path,
                op: ChangeOp::Add,
                lines_added: 50 + (i as u32 * 5),
                lines_removed: 0,
                byte_size: Some(1500),
                is_binary: false,
            }]
        } else if i % 12 == 0 {
            vec![FileChangeInput {
                path: file_path,
                op: ChangeOp::Delete,
                lines_added: 0,
                lines_removed: 20,
                byte_size: None,
                is_binary: false,
            }]
        } else {
            vec![FileChangeInput {
                path: file_path,
                op: ChangeOp::Modify,
                lines_added: 15,
                lines_removed: 5,
                byte_size: None,
                is_binary: false,
            }]
        };

        builder.add_commit(CommitInput {
            timestamp: ts,
            username: user.to_string(),
            files,
        });
    }

    let history = builder.finish();
    assert_eq!(history.commit_count(), num_commits as usize);
    assert!(!history.is_empty());
    assert!(history.snapshots.len() >= 3); // Stride 64 stores at 0, 64, 128, plus finish

    // Verify property: replaying from nearest snapshot at every commit index `c`
    // produces the EXACT state as replaying from scratch!
    for c in 0..num_commits as usize {
        let fast_state = history.state_at_commit(c);
        let metrics = history.metrics_at_commit(c).unwrap();

        assert_eq!(fast_state.commit_index, c);
        assert_eq!(fast_state.total_files() as u32, metrics.total_files);
        assert_eq!(fast_state.total_lines(), metrics.total_lines);

        // Invariant: sum of live file lines == sum of cohort lines == metrics total lines
        let sum_file_lines: u64 = fast_state.files.iter().map(|f| f.lines as u64).sum();
        let sum_cohort_lines: u64 = fast_state.cohort_totals.iter().sum();
        assert_eq!(sum_file_lines, metrics.total_lines);
        assert_eq!(sum_cohort_lines, metrics.total_lines);
    }

    // State at timestamp binary search
    let mid_ts = base_ts + 50 * 86400 * 5;
    let snap_at_ts = history.state_at_timestamp(mid_ts);
    assert!(snap_at_ts.is_some());
    assert_eq!(snap_at_ts.unwrap().timestamp, mid_ts);

    // Timestamp before history
    assert!(history.state_at_timestamp(base_ts - 1000).is_none());
    // Timestamp after history
    let snap_after = history
        .state_at_timestamp(base_ts + 500 * 86400 * 5)
        .unwrap();
    assert_eq!(snap_after.commit_index, num_commits as usize - 1);

    // Sliding window summary
    let window_30d = history.window_summary(num_commits as usize - 1, 30 * 86400);
    assert!(window_30d.commits_in_window > 0);
    assert!(window_30d.distinct_active_editors > 0);
    assert!(!window_30d.top_editors.is_empty());

    // Window on out of bounds commit
    let empty_win = history.window_summary(9999, 1000);
    assert_eq!(empty_win.commits_in_window, 0);

    // Cohort survival & half-life
    let c0 = CohortId(0);
    let survival = history.cohort_survival(c0);
    assert!(!survival.is_empty());
    assert!(survival[0].survival_fraction() > 0.0);
    assert!(survival[0].survival_fraction() <= 1.0);

    // Nonexistent cohort
    assert!(history.cohort_survival(CohortId(999)).is_empty());
    assert_eq!(history.cohort_half_life(CohortId(999)), None);

    // Export CSV
    let csv = history.export_csv();
    assert!(csv.starts_with("commit,timestamp,total_files,total_lines,lines_added,lines_removed,active_editors_30d,dominant_cohort\n"));
    let lines_count = csv.lines().count();
    assert_eq!(lines_count, num_commits as usize + 1);
}

#[test]
fn test_cache_roundtrip_atomic_persistence_and_corruptions() {
    let mut builder = HistoryBuilder::new(CohortMode::Quarter, ChurnDecayModel::Proportional);
    builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Alice".to_string(),
        files: vec![FileChangeInput {
            path: "src/main.rs".to_string(),
            op: ChangeOp::Add,
            lines_added: 80,
            lines_removed: 0,
            byte_size: Some(2500),
            is_binary: false,
        }],
    });
    builder.add_commit(CommitInput {
        timestamp: 1609545600,
        username: "Bob".to_string(),
        files: vec![FileChangeInput {
            path: "src/main.rs".to_string(),
            op: ChangeOp::Modify,
            lines_added: 20,
            lines_removed: 10,
            byte_size: Some(2800),
            is_binary: false,
        }],
    });
    let history = builder.finish();

    let cache_key = "repo_test_key_v1";
    let bytes = history.save_to_bytes(cache_key);

    // Roundtrip load from bytes
    let loaded = History::load_from_bytes(&bytes, cache_key).expect("roundtrip load succeeded");
    assert_eq!(loaded.commit_count(), 2);
    assert_eq!(loaded.paths.len(), history.paths.len());
    assert_eq!(loaded.users.len(), history.users.len());
    assert_eq!(loaded.cohorts.len(), history.cohorts.len());
    assert_eq!(loaded.metrics, history.metrics);

    // Key mismatch
    let err_key = History::load_from_bytes(&bytes, "wrong_key");
    assert!(matches!(err_key, Err(CacheError::KeyMismatch { .. })));

    // Truncated buffer
    let err_eof = History::load_from_bytes(&bytes[..10], cache_key);
    assert!(matches!(err_eof, Err(CacheError::UnexpectedEof)));

    // Corrupted checksum (flip a byte in payload)
    let mut bad_payload = bytes.clone();
    bad_payload[20] ^= 0xFF;
    let err_chk = History::load_from_bytes(&bad_payload, cache_key);
    assert!(matches!(err_chk, Err(CacheError::ChecksumMismatch { .. })));

    // Invalid magic
    let mut bad_magic = bytes.clone();
    bad_magic[0] = b'X';
    let new_checksum = fnv1a_64(&bad_magic[..bad_magic.len() - 8]);
    let len = bad_magic.len();
    bad_magic[len - 8..].copy_from_slice(&new_checksum.to_le_bytes());
    let err_magic = History::load_from_bytes(&bad_magic, cache_key);
    assert!(matches!(err_magic, Err(CacheError::InvalidMagic { .. })));

    // Unsupported version
    let mut bad_version = bytes.clone();
    bad_version[7] = 2; // change version to 2
    let new_checksum = fnv1a_64(&bad_version[..bad_version.len() - 8]);
    let len = bad_version.len();
    bad_version[len - 8..].copy_from_slice(&new_checksum.to_le_bytes());
    let err_ver = History::load_from_bytes(&bad_version, cache_key);
    assert!(matches!(err_ver, Err(CacheError::UnsupportedVersion(2))));

    // Atomic file save and load
    let dir = tempfile::tempdir().unwrap();
    let cache_file = dir.path().join("history.bin");

    history
        .save_to_path(&cache_file, cache_key)
        .expect("save to path");
    assert!(cache_file.exists());

    let file_loaded = History::load_from_path(&cache_file, cache_key).expect("load from path");
    assert_eq!(file_loaded.commit_count(), 2);

    // Nonexistent file
    let bad_path = dir.path().join("does_not_exist.bin");
    assert!(matches!(
        History::load_from_path(&bad_path, cache_key),
        Err(CacheError::Io(_))
    ));
}

#[test]
fn test_edge_cases_and_coverage() {
    // 1. intern default
    let pt_def = PathTable::default();
    assert!(pt_def.is_empty());

    // 2. theseus edge cases
    let mut fc = FileCohorts::new();
    fc.add_lines(CohortId(1), 0); // 0 lines added
    assert_eq!(fc.total_lines(), 0);

    // Proportional remainder > 0
    fc.add_lines(CohortId(1), 10);
    fc.add_lines(CohortId(2), 10);
    fc.add_lines(CohortId(3), 10);
    assert_eq!(fc.total_lines(), 30);
    let deducted = fc.remove_lines(1, ChurnDecayModel::Proportional); // leaves 29 lines, 29 % 3 = 2 remainder!
    assert_eq!(fc.total_lines(), 29);
    let total_deducted: u32 = deducted.iter().map(|(_, l)| *l).sum();
    assert_eq!(total_deducted, 1);

    // SurvivalPoint with 0 initial lines
    let sp_zero = SurvivalPoint {
        age_seconds: 10,
        surviving_lines: 5,
        initial_lines: 0,
    };
    assert_eq!(sp_zero.survival_fraction(), 0.0);

    // estimate_half_life exact flat 0.50
    let sp_half1 = SurvivalPoint {
        age_seconds: 100,
        surviving_lines: 50,
        initial_lines: 100,
    };
    let sp_half2 = SurvivalPoint {
        age_seconds: 200,
        surviving_lines: 50,
        initial_lines: 100,
    };
    assert_eq!(estimate_half_life(&[sp_half1, sp_half2]), Some(100.0));

    // 3. snapshot dominant_cohort with all 0 lines
    let mut snap_zeros = TreeSnapshot::new();
    snap_zeros.add_cohort_lines(CohortId(0), 0);
    assert_eq!(snap_zeros.dominant_cohort(), None);

    // 4. empty history
    let empty_hist =
        HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst).finish();
    assert!(empty_hist.is_empty());
    assert_eq!(empty_hist.commit_count(), 0);
    assert!(empty_hist.commit_changes(0).is_empty());
    assert_eq!(empty_hist.state_at_commit(0).total_files(), 0);
    assert_eq!(empty_hist.window_summary(0, 1000).commits_in_window, 0);

    // 5. Duplicate timestamps in commits for binary search scanning
    let mut dup_builder =
        HistoryBuilder::new(CohortMode::Author, ChurnDecayModel::LifoYoungestFirst);
    // Commit with 0 files
    dup_builder.add_commit(CommitInput {
        timestamp: 1000,
        username: "Alice".to_string(),
        files: Vec::new(),
    });
    // Two commits at timestamp 2000
    dup_builder.add_commit(CommitInput {
        timestamp: 2000,
        username: "Bob".to_string(),
        files: vec![FileChangeInput {
            path: "foo.rs".to_string(),
            op: ChangeOp::Add,
            lines_added: 50,
            lines_removed: 0,
            byte_size: None,
            is_binary: false,
        }],
    });
    dup_builder.add_commit(CommitInput {
        timestamp: 2000,
        username: "Charlie".to_string(),
        files: vec![FileChangeInput {
            path: "foo.rs".to_string(),
            op: ChangeOp::Modify,
            lines_added: 10,
            lines_removed: 5,
            byte_size: None,
            is_binary: false,
        }],
    });
    // Commit with Delete
    dup_builder.add_commit(CommitInput {
        timestamp: 3000,
        username: "Alice".to_string(),
        files: vec![FileChangeInput {
            path: "foo.rs".to_string(),
            op: ChangeOp::Delete,
            lines_added: 0,
            lines_removed: 55,
            byte_size: None,
            is_binary: false,
        }],
    });
    let dup_hist = dup_builder.finish();

    // Query state_at_timestamp with exact duplicate timestamp
    let snap_at_2000 = dup_hist.state_at_timestamp(2000).unwrap();
    assert_eq!(snap_at_2000.commit_index, 2); // advances to the last commit at 2000

    // Query window_summary with exact start timestamp matching multiple commits
    let win_summary = dup_hist.window_summary(3, 1000); // 3000 - 1000 = 2000
    assert_eq!(win_summary.commits_in_window, 3); // includes commits at 2000 (Bob and Charlie) and 3000 (Alice)

    // Cohort survival where commit 0 is prior to cohort 1 birth, and cohort 0 has 0 lines
    let surv_c0 = dup_hist.cohort_survival(CohortId(0)); // Alice had 0 lines added
    assert!(surv_c0.is_empty());

    let surv_c1 = dup_hist.cohort_survival(CohortId(1)); // Bob born at commit 1
    assert_eq!(surv_c1.len(), 3); // commits 1, 2, 3 (commit 0 skipped)

    // Cache roundtrip with Year, Author, Delete, and empty commit files
    let dup_bytes = dup_hist.save_to_bytes("dup_key");
    let dup_loaded = History::load_from_bytes(&dup_bytes, "dup_key").expect("load dup");
    assert_eq!(dup_loaded.commit_count(), 4);

    // Corrupted payload checks in load_from_bytes:
    // Corrupt cohort_mode (offset after magic and key)
    // Find offset where cohort mode is written: magic (8) + key_len (4) + key ("dup_key".len() = 7) = 19
    let mut bad_mode = dup_bytes.clone();
    bad_mode[19] = 99; // invalid cohort mode
    let len = bad_mode.len();
    let new_chk = fnv1a_64(&bad_mode[..len - 8]);
    bad_mode[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_mode, "dup_key"),
        Err(CacheError::Corrupted(_))
    ));

    // Corrupt decay_model (offset 20)
    let mut bad_decay = dup_bytes.clone();
    bad_decay[20] = 99; // invalid decay model
    let len = bad_decay.len();
    let new_chk = fnv1a_64(&bad_decay[..len - 8]);
    bad_decay[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_decay, "dup_key"),
        Err(CacheError::Corrupted(_))
    ));

    // Test Year + Lifo cache roundtrip
    let mut y_builder = HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst);
    y_builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Dave".to_string(),
        files: vec![
            FileChangeInput {
                path: "a.rs".to_string(),
                op: ChangeOp::Add,
                lines_added: 10,
                lines_removed: 0,
                byte_size: Some(300),
                is_binary: false,
            },
            FileChangeInput {
                path: "b.rs".to_string(),
                op: ChangeOp::Delete,
                lines_added: 0,
                lines_removed: 10,
                byte_size: None,
                is_binary: false,
            },
        ],
    });
    // Commit with 0 changes
    y_builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Dave".to_string(),
        files: Vec::new(),
    });
    // More identical timestamps to guarantee binary search loop executes
    y_builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Dave".to_string(),
        files: Vec::new(),
    });
    y_builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Dave".to_string(),
        files: Vec::new(),
    });
    y_builder.add_commit(CommitInput {
        timestamp: 1609459200,
        username: "Dave".to_string(),
        files: Vec::new(),
    });
    let y_hist = y_builder.finish();
    let y_bytes = y_hist.save_to_bytes("y_key");
    let y_loaded = History::load_from_bytes(&y_bytes, "y_key").expect("load y");
    assert_eq!(y_loaded.commit_count(), 5);

    // State at timestamp with 5 identical timestamps
    let at_ts = y_hist.state_at_timestamp(1609459200).unwrap();
    assert_eq!(at_ts.commit_index, 4);

    // Corrupted payload with invalid change op:
    let mut bad_op_builder =
        HistoryBuilder::new(CohortMode::Year, ChurnDecayModel::LifoYoungestFirst);
    bad_op_builder.add_commit(CommitInput {
        timestamp: 1000,
        username: "U".to_string(),
        files: vec![FileChangeInput {
            path: "f.txt".to_string(),
            op: ChangeOp::Add,
            lines_added: 1,
            lines_removed: 0,
            byte_size: None,
            is_binary: false,
        }],
    });
    let bad_op_hist = bad_op_builder.finish();
    let mut bad_op_bytes = bad_op_hist.save_to_bytes("k");
    bad_op_bytes[53] = 99; // invalid change op
    let len = bad_op_bytes.len();
    let new_chk = fnv1a_64(&bad_op_bytes[..len - 8]);
    bad_op_bytes[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_op_bytes, "k"),
        Err(CacheError::Corrupted(_))
    ));

    // Unexpected EOF when string length exceeds remaining buffer
    let mut bad_len_bytes = bad_op_hist.save_to_bytes("k");
    bad_len_bytes[20..24].copy_from_slice(&10_000u32.to_le_bytes());
    let len = bad_len_bytes.len();
    let new_chk = fnv1a_64(&bad_len_bytes[..len - 8]);
    bad_len_bytes[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_len_bytes, "k"),
        Err(CacheError::UnexpectedEof)
    ));

    // Change range out of bounds
    let mut bad_range_bytes = bad_op_hist.save_to_bytes("k");
    let len = bad_range_bytes.len();
    bad_range_bytes[len - 12..len - 8].copy_from_slice(&100u32.to_le_bytes());
    let new_chk = fnv1a_64(&bad_range_bytes[..len - 8]);
    bad_range_bytes[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_range_bytes, "k"),
        Err(CacheError::Corrupted(_))
    ));

    // Invalid user index
    let mut bad_user_bytes = bad_op_hist.save_to_bytes("k");
    let len = bad_user_bytes.len();
    bad_user_bytes[len - 20..len - 16].copy_from_slice(&9999u32.to_le_bytes());
    let new_chk = fnv1a_64(&bad_user_bytes[..len - 8]);
    bad_user_bytes[len - 8..].copy_from_slice(&new_chk.to_le_bytes());
    assert!(matches!(
        History::load_from_bytes(&bad_user_bytes, "k"),
        Err(CacheError::Corrupted(_))
    ));
}

#[test]
fn test_history_worker_streaming_cache_and_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let cache_file = dir.path().join("worker_cache.bin");
    let cache_key = "worker_repo_key_1";

    let config = HistoryWorkerConfig {
        cache_path: Some(cache_file.clone()),
        cache_key: cache_key.to_string(),
        cohort_mode: CohortMode::Year,
        decay_model: ChurnDecayModel::LifoYoungestFirst,
        publish_every: 5,
    };

    // 1. Streaming commits and observing intermediate snapshots
    let (worker, tx) = HistoryWorker::spawn(config.clone());
    assert!(matches!(
        worker.status(),
        WorkerStatus::Indexing { commits_indexed: 0 }
    ));

    for i in 0..12 {
        tx.send(CommitInput {
            timestamp: 1609459200 + i * 100,
            username: "Alice".to_string(),
            files: vec![FileChangeInput {
                path: format!("file_{i}.rs"),
                op: ChangeOp::Add,
                lines_added: 10,
                lines_removed: 0,
                byte_size: Some(300),
                is_binary: false,
            }],
        })
        .unwrap();
    }

    // Wait until at least 5 commits are indexed and intermediate snapshot is published
    let start_wait = std::time::Instant::now();
    while worker.latest().is_none() && start_wait.elapsed() < std::time::Duration::from_secs(2) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let intermediate = worker.latest();
    assert!(intermediate.is_some());

    // Drop sender to signal end of stream
    drop(tx);

    let final_hist = worker.wait().expect("final history returned");
    assert_eq!(final_hist.commit_count(), 12);
    assert!(cache_file.exists());

    // 2. Second worker loads immediately from valid on-disk cache
    let (cache_worker, _cache_tx) = HistoryWorker::spawn(config.clone());
    let loaded_hist = cache_worker.wait().expect("loaded from cache");
    assert_eq!(loaded_hist.commit_count(), 12);

    // 3. Fallback to stream indexing when cache is corrupted
    std::fs::write(&cache_file, b"corrupted garbage cache data").unwrap();
    let (corrupt_worker, tx_corrupt) = HistoryWorker::spawn(config.clone());

    for i in 0..4 {
        tx_corrupt
            .send(CommitInput {
                timestamp: 1700000000 + i * 100,
                username: "Bob".to_string(),
                files: vec![FileChangeInput {
                    path: "new.rs".to_string(),
                    op: ChangeOp::Add,
                    lines_added: 50,
                    lines_removed: 0,
                    byte_size: None,
                    is_binary: false,
                }],
            })
            .unwrap();
    }
    drop(tx_corrupt);

    let recovered_hist = corrupt_worker.wait().expect("recovered history");
    assert_eq!(recovered_hist.commit_count(), 4);
    // Cache file was overwritten with fresh valid data
    let re_read = History::load_from_path(&cache_file, cache_key);
    assert!(re_read.is_ok());

    // 4. Cancellation mid-stream
    let cancel_config = HistoryWorkerConfig::default();
    assert_eq!(cancel_config.publish_every, 128);
    let (cancel_worker, cancel_tx) = HistoryWorker::spawn(cancel_config);

    for i in 0..5 {
        let _ = cancel_tx.send(CommitInput {
            timestamp: 1600000000 + i * 100,
            username: "Eve".to_string(),
            files: Vec::new(),
        });
    }

    cancel_worker.cancel();
    // Worker exits cleanly and joins on wait or drop
    let _ = cancel_worker.wait();
}
