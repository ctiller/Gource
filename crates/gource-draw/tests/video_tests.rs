use gource_draw::{VideoCodec, VideoConfig, VideoExporter, VideoSink};
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_video_codec_inference_and_parsing() {
    assert_eq!(VideoCodec::from_extension("mp4"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("MP4"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("m4v"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("webm"), VideoCodec::Vp9);
    assert_eq!(VideoCodec::from_extension("mkv"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("mov"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("avi"), VideoCodec::H264);
    assert_eq!(VideoCodec::from_extension("gif"), VideoCodec::Gif);
    assert_eq!(VideoCodec::from_extension("unknown"), VideoCodec::H264);

    assert_eq!(VideoCodec::parse_name("auto"), Some(VideoCodec::Auto));
    assert_eq!(VideoCodec::parse_name("h264"), Some(VideoCodec::H264));
    assert_eq!(VideoCodec::parse_name("x264"), Some(VideoCodec::H264));
    assert_eq!(VideoCodec::parse_name("avc"), Some(VideoCodec::H264));
    assert_eq!(VideoCodec::parse_name("h265"), Some(VideoCodec::H265));
    assert_eq!(VideoCodec::parse_name("x265"), Some(VideoCodec::H265));
    assert_eq!(VideoCodec::parse_name("hevc"), Some(VideoCodec::H265));
    assert_eq!(VideoCodec::parse_name("vp8"), Some(VideoCodec::Vp8));
    assert_eq!(VideoCodec::parse_name("vp9"), Some(VideoCodec::Vp9));
    assert_eq!(VideoCodec::parse_name("av1"), Some(VideoCodec::Av1));
    assert_eq!(VideoCodec::parse_name("svtav1"), Some(VideoCodec::Av1));
    assert_eq!(VideoCodec::parse_name("prores"), Some(VideoCodec::ProRes));
    assert_eq!(VideoCodec::parse_name("gif"), Some(VideoCodec::Gif));
    assert_eq!(VideoCodec::parse_name("invalid_codec"), None);

    assert_eq!(VideoCodec::H264.ffmpeg_codec_name(), "libx264");
    assert_eq!(VideoCodec::H265.ffmpeg_codec_name(), "libx265");
    assert_eq!(VideoCodec::Vp8.ffmpeg_codec_name(), "libvpx");
    assert_eq!(VideoCodec::Vp9.ffmpeg_codec_name(), "libvpx-vp9");
    assert_eq!(VideoCodec::Av1.ffmpeg_codec_name(), "libsvtav1");
    assert_eq!(VideoCodec::ProRes.ffmpeg_codec_name(), "prores_ks");
    assert_eq!(VideoCodec::Gif.ffmpeg_codec_name(), "gif");
}

#[test]
fn test_video_config_creation_and_resolution() {
    let cfg_mp4 = VideoConfig::new("output.mp4", 1920, 1080, 60);
    assert_eq!(cfg_mp4.width, 1920);
    assert_eq!(cfg_mp4.height, 1080);
    assert_eq!(cfg_mp4.framerate, 60);
    assert_eq!(cfg_mp4.codec, VideoCodec::H264);
    assert_eq!(cfg_mp4.resolve_codec(), VideoCodec::H264);

    let mut cfg_auto = VideoConfig::new("output.webm", 1280, 720, 30);
    cfg_auto.codec = VideoCodec::Auto;
    assert_eq!(cfg_auto.resolve_codec(), VideoCodec::Vp9);

    let mut cfg_custom = VideoConfig::new("output.mkv", 800, 600, 24);
    cfg_custom.codec = VideoCodec::H265;
    assert_eq!(cfg_custom.resolve_codec(), VideoCodec::H265);
}

#[test]
fn test_ffmpeg_availability_detection() {
    // Normal ffmpeg path on this machine
    let available = VideoExporter::is_ffmpeg_available(None);
    assert!(available);

    // Nonexistent binary should return false
    let nonexistent = VideoExporter::is_ffmpeg_available(Some("nonexistent_binary_xyz_123"));
    assert!(!nonexistent);
}

#[test]
fn test_video_exporter_missing_ffmpeg_error() {
    let mut config = VideoConfig::new("test.mp4", 640, 480, 30);
    config.ffmpeg_path = Some("nonexistent_binary_xyz_123".to_string());

    let res = VideoExporter::new(config);
    assert!(res.is_err());
    let err = res.err().unwrap();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    assert!(err.to_string().contains("ffmpeg executable not found"));
}

#[test]
fn test_video_exporter_invalid_frame_buffer_size() {
    let temp = NamedTempFile::new().unwrap();
    let path = temp
        .path()
        .with_extension("gif")
        .to_str()
        .unwrap()
        .to_string();

    let mut config = VideoConfig::new(path, 16, 16, 10);
    config.codec = VideoCodec::Gif;
    config.ffmpeg_path = Some("nonexistent".to_string()); // Force pure-Rust GIF backend

    let mut exporter = VideoExporter::new(config).expect("create gif exporter");
    assert_eq!(exporter.config().width, 16);

    // Provide 10 bytes instead of 16 * 16 * 4 = 1024 bytes
    let bad_buf = vec![0u8; 10];
    let res = exporter.write_frame_rgba(16, 16, &bad_buf);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    let _ = exporter.finish();
}

#[test]
fn test_pure_rust_gif_export() {
    let temp = NamedTempFile::new().unwrap();
    let path = temp
        .path()
        .with_extension("gif")
        .to_str()
        .unwrap()
        .to_string();

    let width = 32;
    let height = 32;
    let mut config = VideoConfig::new(&path, width, height, 10);
    config.codec = VideoCodec::Gif;
    config.ffmpeg_path = Some("nonexistent_force_rust_backend".to_string());

    let mut exporter = VideoExporter::new(config).expect("create gif exporter");

    // Write 3 frames of alternating red, green, blue
    for frame_idx in 0..3 {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for pixel in rgba.chunks_exact_mut(4) {
            match frame_idx {
                0 => {
                    pixel[0] = 255;
                    pixel[3] = 255;
                }
                1 => {
                    pixel[1] = 255;
                    pixel[3] = 255;
                }
                _ => {
                    pixel[2] = 255;
                    pixel[3] = 255;
                }
            }
        }
        exporter
            .write_frame_rgba(width, height, &rgba)
            .expect("write frame");
    }

    exporter.flush().expect("flush frames");
    exporter.finish().expect("finish gif export");

    let metadata = fs::metadata(&path).expect("gif file exists");
    assert!(metadata.len() > 0, "GIF file should not be empty");

    // Verify written GIF can be opened and decoded
    let img_bytes = fs::read(&path).expect("read gif");
    assert!(img_bytes.starts_with(b"GIF89a") || img_bytes.starts_with(b"GIF87a"));
}

#[test]
fn test_ffmpeg_mp4_and_webm_export() {
    if !VideoExporter::is_ffmpeg_available(None) {
        return;
    }

    // 1. Test MP4 export
    let temp_mp4 = NamedTempFile::new().unwrap();
    let mp4_path = temp_mp4
        .path()
        .with_extension("mp4")
        .to_str()
        .unwrap()
        .to_string();

    let width = 64;
    let height = 64;
    let mut config_mp4 = VideoConfig::new(&mp4_path, width, height, 30);
    config_mp4.preset = Some("ultrafast".to_string());
    config_mp4.crf = Some(28);

    let mut exporter_mp4 = VideoExporter::new(config_mp4).expect("create mp4 exporter");

    let red_frame = vec![255u8; (width * height * 4) as usize];
    for _ in 0..5 {
        exporter_mp4
            .write_frame_rgba(width, height, &red_frame)
            .expect("write mp4 frame");
    }

    exporter_mp4.flush().expect("flush mp4");
    exporter_mp4.finish().expect("finish mp4 export");

    let mp4_len = fs::metadata(&mp4_path).expect("mp4 exists").len();
    assert!(mp4_len > 0, "MP4 video should be non-empty");

    // 2. Test WebM export via VideoSink trait
    let temp_webm = NamedTempFile::new().unwrap();
    let webm_path = temp_webm
        .path()
        .with_extension("webm")
        .to_str()
        .unwrap()
        .to_string();

    let mut config_webm = VideoConfig::new(&webm_path, width, height, 30);
    config_webm.codec = VideoCodec::Vp8; // fast VP8
    let mut sink: Box<dyn VideoSink> =
        Box::new(VideoExporter::new(config_webm).expect("create webm exporter"));

    for _ in 0..5 {
        sink.write_frame(width, height, &red_frame)
            .expect("write webm frame");
    }

    sink.flush().expect("flush webm");
    sink.finish().expect("finish webm export");

    let webm_len = fs::metadata(&webm_path).expect("webm exists").len();
    assert!(webm_len > 0, "WebM video should be non-empty");
}

#[test]
fn test_video_sink_ppm_exporter() {
    let temp = NamedTempFile::new().unwrap();
    let path = temp.path().to_str().unwrap();

    let ppm = gource_draw::PpmExporter::new(path).expect("create ppm exporter");
    let mut sink: Box<dyn VideoSink> = Box::new(ppm);

    let frame = vec![128u8; 16 * 16 * 4];
    sink.write_frame(16, 16, &frame)
        .expect("write frame to ppm sink");
    sink.flush().expect("flush ppm sink");
    sink.finish().expect("finish ppm sink");

    let meta = fs::metadata(path).expect("ppm file exists");
    assert!(meta.len() > 0);
}

#[test]
fn test_ffmpeg_additional_codecs() {
    if !VideoExporter::is_ffmpeg_available(None) {
        return;
    }

    let width = 32;
    let height = 32;
    let frame = vec![200u8; (width * height * 4) as usize];

    // Test H265
    {
        let temp = NamedTempFile::new().unwrap();
        let path = temp
            .path()
            .with_extension("mp4")
            .to_str()
            .unwrap()
            .to_string();
        let mut cfg = VideoConfig::new(path, width, height, 25);
        cfg.codec = VideoCodec::H265;
        cfg.crf = Some(28);
        cfg.preset = Some("ultrafast".to_string());
        cfg.bitrate = Some("500k".to_string());
        let mut exp = VideoExporter::new(cfg).expect("create h265 exporter");
        exp.write_frame_rgba(width, height, &frame).unwrap();
        exp.finish().unwrap();
    }

    // Test VP9
    {
        let temp = NamedTempFile::new().unwrap();
        let path = temp
            .path()
            .with_extension("webm")
            .to_str()
            .unwrap()
            .to_string();
        let mut cfg = VideoConfig::new(path, width, height, 25);
        cfg.codec = VideoCodec::Vp9;
        cfg.crf = Some(35);
        cfg.bitrate = Some("500k".to_string());
        let mut exp = VideoExporter::new(cfg).expect("create vp9 exporter");
        exp.write_frame_rgba(width, height, &frame).unwrap();
        exp.finish().unwrap();
    }

    // Test ProRes
    {
        let temp = NamedTempFile::new().unwrap();
        let path = temp
            .path()
            .with_extension("mov")
            .to_str()
            .unwrap()
            .to_string();
        let mut cfg = VideoConfig::new(path, width, height, 25);
        cfg.codec = VideoCodec::ProRes;
        let mut exp = VideoExporter::new(cfg).expect("create prores exporter");
        exp.write_frame_rgba(width, height, &frame).unwrap();
        exp.finish().unwrap();
    }

    // Test FFmpeg GIF mode
    {
        let temp = NamedTempFile::new().unwrap();
        let path = temp
            .path()
            .with_extension("gif")
            .to_str()
            .unwrap()
            .to_string();
        let mut cfg = VideoConfig::new(path, width, height, 10);
        cfg.codec = VideoCodec::Gif;
        // Uses ffmpeg since ffmpeg_path is default None and ffmpeg is available
        let mut exp = VideoExporter::new(cfg).expect("create ffmpeg gif exporter");
        exp.write_frame_rgba(width, height, &frame).unwrap();
        exp.finish().unwrap();
    }
}

#[test]
fn test_video_exporter_closed_and_error_handling() {
    let temp = NamedTempFile::new().unwrap();
    let path = temp
        .path()
        .with_extension("gif")
        .to_str()
        .unwrap()
        .to_string();

    let mut config = VideoConfig::new(path, 16, 16, 10);
    config.codec = VideoCodec::Gif;
    config.ffmpeg_path = Some("nonexistent".to_string());

    let mut exporter = VideoExporter::new(config).expect("create gif exporter");
    exporter.close();
    let bad_write = exporter.write_frame_rgba(16, 16, &[0u8; 16 * 16 * 4]);
    assert!(bad_write.is_err());
    assert_eq!(
        bad_write.unwrap_err().kind(),
        std::io::ErrorKind::NotConnected
    );

    let bad_flush = exporter.flush();
    assert!(bad_flush.is_ok());

    let boxed: Box<dyn VideoSink> = Box::new(exporter);
    boxed.finish().expect("finish");
}

#[test]
fn test_ffmpeg_av1_codec() {
    if !VideoExporter::is_ffmpeg_available(None) {
        return;
    }

    let width = 32;
    let height = 32;
    let frame = vec![200u8; (width * height * 4) as usize];

    let temp = NamedTempFile::new().unwrap();
    let path = temp
        .path()
        .with_extension("mkv")
        .to_str()
        .unwrap()
        .to_string();
    let mut cfg = VideoConfig::new(path, width, height, 25);
    cfg.codec = VideoCodec::Av1;
    cfg.crf = Some(40);
    let mut exp = VideoExporter::new(cfg).expect("create av1 exporter");
    exp.write_frame_rgba(width, height, &frame).unwrap();
    exp.finish().unwrap();
}

#[test]
fn test_gfx_facade_coverage() {
    let mut gfx = gource_draw::Gfx::default();
    let mut list = gource_draw::DrawList::new(gource_core::uvec2(800, 600));
    let face_id = gfx.fonts.default_face();
    let font_id = gfx.fonts.font(face_id, 14);
    let style = gource_draw::TextStyle::new(gource_core::Vec4::ONE);
    gfx.draw_text(
        &mut list,
        font_id,
        gource_core::Vec2::ZERO,
        "Hello Gfx",
        &style,
    );
    let width = gfx.text_width(font_id, "Hello Gfx");
    assert!(width > 0.0);
}
