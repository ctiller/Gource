//! Frame capture: `--output-ppm-stream` video frames and F12 screenshots.
//!
//! The C++ version reads the back buffer with `glReadPixels` right after
//! drawing. Here each captured frame is a Bevy [`Screenshot`] of the primary
//! window, spawned in the frame whose draw list should be captured.
//! Screenshots arrive asynchronously and possibly out of order, so video
//! frames are numbered and released to the [`PpmExporter`] strictly in order
//! through a [`ReorderBuffer`].

use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bevy::{
    prelude::*,
    render::{
        render_resource::TextureFormat,
        view::screenshot::{Screenshot, ScreenshotCaptured},
    },
};
use gource_draw::PpmExporter;

/// Video frames that may be requested but not yet written before the
/// simulation is held back (keeps memory bounded if the GPU or the output
/// pipe is slow).
pub const MAX_FRAMES_IN_FLIGHT: u64 = 6;

/// A frame whose successor has arrived this many frames ago is considered
/// lost (e.g. the window surface was unavailable) and is replaced by a copy
/// of the previous frame so the video keeps its timing.
pub const LOST_FRAME_LOOKAHEAD: u64 = 8;

/// A captured frame as tightly packed RGBA8 rows, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Convert a captured image (in the window surface format) to RGBA8.
/// Values are copied verbatim: the renderer writes gamma-encoded values, so
/// the bytes of an sRGB surface are already what the C++ `glReadPixels`
/// returned.
pub fn frame_from_image(image: &Image) -> Option<Frame> {
    let width = image.texture_descriptor.size.width;
    let height = image.texture_descriptor.size.height;
    let data = image.data.as_ref()?;
    let expected = width as usize * height as usize * 4;
    let rgba = match image.texture_descriptor.format {
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => {
            (data.len() == expected).then(|| data.clone())?
        }
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
            if data.len() != expected {
                return None;
            }
            let mut rgba = data.clone();
            for px in rgba.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
            rgba
        }
        _ => image.clone().try_into_dynamic().ok()?.to_rgba8().into_raw(),
    };
    Some(Frame {
        width,
        height,
        rgba,
    })
}

/// Save a screenshot as PNG: RGBA with `--transparent`, otherwise RGB
/// (`PNGWriter png(gGourceSettings.transparent ? 4 : 3)`).
pub fn save_png(path: &std::path::Path, frame: &Frame, with_alpha: bool) -> image::ImageResult<()> {
    let rgba = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone())
        .ok_or_else(|| {
            image::ImageError::Parameter(image::error::ParameterError::from_kind(
                image::error::ParameterErrorKind::DimensionMismatch,
            ))
        })?;
    if with_alpha {
        rgba.save_with_format(path, image::ImageFormat::Png)
    } else {
        image::DynamicImage::ImageRgba8(rgba)
            .to_rgb8()
            .save_with_format(path, image::ImageFormat::Png)
    }
}

/// Releases numbered items in order. Items may be inserted in any order;
/// [`ReorderBuffer::pop`] yields `next` once it is available. An item whose
/// successors are more than `lookahead` ahead is reported as lost.
#[derive(Debug)]
pub struct ReorderBuffer<T> {
    next: u64,
    pending: BTreeMap<u64, T>,
    lookahead: u64,
}

/// What [`ReorderBuffer::pop`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Popped<T> {
    /// The next item, in order.
    Item(T),
    /// The next item never arrived and was skipped.
    Lost(u64),
}

impl<T> ReorderBuffer<T> {
    pub fn new(lookahead: u64) -> Self {
        Self {
            next: 0,
            pending: BTreeMap::new(),
            lookahead,
        }
    }

    /// Index of the next item to be released.
    pub fn next_index(&self) -> u64 {
        self.next
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Store item `index`. Items older than the next index are dropped.
    pub fn insert(&mut self, index: u64, item: T) {
        if index >= self.next {
            self.pending.insert(index, item);
        }
    }

    pub fn pop(&mut self) -> Option<Popped<T>> {
        if let Some(item) = self.pending.remove(&self.next) {
            self.next += 1;
            return Some(Popped::Item(item));
        }
        let newest = *self.pending.keys().next_back()?;
        if newest > self.next + self.lookahead {
            let lost = self.next;
            self.next += 1;
            return Some(Popped::Lost(lost));
        }
        None
    }

    /// Give up on everything before `index` (used when shutting down).
    pub fn skip_to(&mut self, index: u64) {
        self.next = self.next.max(index);
        self.pending.retain(|&i, _| i >= index);
    }
}

/// Writes video frames in order to a [`PpmExporter`].
#[derive(Resource)]
pub struct Recorder {
    exporter: Option<PpmExporter>,
    /// Number assigned to the next requested video frame.
    requested: u64,
    received: Arc<Mutex<ReorderBuffer<Option<Frame>>>>,
    last_frame: Option<Frame>,
    written: u64,
    error: Option<String>,
}

impl Recorder {
    pub fn new(exporter: Option<PpmExporter>) -> Self {
        Self {
            exporter,
            requested: 0,
            received: Arc::new(Mutex::new(ReorderBuffer::new(LOST_FRAME_LOOKAHEAD))),
            last_frame: None,
            written: 0,
            error: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.exporter.is_some()
    }

    /// Video frames requested but not written yet.
    pub fn in_flight(&self) -> u64 {
        self.requested - self.written
    }

    pub fn frames_written(&self) -> u64 {
        self.written
    }

    /// The first write error, if any (the C++ exporter stops silently; we
    /// report it on exit).
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Reserve the number for a new video frame.
    pub fn next_frame_index(&mut self) -> u64 {
        let index = self.requested;
        self.requested += 1;
        index
    }

    /// Where captured frames are delivered (shared with the screenshot
    /// observers).
    pub fn sink(&self) -> Arc<Mutex<ReorderBuffer<Option<Frame>>>> {
        self.received.clone()
    }

    /// Write every frame that is ready, in order.
    pub fn flush(&mut self) {
        loop {
            let popped = {
                let mut received = self.received.lock().unwrap_or_else(|e| e.into_inner());
                received.pop()
            };
            let frame = match popped {
                None => break,
                Some(Popped::Item(Some(frame))) => Some(frame),
                Some(Popped::Item(None)) | Some(Popped::Lost(_)) => {
                    warn!("a video frame could not be captured; repeating the previous frame");
                    None
                }
            };
            if let Some(frame) = frame {
                self.last_frame = Some(frame);
            }
            if let Some(frame) = &self.last_frame {
                self.write(frame.clone());
            }
            self.written += 1;
        }
    }

    fn write(&mut self, frame: Frame) {
        if self.error.is_some() {
            return;
        }
        if let Some(exporter) = self.exporter.as_mut()
            && let Err(e) = exporter.write_frame_rgba(frame.width, frame.height, &frame.rgba)
        {
            self.error = Some(e.to_string());
        }
    }

    /// Stop waiting for outstanding frames and close the stream.
    pub fn finish(&mut self) -> io::Result<()> {
        self.flush();
        {
            let mut received = self.received.lock().unwrap_or_else(|e| e.into_inner());
            received.skip_to(self.requested);
        }
        self.written = self.requested;
        match self.exporter.take() {
            Some(exporter) => exporter.finish(),
            None => Ok(()),
        }
    }
}

/// What to do with the frame being drawn now.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CaptureJob {
    /// Video frame number, when recording this frame.
    pub video_frame: Option<u64>,
    /// F12 screenshot destination and whether to keep the alpha channel.
    pub screenshot: Option<(PathBuf, bool)>,
}

impl CaptureJob {
    pub fn is_empty(&self) -> bool {
        self.video_frame.is_none() && self.screenshot.is_none()
    }
}

/// Spawn the screenshot entity for `job`. Only one screenshot per window can
/// be taken per frame, so video capture and F12 share one.
pub fn spawn_capture(
    commands: &mut Commands,
    job: CaptureJob,
    sink: Arc<Mutex<ReorderBuffer<Option<Frame>>>>,
) {
    if job.is_empty() {
        return;
    }
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>| {
            let frame = frame_from_image(&captured.image);
            if let Some((path, with_alpha)) = &job.screenshot {
                match &frame {
                    Some(frame) => {
                        let (path, frame, with_alpha) = (path.clone(), frame.clone(), *with_alpha);
                        // Encoding a PNG takes a while; don't stall the frame.
                        std::thread::spawn(move || {
                            if let Err(e) = save_png(&path, &frame, with_alpha) {
                                error!("could not write screenshot {}: {e}", path.display());
                            }
                        });
                    }
                    None => error!("unsupported screenshot format"),
                }
            }
            if let Some(index) = job.video_frame {
                sink.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(index, frame);
            }
        },
    );
}

/// Bounded wait for outstanding captures when quitting.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(duration: Duration) -> Self {
        Self(Instant::now() + duration)
    }

    pub fn passed(&self) -> bool {
        Instant::now() >= self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension},
    };

    fn image(format: TextureFormat, data: Vec<u8>, width: u32, height: u32) -> Image {
        Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            format,
            RenderAssetUsages::MAIN_WORLD,
        )
    }

    #[test]
    fn bgra_is_swizzled_and_rgba_kept() {
        let bgra = image(
            TextureFormat::Bgra8UnormSrgb,
            vec![1, 2, 3, 4, 5, 6, 7, 8],
            2,
            1,
        );
        let frame = frame_from_image(&bgra).unwrap();
        assert_eq!(frame.rgba, vec![3, 2, 1, 4, 7, 6, 5, 8]);
        assert_eq!((frame.width, frame.height), (2, 1));

        let rgba = image(TextureFormat::Rgba8Unorm, vec![1, 2, 3, 4], 1, 1);
        assert_eq!(frame_from_image(&rgba).unwrap().rgba, vec![1, 2, 3, 4]);
    }

    #[test]
    fn wrong_sizes_are_rejected() {
        let mut bgra = image(TextureFormat::Bgra8Unorm, vec![0; 8], 2, 1);
        bgra.data = Some(vec![0; 4]);
        assert!(frame_from_image(&bgra).is_none());
        let mut rgba = image(TextureFormat::Rgba8UnormSrgb, vec![0; 8], 2, 1);
        rgba.data = Some(vec![0; 4]);
        assert!(frame_from_image(&rgba).is_none());
        rgba.data = None;
        assert!(frame_from_image(&rgba).is_none());
    }

    #[test]
    fn other_formats_convert_through_image_crate() {
        let luma = image(TextureFormat::R8Unorm, vec![7, 9], 2, 1);
        let frame = frame_from_image(&luma).unwrap();
        assert_eq!(frame.rgba, vec![7, 7, 7, 255, 9, 9, 9, 255]);

        // Formats Bevy can't convert are reported, not guessed.
        let half = image(TextureFormat::Rgba16Float, vec![0; 8], 1, 1);
        assert!(frame_from_image(&half).is_none());
    }

    #[test]
    fn reorder_buffer_releases_in_order() {
        let mut buffer = ReorderBuffer::new(3);
        buffer.insert(1, 'b');
        assert_eq!(buffer.pop(), None);
        buffer.insert(0, 'a');
        assert_eq!(buffer.pop(), Some(Popped::Item('a')));
        assert_eq!(buffer.pop(), Some(Popped::Item('b')));
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.next_index(), 2);
        buffer.insert(0, 'z');
        assert_eq!(buffer.pending(), 0, "stale items are dropped");
    }

    #[test]
    fn reorder_buffer_reports_lost_items() {
        let mut buffer = ReorderBuffer::new(2);
        buffer.insert(3, 'd');
        assert_eq!(buffer.pop(), Some(Popped::Lost(0)));
        assert_eq!(buffer.pop(), None, "3 is within lookahead of 1");
        buffer.skip_to(3);
        assert_eq!(buffer.pop(), Some(Popped::Item('d')));
    }

    fn frame(v: u8) -> Frame {
        Frame {
            width: 1,
            height: 1,
            rgba: vec![v, v, v, 255],
        }
    }

    #[test]
    fn recorder_writes_frames_in_order_and_repeats_lost_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.ppm");
        let exporter = PpmExporter::new(path.to_str().unwrap()).unwrap();
        let mut recorder = Recorder::new(Some(exporter));
        assert!(recorder.is_recording());
        let sink = recorder.sink();
        let indices: Vec<u64> = (0..4).map(|_| recorder.next_frame_index()).collect();
        assert_eq!(indices, vec![0, 1, 2, 3]);
        assert_eq!(recorder.in_flight(), 4);

        sink.lock().unwrap().insert(1, Some(frame(20)));
        sink.lock().unwrap().insert(0, Some(frame(10)));
        sink.lock().unwrap().insert(2, None);
        recorder.flush();
        assert_eq!(recorder.frames_written(), 3);
        assert_eq!(recorder.in_flight(), 1);

        recorder.finish().unwrap();
        assert_eq!(recorder.in_flight(), 0);
        assert!(recorder.error().is_none());

        let data = std::fs::read(&path).unwrap();
        let header = b"P6\n1 1 255\n";
        let mut expected = Vec::new();
        for v in [10u8, 20, 20] {
            expected.extend_from_slice(header);
            expected.extend_from_slice(&[v, v, v]);
        }
        assert_eq!(data, expected);
    }

    #[test]
    fn recorder_without_exporter() {
        let mut recorder = Recorder::new(None);
        assert!(!recorder.is_recording());
        recorder.next_frame_index();
        recorder.sink().lock().unwrap().insert(0, Some(frame(1)));
        recorder.flush();
        assert_eq!(recorder.frames_written(), 1);
        recorder.finish().unwrap();
    }

    #[test]
    fn png_with_and_without_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let rgb_path = dir.path().join("rgb.png");
        let rgba_path = dir.path().join("rgba.png");
        let f = Frame {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 128, 0, 255, 0, 255],
        };
        save_png(&rgb_path, &f, false).unwrap();
        save_png(&rgba_path, &f, true).unwrap();
        let rgb = image::open(&rgb_path).unwrap();
        assert_eq!(rgb.color(), image::ColorType::Rgb8);
        assert_eq!(rgb.to_rgb8().get_pixel(0, 0).0, [255, 0, 0]);
        let rgba = image::open(&rgba_path).unwrap();
        assert_eq!(rgba.color(), image::ColorType::Rgba8);
        assert_eq!(rgba.to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 128]);

        let bad = Frame {
            width: 3,
            height: 3,
            rgba: vec![0; 4],
        };
        assert!(save_png(&rgb_path, &bad, false).is_err());
    }

    #[test]
    fn capture_job_and_deadline() {
        assert!(CaptureJob::default().is_empty());
        let job = CaptureJob {
            video_frame: Some(1),
            screenshot: None,
        };
        assert!(!job.is_empty());
        assert!(Deadline::after(Duration::ZERO).passed());
        assert!(!Deadline::after(Duration::from_secs(60)).passed());
    }
}
