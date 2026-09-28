use glam::{Vec2, Vec3, Vec4};
use gource_settings::{
    CliAction, GOURCE_VERSION,
    conffile::{ConfEntry, ConfFile, ConfSection},
    display::DisplaySettings,
    gource::{CameraMode, GourceSettings, LogLevel},
    help::help_text,
    parse_command_line,
};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_conffile_primitives() {
    let entry_empty = ConfEntry::new("name", "", 1);
    assert!(!entry_empty.has_value());
    assert_eq!(entry_empty.get_int(), 0);
    assert_eq!(entry_empty.get_float(), 0.0);
    assert!(!entry_empty.get_bool());
    assert!(!entry_empty.is_int());
    assert!(!entry_empty.is_float());
    assert!(!entry_empty.is_bool());
    assert!(!entry_empty.is_vec2());
    assert!(!entry_empty.is_vec3());
    assert!(!entry_empty.is_vec4());
    assert_eq!(entry_empty.get_vec2(), Vec2::ZERO);
    assert_eq!(entry_empty.get_vec3(), Vec3::ZERO);
    assert_eq!(entry_empty.get_vec4(), Vec4::ZERO);

    let e_int = ConfEntry::new("x", "  -42  ", 2);
    assert!(e_int.has_value());
    assert!(e_int.is_int());
    assert!(e_int.is_float());
    assert_eq!(e_int.get_int(), -42);
    assert_eq!(e_int.get_float(), -42.0);

    let e_pos_int = ConfEntry::new("x", "+100", 3);
    assert_eq!(e_pos_int.get_int(), 100);

    let e_float = ConfEntry::new("f", "2.5", 4);
    assert!(e_float.is_float());
    assert_eq!(e_float.get_int(), 2);
    assert!((e_float.get_float() - 2.5).abs() < 1e-4);

    let e_exp = ConfEntry::new("f", "1.5e2", 5);
    assert_eq!(e_exp.get_float(), 150.0);

    for tr in ["1", "true", "True", "TRUE", "yes", "Yes", "YES"] {
        let e = ConfEntry::new("b", tr, 6);
        assert!(e.is_bool());
        assert!(e.get_bool());
    }
    for fa in ["0", "false", "False", "FALSE", "no", "No", "NO"] {
        let e = ConfEntry::new("b", fa, 7);
        assert!(e.is_bool());
        assert!(!e.get_bool());
    }
    let e_bad_bool = ConfEntry::new("b", "maybe", 8);
    assert!(!e_bad_bool.is_bool());
    assert!(!e_bad_bool.get_bool());

    let e_vec2 = ConfEntry::new("v", "vec2(10.5, -20.5)", 9);
    assert!(e_vec2.is_vec2());
    assert_eq!(e_vec2.get_vec2(), Vec2::new(10.5, -20.5));

    let e_vec3 = ConfEntry::new("v", "vec3(1.0, 2.0, 3.0)", 10);
    assert!(e_vec3.is_vec3());
    assert_eq!(e_vec3.get_vec3(), Vec3::new(1.0, 2.0, 3.0));

    let e_vec4 = ConfEntry::new("v", "vec4(1.0, 2.0, 3.0, 4.0)", 11);
    assert!(e_vec4.is_vec4());
    assert_eq!(e_vec4.get_vec4(), Vec4::new(1.0, 2.0, 3.0, 4.0));

    let e_bad_vec = ConfEntry::new("v", "vec2(1.0)", 12);
    assert!(!e_bad_vec.is_vec2());
    assert_eq!(e_bad_vec.get_vec2(), Vec2::ZERO);

    let e_bad_vec_chars = ConfEntry::new("v", "vec2(1.0, foo)", 13);
    assert!(!e_bad_vec_chars.is_vec2());
}

#[test]
fn test_confsection_methods() {
    let mut sec = ConfSection {
        name: "test".to_owned(),
        entries: Vec::new(),
        line: 1,
    };
    sec.add_entry("k1", "v1");
    sec.add_entry("k1", "v2");
    sec.add_entry("k2", "vec3(0.5, 0.5, 0.5)");
    sec.add_entry("k3", "vec4(1, 2, 3, 4)");
    sec.add_entry("k_empty", "");

    assert!(sec.has_value("k1"));
    assert!(!sec.has_value("k_empty"));
    assert!(!sec.has_value("nonexistent"));

    assert_eq!(sec.get_string("k1"), "v1");
    assert_eq!(sec.get_string("nonexistent"), "");
    assert_eq!(sec.get_int("nonexistent"), 0);
    assert_eq!(sec.get_float("nonexistent"), 0.0);
    assert!(!sec.get_bool("nonexistent"));
    assert_eq!(sec.get_vec3("nonexistent"), Vec3::ZERO);
    assert_eq!(sec.get_vec4("nonexistent"), Vec4::ZERO);

    assert_eq!(sec.get_vec3("k2"), Vec3::new(0.5, 0.5, 0.5));
    assert_eq!(sec.get_vec4("k3"), Vec4::new(1.0, 2.0, 3.0, 4.0));

    assert_eq!(sec.entries_named("k1").count(), 2);
    assert_eq!(sec.entries_named("nonexistent").count(), 0);

    sec.set_entry("k1", "v_updated");
    assert_eq!(sec.get_string("k1"), "v_updated");
    sec.set_entry("k_new", "v_new");
    assert_eq!(sec.get_string("k_new"), "v_new");
}

#[test]
fn test_conffile_disk_roundtrip() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("sample.conf");

    let mut conf = ConfFile::new();
    conf.filename = file_path.to_string_lossy().to_string();
    conf.set_entry("display", "viewport", "1280x720");
    conf.set_entry("gource", "path", ".");
    conf.set_entry("gource", "font-size", "20");
    conf.save(&file_path).unwrap();

    let loaded = ConfFile::load(&file_path).unwrap();
    assert_eq!(loaded.count_sections("display"), 1);
    assert_eq!(loaded.count_sections("gource"), 1);
    assert_eq!(
        loaded.section("display").unwrap().get_string("viewport"),
        "1280x720"
    );
    assert_eq!(loaded.section("gource").unwrap().get_int("font-size"), 20);

    // Test load non-existent
    let non_existent = dir.path().join("missing.conf");
    let err = ConfFile::load(&non_existent).unwrap_err();
    assert!(err.0.contains("failed to open config file"));

    // Test missing entry error
    let sec = loaded.section("gource").unwrap();
    let merr = loaded.missing_entry_error(sec, "required-key");
    assert!(
        merr.0
            .contains("section 'gource' missing required entry 'required-key'")
    );
}

#[test]
fn test_display_settings_comprehensive() {
    let mut conf = ConfFile::new();
    conf.filename = "test.conf".to_owned();
    let sec = conf.add_section("display");
    sec.add_entry("viewport", "1024x768!");
    sec.add_entry("window-position", "10x20");
    sec.add_entry("screen", "3");
    sec.add_entry("fullscreen", "true");
    sec.add_entry("windowed", "true"); // overrides fullscreen to false
    sec.add_entry("frameless", "true");
    sec.add_entry("transparent", "true");
    sec.add_entry("no-vsync", "true");
    sec.add_entry("high-dpi", "true");
    sec.add_entry("output-ppm-stream", "-");
    sec.add_entry("output-framerate", "25");

    let disp = DisplaySettings::import(&conf).unwrap();
    assert_eq!(disp.display_width, 1024);
    assert_eq!(disp.display_height, 768);
    assert!(!disp.resizable);
    assert!(disp.viewport_specified);
    assert_eq!(disp.window_x, 10);
    assert_eq!(disp.window_y, 20);
    assert_eq!(disp.screen, 3);
    assert!(!disp.fullscreen);
    assert!(disp.frameless);
    assert!(disp.transparent);
    assert!(!disp.vsync);
    assert!(disp.high_dpi);
    assert_eq!(disp.output_ppm_filename, "-");
    assert_eq!(disp.output_framerate, 25);

    let mut out_conf = ConfFile::new();
    disp.export(&mut out_conf);
    let out_sec = out_conf.section("display").unwrap();
    assert_eq!(out_sec.get_string("viewport"), "1024x768!");
    assert_eq!(out_sec.get_string("window-position"), "10x20");
    assert_eq!(out_sec.get_int("screen"), 3);
    assert!(out_sec.get_bool("frameless"));

    // Fullscreen without viewport_specified resets width/height to 0
    let mut fs_conf = ConfFile::new();
    let fs_sec = fs_conf.add_section("display");
    fs_sec.add_entry("fullscreen", "true");
    let fs_disp = DisplaySettings::import(&fs_conf).unwrap();
    assert!(fs_disp.fullscreen);
    assert_eq!(fs_disp.display_width, 0);
    assert_eq!(fs_disp.display_height, 0);

    // Export fullscreen
    let mut fs_out = ConfFile::new();
    fs_disp.export(&mut fs_out);
    assert!(fs_out.section("display").unwrap().get_bool("fullscreen"));

    // Display error cases
    let mut err_conf = ConfFile::parse("[display]\nwindow-position=bad\n", "d.conf").unwrap();
    assert_eq!(
        DisplaySettings::import(&err_conf).unwrap_err().0,
        "d.conf, line 2: invalid 'window-position' value"
    );

    err_conf = ConfFile::parse("[display]\noutput-ppm-stream=\n", "d.conf").unwrap();
    assert_eq!(
        DisplaySettings::import(&err_conf).unwrap_err().0,
        "d.conf, line 2: specify ppm output file or '-' for stdout"
    );

    err_conf = ConfFile::parse("[display]\noutput-framerate=\n", "d.conf").unwrap();
    assert_eq!(
        DisplaySettings::import(&err_conf).unwrap_err().0,
        "d.conf, line 2: specify framerate (25,30,60)"
    );
}

#[test]
fn test_gource_settings_comprehensive_validation() {
    let dir = tempdir().unwrap();

    // Create a dummy font file
    let font_path = dir.path().join("test_font.ttf");
    File::create(&font_path)
        .unwrap()
        .write_all(b"fake font")
        .unwrap();

    // Create a dummy caption file
    let caption_path = dir.path().join("captions.txt");
    File::create(&caption_path)
        .unwrap()
        .write_all(b"fake caption")
        .unwrap();

    // Create user image directory with image files
    let img_dir = dir.path().join("avatars");
    std::fs::create_dir(&img_dir).unwrap();
    File::create(img_dir.join("alice.png")).unwrap();
    File::create(img_dir.join("BOB.JPG")).unwrap();
    File::create(img_dir.join("charlie.JPEG")).unwrap();
    File::create(img_dir.join("ignored.txt")).unwrap();

    let mut conf = ConfFile::new();
    conf.filename = "g.conf".to_owned();
    let sec = conf.add_section("gource");

    sec.add_entry("disable-auto-rotate", "true");
    sec.add_entry("disable-auto-skip", "true");
    sec.add_entry("disable-input", "true");
    sec.add_entry("loop", "true");
    sec.add_entry("loop-delay-seconds", "5.5");
    sec.add_entry("git-branch", "main");
    sec.add_entry("colour-images", "true");
    sec.add_entry("crop", "vertical");
    sec.add_entry("log-format", "git");
    sec.add_entry("default-user-image", "default.png");
    sec.add_entry("user-image-dir", img_dir.to_str().unwrap());
    sec.add_entry("caption-file", caption_path.to_str().unwrap());
    sec.add_entry("caption-duration", "8.0");
    sec.add_entry("caption-size", "22");
    sec.add_entry("caption-offset", "15");
    sec.add_entry("caption-colour", "FF0000");
    sec.add_entry("filename-colour", "00FF00");
    sec.add_entry("filename-time", "5.0");
    sec.add_entry("bloom-intensity", "0.8");
    sec.add_entry("bloom-multiplier", "1.2");
    sec.add_entry("elasticity", "0.3");
    sec.add_entry("font-file", font_path.to_str().unwrap());
    sec.add_entry("font-size", "18");
    sec.add_entry("file-font-size", "15");
    sec.add_entry("dir-font-size", "16");
    sec.add_entry("user-font-size", "17");
    sec.add_entry("font-scale", "1.5");
    sec.add_entry("hash-seed", "42");
    sec.add_entry("font-colour", "0000FF");
    sec.add_entry("background-colour", "102030");
    sec.add_entry("highlight-colour", "AABBCC");
    sec.add_entry("selection-colour", "DDEEFF");
    sec.add_entry("dir-colour", "112233");
    sec.add_entry("background-image", "bg.png");
    sec.add_entry("title", "Test Project");
    sec.add_entry("logo", "logo.png");
    sec.add_entry("logo-offset", "30x40");
    sec.add_entry("seconds-per-day", "20.0");
    sec.add_entry("auto-skip-seconds", "4.0");
    sec.add_entry("file-idle-time", "10");
    sec.add_entry("file-idle-time-at-end", "12");
    sec.add_entry("user-idle-time", "6.0");
    sec.add_entry("time-scale", "2.0");
    sec.add_entry("start-date", "2021-01-01");
    sec.add_entry("stop-date", "2021-01-02");
    sec.add_entry("start-position", "0.25");
    sec.add_entry("stop-position", "0.75");
    sec.add_entry("stop-at-time", "60.0");
    sec.add_entry("key", "true");
    sec.add_entry("ffp", "true");
    sec.add_entry("realtime", "true");
    sec.add_entry("no-time-travel", "true");
    sec.add_entry("dont-stop", "true");
    sec.add_entry("stop-at-end", "true");
    sec.add_entry("stop-on-idle", "true");
    sec.add_entry("fixed-user-size", "true");
    sec.add_entry("author-time", "true");
    sec.add_entry("max-files", "500");
    sec.add_entry("max-file-lag", "3.0");
    sec.add_entry("user-friction", "0.5");
    sec.add_entry("user-scale", "1.5");
    sec.add_entry("max-user-speed", "600.0");
    sec.add_entry("highlight-users", "true");
    sec.add_entry("highlight-dirs", "true");
    sec.add_entry("camera-mode", "track");
    sec.add_entry("padding", "1.2");
    sec.add_entry("highlight-user", "alice");
    sec.add_entry("highlight-user", "bob");
    sec.add_entry("follow-user", "charlie");
    sec.add_entry("file-extensions", "true");
    sec.add_entry("file-extension-fallback", "true");
    sec.add_entry("file-filter", ".*\\.tmp");
    sec.add_entry("file-show-filter", ".*\\.rs");
    sec.add_entry("user-filter", "bot");
    sec.add_entry("user-show-filter", "dev.*");
    sec.add_entry("dir-name-depth", "3");
    sec.add_entry("dir-name-position", "0.6");
    sec.add_entry("path", dir.path().to_str().unwrap());

    let s = GourceSettings::import(&conf, None).unwrap();
    assert!(s.disable_auto_rotate);
    assert_eq!(s.auto_skip_seconds, 4.0);
    assert!(s.disable_input);
    assert!(s.looping);
    assert_eq!(s.loop_delay_seconds, 5.5);
    assert_eq!(s.git_branch, "main");
    assert!(s.colour_user_images);
    assert!(s.crop_vertical);
    assert!(!s.crop_horizontal);
    assert_eq!(s.log_format, "git");
    assert_eq!(s.default_user_image, "default.png");
    assert_eq!(s.user_image_map.len(), 3);
    assert!(s.user_image_map.contains_key("alice"));
    assert!(s.user_image_map.contains_key("BOB"));
    assert!(s.user_image_map.contains_key("charlie"));
    assert_eq!(s.caption_duration, 8.0);
    assert_eq!(s.caption_size, 22);
    assert_eq!(s.caption_offset, 15);
    assert_eq!(s.filename_time, 5.0);
    assert_eq!(s.bloom_intensity, 0.8);
    assert_eq!(s.bloom_multiplier, 1.2);
    assert_eq!(s.elasticity, 0.3);
    assert_eq!(s.font_size, 18);
    assert_eq!(s.font_scale, 1.5);
    assert_eq!(s.hash_seed, 42);
    assert_eq!(s.title, "Test Project");
    assert_eq!(s.logo, "logo.png");
    assert_eq!(s.logo_offset, Vec2::new(30.0, 40.0));
    assert_eq!(s.file_idle_time, 10.0);
    assert_eq!(s.file_idle_time_at_end, 12.0);
    assert_eq!(s.user_idle_time, 6.0);
    assert_eq!(s.time_scale, 2.0);
    assert_eq!(s.start_position, 0.25);
    assert_eq!(s.stop_position, 0.75);
    assert_eq!(s.stop_at_time, 60.0);
    assert!(s.show_key);
    assert!(s.ffp);
    assert_eq!(s.days_per_second, 1.0 / 86400.0); // realtime
    assert!(s.no_time_travel);
    assert!(s.dont_stop);
    assert!(s.stop_at_end);
    assert!(s.stop_on_idle);
    assert!(s.fixed_user_size);
    assert!(s.author_time);
    assert_eq!(s.max_files, 500);
    assert_eq!(s.max_file_lag, 3.0);
    assert_eq!(s.user_friction, 2.0); // 1.0 / 0.5
    assert_eq!(s.user_scale, 1.5);
    assert_eq!(s.max_user_speed, 600.0);
    assert!(s.highlight_all_users);
    assert!(s.highlight_dirs);
    assert_eq!(s.camera_mode, CameraMode::Track);
    assert_eq!(s.camera_mode.as_str(), "track");
    assert_eq!(CameraMode::Overview.as_str(), "overview");
    assert_eq!(s.padding, 1.2);
    assert_eq!(s.highlight_users, vec!["alice", "bob"]);
    assert_eq!(s.follow_users, vec!["charlie"]);
    assert!(s.file_extensions);
    assert!(s.file_extension_fallback);
    assert_eq!(s.file_filters.len(), 1);
    assert_eq!(s.file_show_filters.len(), 1);
    assert_eq!(s.user_filters.len(), 1);
    assert_eq!(s.user_show_filters.len(), 1);
    assert_eq!(s.dir_name_depth, 3);
    assert_eq!(s.dir_name_position, 0.6);
}

#[test]
fn test_gource_settings_error_messages() {
    let check_err = |cfg_text: &str, expected_sub: &str| {
        let conf = ConfFile::parse(cfg_text, "err.conf").unwrap();
        let err = GourceSettings::import(&conf, None).unwrap_err();
        assert_eq!(err.0, expected_sub);
    };

    check_err(
        "[gource]\nhide=\n",
        "err.conf, line 2: no value specified for 'hide'",
    );
    check_err(
        "[gource]\nhide=invalid\n",
        "err.conf, line 2: unknown option hide invalid",
    );
    check_err(
        "[gource]\ndate-format=\n",
        "err.conf, line 2: no value specified for 'date-format'",
    );
    check_err(
        "[gource]\nloop-delay-seconds=\n",
        "err.conf, line 2: specify loop-delay-seconds (float)",
    );
    check_err(
        "[gource]\nloop-delay-seconds=0\n",
        "err.conf, line 2: invalid 'loop-delay-seconds' value",
    );
    check_err(
        "[gource]\ngit-branch=\n",
        "err.conf, line 2: no value specified for 'git-branch'",
    );
    check_err(
        "[gource]\ncrop=\n",
        "err.conf, line 2: specify crop (vertical,horizontal)",
    );
    check_err(
        "[gource]\ncrop=diagonal\n",
        "err.conf, line 2: invalid 'crop' value",
    );
    check_err(
        "[gource]\nlog-format=\n",
        "err.conf, line 2: specify log-format (format)",
    );
    check_err(
        "[gource]\nlog-format=cvs\n",
        "err.conf, line 2: please use either 'cvs2cl' or 'cvs-exp'",
    );
    check_err(
        "[gource]\nlog-format=invalid\n",
        "err.conf, line 2: invalid 'log-format' value",
    );
    check_err(
        "[gource]\ndefault-user-image=\n",
        "err.conf, line 2: specify default-user-image (image path)",
    );
    check_err(
        "[gource]\nuser-image-dir=\n",
        "err.conf, line 2: specify user-image-dir (directory)",
    );
    check_err(
        "[gource]\nuser-image-dir=/nonexistent/dir\n",
        "err.conf, line 2: specified user-image-dir is not a directory",
    );
    check_err(
        "[gource]\ncaption-file=\n",
        "err.conf, line 2: specify caption file (filename)",
    );
    check_err(
        "[gource]\ncaption-file=/nonexistent/caps.txt\n",
        "err.conf, line 2: caption file not found",
    );
    check_err(
        "[gource]\ncaption-duration=\n",
        "err.conf, line 2: specify caption duration (seconds)",
    );
    check_err(
        "[gource]\ncaption-duration=0\n",
        "err.conf, line 2: invalid 'caption-duration' value",
    );
    check_err(
        "[gource]\ncaption-size=\n",
        "err.conf, line 2: specify caption size",
    );
    check_err(
        "[gource]\ncaption-size=0\n",
        "err.conf, line 2: invalid 'caption-size' value",
    );
    check_err(
        "[gource]\ncaption-size=101\n",
        "err.conf, line 2: invalid 'caption-size' value",
    );
    check_err(
        "[gource]\ncaption-offset=\n",
        "err.conf, line 2: specify caption offset",
    );
    check_err(
        "[gource]\ncaption-colour=\n",
        "err.conf, line 2: specify caption colour (FFFFFF)",
    );
    check_err(
        "[gource]\ncaption-colour=bad\n",
        "err.conf, line 2: invalid 'caption-colour' value",
    );
    check_err(
        "[gource]\nfilename-colour=\n",
        "err.conf, line 2: specify filename colour (FFFFFF)",
    );
    check_err(
        "[gource]\nfilename-colour=bad\n",
        "err.conf, line 2: invalid 'filename-colour' value",
    );
    check_err(
        "[gource]\nfilename-time=\n",
        "err.conf, line 2: specify duration to keep files on screen (float)",
    );
    check_err(
        "[gource]\nfilename-time=1.5\n",
        "err.conf, line 2: filename-time must be >= 2.0",
    );
    check_err(
        "[gource]\nbloom-intensity=\n",
        "err.conf, line 2: specify bloom-intensity (float)",
    );
    check_err(
        "[gource]\nbloom-intensity=0\n",
        "err.conf, line 2: invalid 'bloom-intensity' value",
    );
    check_err(
        "[gource]\nbloom-multiplier=\n",
        "err.conf, line 2: specify bloom-multiplier (float)",
    );
    check_err(
        "[gource]\nbloom-multiplier=0\n",
        "err.conf, line 2: invalid 'bloom-multiplier' value",
    );
    check_err(
        "[gource]\nelasticity=\n",
        "err.conf, line 2: specify elasticity (float)",
    );
    check_err(
        "[gource]\nelasticity=0\n",
        "err.conf, line 2: invalid 'elasticity' value",
    );
    check_err(
        "[gource]\nfont-file=\n",
        "err.conf, line 2: specify font file",
    );
    check_err(
        "[gource]\nfont-file=/nonexistent/font.ttf\n",
        "err.conf, line 2: invalid 'font-file' value",
    );
    check_err(
        "[gource]\nfont-size=\n",
        "err.conf, line 2: specify font size",
    );
    check_err(
        "[gource]\nfont-size=0\n",
        "err.conf, line 2: invalid 'font-size' value",
    );
    check_err(
        "[gource]\nfile-font-size=\n",
        "err.conf, line 2: specify font size",
    );
    check_err(
        "[gource]\nfile-font-size=0\n",
        "err.conf, line 2: invalid 'file-font-size' value",
    );
    check_err(
        "[gource]\ndir-font-size=\n",
        "err.conf, line 2: specify font size",
    );
    check_err(
        "[gource]\ndir-font-size=0\n",
        "err.conf, line 2: invalid 'dir-font-size' value",
    );
    check_err(
        "[gource]\nuser-font-size=\n",
        "err.conf, line 2: specify font size",
    );
    check_err(
        "[gource]\nuser-font-size=0\n",
        "err.conf, line 2: invalid 'user-font-size' value",
    );
    check_err(
        "[gource]\nfont-scale=\n",
        "err.conf, line 2: specify font scale",
    );
    check_err(
        "[gource]\nfont-scale=15.0\n",
        "err.conf, line 2: invalid 'font-scale' value",
    );
    check_err(
        "[gource]\nhash-seed=\n",
        "err.conf, line 2: specify hash seed (integer)",
    );
    check_err(
        "[gource]\nfont-colour=\n",
        "err.conf, line 2: specify font colour (FFFFFF)",
    );
    check_err(
        "[gource]\nfont-colour=bad\n",
        "err.conf, line 2: invalid 'font-colour' value",
    );
    check_err(
        "[gource]\nbackground-colour=\n",
        "err.conf, line 2: specify background colour (FFFFFF)",
    );
    check_err(
        "[gource]\nbackground-colour=bad\n",
        "err.conf, line 2: invalid 'background-colour' value",
    );
    check_err(
        "[gource]\nhighlight-colour=\n",
        "err.conf, line 2: specify highlight colour (FFFFFF)",
    );
    check_err(
        "[gource]\nhighlight-colour=bad\n",
        "err.conf, line 2: invalid 'highlight-colour' value",
    );
    check_err(
        "[gource]\nselection-colour=\n",
        "err.conf, line 2: specify selection colour (FFFFFF)",
    );
    check_err(
        "[gource]\nselection-colour=bad\n",
        "err.conf, line 2: invalid 'selection-colour' value",
    );
    check_err(
        "[gource]\ndir-colour=\n",
        "err.conf, line 2: specify dir colour (FFFFFF)",
    );
    check_err(
        "[gource]\ndir-colour=bad\n",
        "err.conf, line 2: invalid 'dir-colour' value",
    );
    check_err(
        "[gource]\nbackground-image=\n",
        "err.conf, line 2: specify background image (image path)",
    );
    check_err("[gource]\ntitle=\n", "err.conf, line 2: specify title");
    check_err(
        "[gource]\nlogo=\n",
        "err.conf, line 2: specify logo (image path)",
    );
    check_err(
        "[gource]\nlogo-offset=\n",
        "err.conf, line 2: specify logo-offset (XxY)",
    );
    check_err(
        "[gource]\nlogo-offset=bad\n",
        "err.conf, line 2: invalid 'logo-offset' value",
    );
    check_err(
        "[gource]\nseconds-per-day=\n",
        "err.conf, line 2: specify seconds-per-day (seconds)",
    );
    check_err(
        "[gource]\nseconds-per-day=0\n",
        "err.conf, line 2: invalid 'seconds-per-day' value",
    );
    check_err(
        "[gource]\nauto-skip-seconds=\n",
        "err.conf, line 2: specify auto-skip-seconds (seconds)",
    );
    check_err(
        "[gource]\nauto-skip-seconds=0\n",
        "err.conf, line 2: invalid 'auto-skip-seconds' value",
    );
    check_err(
        "[gource]\nfile-idle-time=\n",
        "err.conf, line 2: specify file-idle-time (seconds)",
    );
    check_err(
        "[gource]\nfile-idle-time=bad\n",
        "err.conf, line 2: invalid 'file-idle-time' value",
    );
    check_err(
        "[gource]\nfile-idle-time-at-end=\n",
        "err.conf, line 2: specify file-idle-time-at-end (seconds)",
    );
    check_err(
        "[gource]\nfile-idle-time-at-end=bad\n",
        "err.conf, line 2: invalid 'file-idle-time-at-end' value",
    );
    check_err(
        "[gource]\nuser-idle-time=\n",
        "err.conf, line 2: specify user-idle-time (seconds)",
    );
    check_err(
        "[gource]\nuser-idle-time=-1\n",
        "err.conf, line 2: invalid 'user-idle-time' value",
    );
    check_err(
        "[gource]\ntime-scale=\n",
        "err.conf, line 2: specify time-scale (scale)",
    );
    check_err(
        "[gource]\ntime-scale=0\n",
        "err.conf, line 2: time-scale outside of range 0.0 - 4.0",
    );
    check_err(
        "[gource]\ntime-scale=5\n",
        "err.conf, line 2: time-scale outside of range 0.0 - 4.0",
    );
    check_err(
        "[gource]\nstart-date=\n",
        "err.conf, line 2: specify start-date (YYYY-MM-DD hh:mm:ss)",
    );
    check_err(
        "[gource]\nstart-date=invalid\n",
        "err.conf, line 2: invalid 'start-date' value",
    );
    check_err(
        "[gource]\nstop-date=\n",
        "err.conf, line 2: specify stop-date (YYYY-MM-DD hh:mm:ss)",
    );
    check_err(
        "[gource]\nstop-date=invalid\n",
        "err.conf, line 2: invalid 'stop-date' value",
    );
    check_err(
        "[gource]\nstart-position=\n",
        "err.conf, line 2: specify start-position (float,random)",
    );
    check_err(
        "[gource]\nstart-position=0\n",
        "err.conf, line 2: start-position outside of range 0.0 - 1.0 (non-inclusive)",
    );
    check_err(
        "[gource]\nstop-position=\n",
        "err.conf, line 2: specify stop-position (float)",
    );
    check_err(
        "[gource]\nstop-position=1.5\n",
        "err.conf, line 2: stop-position outside of range 0.0 - 1.0 (inclusive)",
    );
    check_err(
        "[gource]\nstop-at-time=\n",
        "err.conf, line 2: specify stop-at-time (seconds)",
    );
    check_err(
        "[gource]\nstop-at-time=0\n",
        "err.conf, line 2: invalid 'stop-at-time' value",
    );
    check_err(
        "[gource]\nmax-files=\n",
        "err.conf, line 2: specify max-files (number)",
    );
    check_err(
        "[gource]\nmax-files=bad\n",
        "err.conf, line 2: invalid 'max-files' value",
    );
    check_err(
        "[gource]\nmax-file-lag=\n",
        "err.conf, line 2: specify max-file-lag (seconds)",
    );
    check_err(
        "[gource]\nmax-file-lag=0\n",
        "err.conf, line 2: invalid 'max-file-lag' value",
    );
    check_err(
        "[gource]\nuser-friction=\n",
        "err.conf, line 2: specify user-friction (seconds)",
    );
    check_err(
        "[gource]\nuser-friction=0\n",
        "err.conf, line 2: invalid 'user-friction' value",
    );
    check_err(
        "[gource]\nuser-scale=\n",
        "err.conf, line 2: specify user-scale (scale)",
    );
    check_err(
        "[gource]\nuser-scale=0\n",
        "err.conf, line 2: invalid 'user-scale' value",
    );
    check_err(
        "[gource]\nmax-user-speed=\n",
        "err.conf, line 2: specify max-user-speed (units)",
    );
    check_err(
        "[gource]\nmax-user-speed=0\n",
        "err.conf, line 2: invalid 'max-user-speed' value",
    );
    check_err(
        "[gource]\ncamera-mode=\n",
        "err.conf, line 2: specify camera-mode (overview,track)",
    );
    check_err(
        "[gource]\ncamera-mode=bad\n",
        "err.conf, line 2: invalid 'camera-mode' value",
    );
    check_err(
        "[gource]\npadding=\n",
        "err.conf, line 2: specify padding (float)",
    );
    check_err(
        "[gource]\npadding=2.5\n",
        "err.conf, line 2: invalid 'padding' value",
    );
    check_err(
        "[gource]\nhighlight-user=\n",
        "err.conf, line 2: specify highlight-user (user)",
    );
    check_err(
        "[gource]\nfollow-user=\n",
        "err.conf, line 2: specify follow-user (user)",
    );
    check_err(
        "[gource]\nfile-filter=\n",
        "err.conf, line 2: specify file-filter (regex)",
    );
    check_err(
        "[gource]\nfile-filter=[\n",
        "err.conf, line 2: invalid file-filter regular expression",
    );
    check_err(
        "[gource]\nfile-show-filter=\n",
        "err.conf, line 2: specify file-show-filter (regex)",
    );
    check_err(
        "[gource]\nfile-show-filter=[\n",
        "err.conf, line 2: invalid file-show-filter regular expression",
    );
    check_err(
        "[gource]\nuser-filter=\n",
        "err.conf, line 2: specify user-filter (regex)",
    );
    check_err(
        "[gource]\nuser-filter=[\n",
        "err.conf, line 2: invalid user-filter regular expression",
    );
    check_err(
        "[gource]\nuser-show-filter=\n",
        "err.conf, line 2: specify user-show-filter (regex)",
    );
    check_err(
        "[gource]\nuser-show-filter=[\n",
        "err.conf, line 2: invalid user-show-filter regular expression",
    );
    check_err(
        "[gource]\ndir-name-depth=\n",
        "err.conf, line 2: specify dir-name-depth (depth)",
    );
    check_err(
        "[gource]\ndir-name-depth=0\n",
        "err.conf, line 2: invalid 'dir-name-depth' value",
    );
    check_err(
        "[gource]\ndir-name-position=\n",
        "err.conf, line 2: specify dir-name-position (float)",
    );
    check_err(
        "[gource]\ndir-name-position=0.05\n",
        "err.conf, line 2: dir-name-position outside of range 0.1 - 1.0 (inclusive)",
    );
    check_err(
        "[gource]\npath=/nonexistent/path/xyz\n",
        "'/nonexistent/path/xyz' does not appear to be a valid file or directory",
    );
    check_err(
        "[gource]\npath=-\n",
        "log-format required when reading from STDIN",
    );
}

#[test]
fn test_parse_command_line_all_actions() {
    let dir = tempdir().unwrap();
    let conf_path = dir.path().join("my.conf");
    let save_path = dir.path().join("saved.conf");

    // Test SaveConfig action
    let args = vec![
        "--save-config".to_string(),
        save_path.to_str().unwrap().to_string(),
        ".".to_string(),
    ];
    let action = parse_command_line(&args).unwrap();
    match action {
        CliAction::SaveConfig { path, config } => {
            assert_eq!(path, save_path.to_str().unwrap());
            assert_eq!(config.gource.path, ".");
            config.conf.save(Path::new(&path)).unwrap();
            assert!(save_path.exists());
        }
        _ => panic!("expected SaveConfig"),
    }

    // Test OutputCustomLog action
    let args = vec![
        "--output-custom-log".to_string(),
        "custom.log".to_string(),
        ".".to_string(),
    ];
    let action = parse_command_line(&args).unwrap();
    match action {
        CliAction::OutputCustomLog { output, config } => {
            assert_eq!(output, "custom.log");
            assert_eq!(config.gource.path, ".");
        }
        _ => panic!("expected OutputCustomLog"),
    }

    // Test loading config from positional arg
    let mut initial_conf = ConfFile::new();
    initial_conf.set_entry("display", "screen", "4");
    initial_conf.set_entry("gource", "title", "Positional Conf");
    initial_conf.save(&conf_path).unwrap();

    let args = vec![conf_path.to_str().unwrap().to_string(), ".".to_string()];
    let action = parse_command_line(&args).unwrap();
    match action {
        CliAction::Run(config) => {
            assert_eq!(config.display.screen, 4);
            assert_eq!(config.gource.title, "Positional Conf");
            assert_eq!(config.gource.path, ".");
        }
        _ => panic!("expected Run"),
    }

    // Test --load-config precedence: command line args override loaded config
    let args = vec![
        "--load-config".to_string(),
        conf_path.to_str().unwrap().to_string(),
        "--title".to_string(),
        "Overridden Title".to_string(),
        ".".to_string(),
    ];
    let action = parse_command_line(&args).unwrap();
    match action {
        CliAction::Run(config) => {
            assert_eq!(config.display.screen, 4);
            assert_eq!(config.gource.title, "Overridden Title");
        }
        _ => panic!("expected Run"),
    }

    // Test multiple [gource] sections
    let multi_conf_path = dir.path().join("multi.conf");
    let mut multi_conf = ConfFile::new();
    let s1 = multi_conf.add_section("gource");
    s1.set_entry("path", ".");
    let s2 = multi_conf.add_section("gource");
    s2.set_entry("path", ".");
    multi_conf.save(&multi_conf_path).unwrap();

    let args = vec![
        "--load-config".to_string(),
        multi_conf_path.to_str().unwrap().to_string(),
        ".".to_string(),
    ];
    let action = parse_command_line(&args).unwrap();
    match action {
        CliAction::Run(config) => {
            assert_eq!(config.gource.repo_count, 2);
        }
        _ => panic!("expected Run"),
    }
}

#[test]
fn test_additional_cli_coverage() {
    // Test magic viewport alias: e.g. "-1280x720"
    let args = vec!["-1280x720".to_string(), ".".to_string()];
    let action = parse_command_line(&args).unwrap();
    if let CliAction::Run(config) = action {
        assert_eq!(config.display.display_width, 1280);
        assert_eq!(config.display.display_height, 720);
    } else {
        panic!("expected Run");
    }

    // Test log-command aliases: --cvs-exp-command, --cvs2cl-command, --svn-log-command, --hg-log-command, --bzr-log-command
    for (flag, expected) in [
        ("--cvs-exp-command", "cvs-exp"),
        ("--cvs2cl-command", "cvs2cl"),
        ("--svn-log-command", "svn"),
        ("--hg-log-command", "hg"),
        ("--bzr-log-command", "bzr"),
    ] {
        let args = vec![flag.to_string()];
        let action = parse_command_line(&args).unwrap();
        match action {
            CliAction::PrintLogCommand { vcs } => assert_eq!(vcs, expected),
            _ => panic!("expected PrintLogCommand"),
        }
    }

    // Test --log-command with valid and invalid options
    for vcs in ["git", "cvs-exp", "cvs2cl", "svn", "hg", "bzr"] {
        let args = vec!["--log-command".to_string(), vcs.to_string()];
        let action = parse_command_line(&args).unwrap();
        match action {
            CliAction::PrintLogCommand { vcs: matched } => assert_eq!(matched, vcs),
            _ => panic!("expected PrintLogCommand"),
        }
    }
    let err =
        parse_command_line(&["--log-command".to_string(), "invalid".to_string()]).unwrap_err();
    assert_eq!(err.0, "invalid log-command value");

    // Test log-level options and invalid value
    for lvl in ["warn", "debug", "info", "error", "pedantic"] {
        let args = vec!["--log-level".to_string(), lvl.to_string(), ".".to_string()];
        let action = parse_command_line(&args).unwrap();
        match action {
            CliAction::Run(config) => {
                assert_eq!(config.gource.log_level, LogLevel::parse(lvl).unwrap())
            }
            _ => panic!("expected Run"),
        }
    }
    let err = parse_command_line(&[
        "--log-level".to_string(),
        "unknown".to_string(),
        ".".to_string(),
    ])
    .unwrap_err();
    assert_eq!(err.0, "invalid log-level value");

    // Test empty arg handling
    let args = vec!["".to_string(), ".".to_string()];
    let action = parse_command_line(&args).unwrap();
    assert!(matches!(action, CliAction::Run(_)));

    // Test errors for load-config and save-config with empty values
    let err = parse_command_line(&["--load-config".to_string(), "".to_string()]).unwrap_err();
    assert_eq!(err.0, "invalid load-config value");
    let err = parse_command_line(&["--save-config".to_string(), "".to_string()]).unwrap_err();
    assert_eq!(err.0, "invalid save-config value");
    let err = parse_command_line(&["--output-custom-log".to_string(), "".to_string()]).unwrap_err();
    assert_eq!(err.0, "invalid output-custom-log value");

    // Test version constant and help text
    assert!(!GOURCE_VERSION.is_empty());
    assert!(!help_text(false).is_empty());
    assert!(!help_text(true).is_empty());
}

#[test]
fn test_additional_gource_settings_and_conffile_coverage() {
    // Test LogLevel::parse
    assert_eq!(LogLevel::parse("warn"), Some(LogLevel::Warn));
    assert_eq!(LogLevel::parse("debug"), Some(LogLevel::Debug));
    assert_eq!(LogLevel::parse("info"), Some(LogLevel::Info));
    assert_eq!(LogLevel::parse("error"), Some(LogLevel::Error));
    assert_eq!(LogLevel::parse("pedantic"), Some(LogLevel::Pedantic));
    assert_eq!(LogLevel::parse("none"), None);

    // Test date-format setting
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", ".");
    sec.add_entry("date-format", "%d %b %Y");
    let s = GourceSettings::import(&conf, None).unwrap();
    assert_eq!(s.date_format, "%d %b %Y");

    // Test empty date-format error
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("date-format", "");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test stop-date with non-zero time adding 86400 seconds
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", ".");
    sec.add_entry("stop-date", "2021-01-01 12:00:00");
    let s = GourceSettings::import(&conf, None).unwrap();
    assert!(s.stop_timestamp > 0);

    // Test start-position random behavior
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", ".");
    sec.add_entry("start-position", "random");
    let s = GourceSettings::import(&conf, None).unwrap();
    assert!(s.start_position >= 0.0 && s.start_position < 1.0);

    // Test hide options: mouse, root, comma-separated
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", ".");
    sec.add_entry("hide", "mouse,root,progress,users");
    let s = GourceSettings::import(&conf, None).unwrap();
    assert!(s.hide_mouse);
    assert!(s.hide_root);
    assert!(s.hide_progress);
    assert!(s.hide_users);

    // Test export display settings when section already exists
    let mut conf = ConfFile::new();
    conf.add_section("display");
    let ds = DisplaySettings::default();
    ds.export(&mut conf);
    assert!(conf.section("display").is_some());

    // Test ConfFile error without filename or line
    let err_str = conf.entry_error(None, "some error");
    assert_eq!(err_str.0, "some error");
    let dummy_entry = ConfEntry::new("k", "v", 0);
    let err_str = conf.entry_error(Some(&dummy_entry), "some error");
    assert_eq!(err_str.0, "some error");

    // Test ConfFile error with empty lines / comments in parse
    let ini = "# initial comment\n\n[test]\nkey=value\n# another comment\n";
    let loaded = ConfFile::parse(ini, "").unwrap();
    assert_eq!(loaded.sections.len(), 1);

    // Test ConfFile parse error on unformatted line
    let bad_ini = "this is not key value and not a header";
    assert!(ConfFile::parse(bad_ini, "").is_err());
    assert!(ConfFile::parse(bad_ini, "my.conf").is_err());

    // Test ConfFile section getter
    assert!(conf.section("non_existent").is_none());

    // Test user-image-dir pointing to non-existent or invalid directory
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("user-image-dir", "/path/that/does/not/exist/ever/12345");
    let err = GourceSettings::import(&conf, None).unwrap_err();
    assert!(
        err.0
            .contains("specified user-image-dir is not a directory")
    );

    // Test path trailing slash stripping
    let dir = tempdir().unwrap();
    let sub = dir.path().join("subdir");
    std::fs::create_dir(&sub).unwrap();
    let mut path_str = sub.to_str().unwrap().to_string();
    path_str.push('/');
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", &path_str);
    let s = GourceSettings::import(&conf, None).unwrap();
    assert_eq!(s.path, sub.to_str().unwrap());

    // Test color parsing with vec3(...) and alpha clamp
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("path", ".");
    sec.add_entry("background-colour", "vec3(1.0, 0.5, 0.2)");
    let s = GourceSettings::import(&conf, None).unwrap();
    assert_eq!(s.background_colour, glam::Vec3::new(1.0, 0.5, 0.2));

    // Test background-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("background-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test font-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("font-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test highlight-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("highlight-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test selection-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("selection-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test dir-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("dir-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test filename-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("filename-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());

    // Test caption-colour invalid
    let mut conf = ConfFile::new();
    let sec = conf.add_section("gource");
    sec.add_entry("caption-colour", "invalid-color");
    assert!(GourceSettings::import(&conf, None).is_err());
}
