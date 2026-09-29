use gource_draw::PpmExporter;

#[test]
fn test_ppm_flush_failure_on_dev_full() {
    // Opening /dev/full succeeds, but flushing bytes to it fails with ENOSPC.
    // By writing a small payload that fits inside BufWriter's internal buffer (8KB),
    // write_frame_rgba does not fail immediately during worker's write_all.
    // When finish() is called, it sends Message::Flush, which triggers writer.flush(),
    // hitting lines 89-90 in crates/gource-draw/src/ppm.rs:
    //   if let Err(ref e) = res {
    //       record(e);
    //       last_err = Some(io::Error::new(e.kind(), e.to_string()));
    //   }
    if let Ok(mut exporter) = PpmExporter::new("/dev/full") {
        let frame = vec![42u8; 4];
        if exporter.write_frame_rgba(1, 1, &frame).is_ok() {
            let res = exporter.finish();
            assert!(
                res.is_err(),
                "finish() must fail when flushing buffer to /dev/full"
            );
        }
    }
}
