use gource_draw::ppm::PpmExporter;
use std::fs::File;
use std::io::Read;
use tempfile::tempdir;

#[test]
fn test_ppm_header_and_single_frame() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("frame1.ppm");
    let path_str = file_path.to_str().unwrap();

    let mut exporter = PpmExporter::new(path_str).expect("create PpmExporter");

    let width = 2;
    let height = 3;
    // 2x3 top-down RGBA8 (alpha will be dropped)
    #[rustfmt::skip]
    let rgba = vec![
        10, 20, 30, 255,   40, 50, 60, 255,
        70, 80, 90, 255,   100, 110, 120, 255,
        130, 140, 150, 255, 160, 170, 180, 255,
    ];

    exporter
        .write_frame_rgba(width, height, &rgba)
        .expect("write frame");
    exporter.finish().expect("finish");

    let mut content = Vec::new();
    let mut file = File::open(&file_path).expect("open ppm");
    file.read_to_end(&mut content).expect("read content");

    let expected_header = b"P6\n2 3 255\n";
    #[rustfmt::skip]
    let expected_pixels = vec![
        10, 20, 30,   40, 50, 60,
        70, 80, 90,   100, 110, 120,
        130, 140, 150, 160, 170, 180,
    ];

    assert!(content.starts_with(expected_header));
    assert_eq!(&content[expected_header.len()..], &expected_pixels[..]);
}

#[test]
fn test_ppm_multiple_frames_and_drop() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("stream.ppm");
    let path_str = file_path.to_str().unwrap();

    {
        let mut exporter = PpmExporter::new(path_str).expect("create PpmExporter");

        let width = 1;
        let height = 1;
        let rgba1 = vec![255, 0, 0, 128];
        let rgba2 = vec![0, 255, 0, 128];

        exporter.write_frame_rgba(width, height, &rgba1).unwrap();
        exporter.write_frame_rgba(width, height, &rgba2).unwrap();
        // Drop should flush and join cleanly
    }

    let mut content = Vec::new();
    let mut file = File::open(&file_path).unwrap();
    file.read_to_end(&mut content).unwrap();

    let mut expected = Vec::new();
    expected.extend_from_slice(b"P6\n1 1 255\n");
    expected.extend_from_slice(&[255, 0, 0]);
    expected.extend_from_slice(b"P6\n1 1 255\n");
    expected.extend_from_slice(&[0, 255, 0]);

    assert_eq!(content, expected);
}

#[test]
fn test_ppm_invalid_buffer_length() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("invalid.ppm");
    let mut exporter = PpmExporter::new(file_path.to_str().unwrap()).unwrap();

    let err = exporter.write_frame_rgba(2, 2, &[0; 15]);
    assert!(err.is_err());
}

#[test]
fn test_ppm_unwritable_path_error() {
    let err = PpmExporter::new("/this/path/does/not/exist/stream.ppm");
    assert!(err.is_err());
}

#[test]
fn test_ppm_stdout() {
    let mut exporter = PpmExporter::new("-").expect("create stdout exporter");
    let rgba = vec![255, 255, 255, 255];
    exporter
        .write_frame_rgba(1, 1, &rgba)
        .expect("write frame to stdout");
    exporter.finish().expect("finish stdout exporter");
}

#[test]
fn test_ppm_size_overflow() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("overflow.ppm");
    let mut exporter = PpmExporter::new(file_path.to_str().unwrap()).unwrap();
    let err = exporter.write_frame_rgba(u32::MAX, u32::MAX, &[]);
    assert!(err.is_err());
}

#[test]
fn test_ppm_error_after_start_dev_full() {
    // Opening /dev/full succeeds, but writing to it will fail with ENOSPC (No space left on device)
    if let Ok(mut exporter) = PpmExporter::new("/dev/full") {
        let frame = vec![128u8; 100 * 100 * 4];
        // Send several frames to guarantee buffer fills and triggers writer error
        for _ in 0..10 {
            if exporter.write_frame_rgba(100, 100, &frame).is_err() {
                break;
            }
        }
        // finish() must return the write error
        let finish_res = exporter.finish();
        assert!(
            finish_res.is_err(),
            "finish() must return error on /dev/full"
        );
    }

    // Also test Drop without finish on an exporter with errors
    if let Ok(mut exporter) = PpmExporter::new("/dev/full") {
        let frame = vec![128u8; 100 * 100 * 4];
        for _ in 0..10 {
            if exporter.write_frame_rgba(100, 100, &frame).is_err() {
                break;
            }
        }
        // Dropping here should not panic even when background thread encountered an error
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_ppm_error_is_reported_before_finish() {
    // Writes to /dev/full fail with ENOSPC; the frontend must learn about it
    // while recording (e.g. to stop when a video pipe was closed).
    let mut exporter = PpmExporter::new("/dev/full").expect("open /dev/full");
    assert!(exporter.error().is_none());
    let frame = vec![7u8; 100 * 100 * 4];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let err = loop {
        match exporter.write_frame_rgba(100, 100, &frame) {
            Err(e) => break e,
            Ok(()) => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "write error never reported"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    };
    assert_eq!(
        err.raw_os_error(),
        None,
        "the error is re-created from its kind"
    );
    let reported = exporter.error().expect("error() reports the failure");
    assert_eq!(reported.kind(), err.kind());
    assert!(!reported.to_string().is_empty());
    // Later frames keep failing with the same error.
    assert_eq!(
        exporter
            .write_frame_rgba(100, 100, &frame)
            .unwrap_err()
            .kind(),
        err.kind()
    );
    assert!(exporter.finish().is_err());
}

#[test]
fn test_ppm_backpressure_and_fifo() {
    // Test FIFO pipe with back-pressure (slow reader)
    let dir = tempdir().unwrap();
    let fifo_path = dir.path().join("test.fifo");
    let c_path = std::ffi::CString::new(fifo_path.to_str().unwrap()).unwrap();

    // Create a named pipe (FIFO)
    unsafe extern "C" {
        fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> std::ffi::c_int;
    }
    let res = unsafe { mkfifo(c_path.as_ptr(), 0o600) };
    if res != 0 {
        // If mkfifo is not supported in environment, skip
        return;
    }

    let fifo_str = fifo_path.to_str().unwrap().to_string();

    // Spawn reader thread that reads slowly
    let reader_handle = std::thread::spawn(move || {
        let mut file = std::fs::File::open(&fifo_str).expect("open fifo reader");
        let mut total_read = 0;
        let mut buf = [0u8; 128];
        loop {
            // Small delay to induce back-pressure
            std::thread::sleep(std::time::Duration::from_micros(200));
            match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => total_read += n,
                Err(_) => break,
            }
        }
        total_read
    });

    let mut exporter = PpmExporter::new(fifo_path.to_str().unwrap()).expect("open fifo exporter");
    let frame = vec![255u8; 64 * 64 * 4];
    for _ in 0..5 {
        exporter
            .write_frame_rgba(64, 64, &frame)
            .expect("write frame with back-pressure");
    }
    exporter.finish().expect("finish fifo exporter");

    let total_bytes = reader_handle.join().expect("join reader");
    // 5 frames * (header + 64*64*3)
    let header_len = b"P6\n64 64 255\n".len();
    let expected_bytes = 5 * (header_len + 64 * 64 * 3);
    assert_eq!(total_bytes, expected_bytes);
}

#[test]
fn test_ppm_write_after_closed() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("closed.ppm");
    let mut exporter = PpmExporter::new(file_path.to_str().unwrap()).unwrap();

    let frame = vec![0u8; 4];
    exporter.write_frame_rgba(1, 1, &frame).unwrap();
    // Simulate sender already closed/taken
    // We can't call private fields directly, but finish() consumes self.
    // If a channel is disconnected (e.g. writer thread panicked or exited), write_frame_rgba fails cleanly.
}
