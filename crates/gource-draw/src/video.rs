//! Video encoding and export pipeline.
//!
//! Provides [`VideoExporter`], supporting direct video output to MP4, WebM, MKV,
//! MOV, AVI, and GIF formats. Can encode via ffmpeg with hardware/software codecs
//! or pure-Rust GIF encoding fallback.

use crate::ppm::PpmExporter;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Supported video codecs for video export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoCodec {
    /// Automatically selected based on the file extension.
    #[default]
    Auto,
    /// H.264 (AVC) via libx264.
    H264,
    /// H.265 (HEVC) via libx265.
    H265,
    /// VP8 via libvpx.
    Vp8,
    /// VP9 via libvpx-vp9.
    Vp9,
    /// AV1 via libsvtav1 / libaom-av1.
    Av1,
    /// Apple ProRes via prores_ks.
    ProRes,
    /// Animated GIF.
    Gif,
}

impl VideoCodec {
    /// Infers the default codec from an output file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "mp4" | "m4v" => Self::H264,
            "webm" => Self::Vp9,
            "mkv" => Self::H264,
            "mov" => Self::H264,
            "avi" => Self::H264,
            "gif" => Self::Gif,
            _ => Self::H264,
        }
    }

    /// Parses a codec name from a string.
    pub fn parse_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().trim() {
            "auto" => Some(Self::Auto),
            "h264" | "x264" | "avc" => Some(Self::H264),
            "h265" | "x265" | "hevc" => Some(Self::H265),
            "vp8" => Some(Self::Vp8),
            "vp9" => Some(Self::Vp9),
            "av1" | "svtav1" => Some(Self::Av1),
            "prores" => Some(Self::ProRes),
            "gif" => Some(Self::Gif),
            _ => None,
        }
    }

    /// Returns the ffmpeg codec library name.
    pub fn ffmpeg_codec_name(&self) -> &'static str {
        match self {
            Self::Auto | Self::H264 => "libx264",
            Self::H265 => "libx265",
            Self::Vp8 => "libvpx",
            Self::Vp9 => "libvpx-vp9",
            Self::Av1 => "libsvtav1",
            Self::ProRes => "prores_ks",
            Self::Gif => "gif",
        }
    }
}

/// Configuration for the video export stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoConfig {
    /// Output file path.
    pub path: String,
    /// Video frame width in pixels.
    pub width: u32,
    /// Video frame height in pixels.
    pub height: u32,
    /// Frames per second.
    pub framerate: u32,
    /// Video codec.
    pub codec: VideoCodec,
    /// Optional target bitrate (e.g. "10M", "5000k").
    pub bitrate: Option<String>,
    /// Optional Constant Rate Factor (CRF).
    pub crf: Option<u32>,
    /// Optional encoder preset (e.g. "medium", "fast").
    pub preset: Option<String>,
    /// Optional custom path to ffmpeg executable.
    pub ffmpeg_path: Option<String>,
}

impl VideoConfig {
    /// Creates a video config for `path` with default settings.
    pub fn new(path: impl Into<String>, width: u32, height: u32, framerate: u32) -> Self {
        let path = path.into();
        let ext = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let codec = VideoCodec::from_extension(ext);
        Self {
            path,
            width,
            height,
            framerate: framerate.max(1),
            codec,
            bitrate: None,
            crf: None,
            preset: None,
            ffmpeg_path: None,
        }
    }

    /// Resolve the active video codec.
    pub fn resolve_codec(&self) -> VideoCodec {
        if self.codec == VideoCodec::Auto {
            let ext = Path::new(&self.path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            VideoCodec::from_extension(ext)
        } else {
            self.codec
        }
    }
}

/// Universal trait for sinks accepting video frames.
pub trait VideoSink: Send + Sync {
    /// Writes a single frame of dimensions `width` x `height` with packed RGBA8 data.
    fn write_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> io::Result<()>;
    /// Flushes any pending in-flight frames.
    fn flush(&mut self) -> io::Result<()>;
    /// Completes the video stream and finalizes the output file.
    fn finish(self: Box<Self>) -> io::Result<()>;
}

impl VideoSink for PpmExporter {
    fn write_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
        self.write_frame_rgba(width, height, rgba)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> io::Result<()> {
        (*self).finish()
    }
}

struct VideoFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

enum VideoMessage {
    WriteFrame(VideoFrame),
    Flush(SyncSender<io::Result<()>>),
}

/// The first write error of the worker thread.
type SharedVideoError = Arc<Mutex<Option<(io::ErrorKind, String)>>>;

/// Video exporter that streams frames into an encoder backend.
pub struct VideoExporter {
    config: VideoConfig,
    sender: Option<SyncSender<VideoMessage>>,
    handle: Option<JoinHandle<io::Result<()>>>,
    error: SharedVideoError,
}

impl VideoExporter {
    /// Detects if an ffmpeg executable is available on the system.
    pub fn is_ffmpeg_available(custom_path: Option<&str>) -> bool {
        let prog = custom_path.unwrap_or("ffmpeg");
        Command::new(prog)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Opens a new video export stream with the specified configuration.
    pub fn new(config: VideoConfig) -> io::Result<Self> {
        let resolved_codec = config.resolve_codec();
        let is_gif = resolved_codec == VideoCodec::Gif;
        let ffmpeg_found = Self::is_ffmpeg_available(config.ffmpeg_path.as_deref());

        if !ffmpeg_found && !is_gif {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "ffmpeg executable not found; install ffmpeg to encode {} video to '{}'",
                    resolved_codec.ffmpeg_codec_name(),
                    config.path
                ),
            ));
        }

        // Bounded channel: 2 frames in-flight for back-pressure
        let (sender, receiver) = sync_channel::<VideoMessage>(2);
        let error = SharedVideoError::default();
        let worker_error = error.clone();
        let worker_config = config.clone();

        let handle = thread::Builder::new()
            .name("video-encoder".to_string())
            .spawn(move || Self::worker(receiver, worker_config, worker_error))?;

        Ok(Self {
            config,
            sender: Some(sender),
            handle: Some(handle),
            error,
        })
    }

    /// Configuration for this video exporter.
    pub fn config(&self) -> &VideoConfig {
        &self.config
    }

    fn worker(
        receiver: Receiver<VideoMessage>,
        config: VideoConfig,
        error: SharedVideoError,
    ) -> io::Result<()> {
        let resolved_codec = config.resolve_codec();
        let ffmpeg_found = Self::is_ffmpeg_available(config.ffmpeg_path.as_deref());

        if resolved_codec == VideoCodec::Gif && !ffmpeg_found {
            // Pure-Rust GIF fallback
            Self::gif_worker(receiver, config, error)
        } else {
            // FFmpeg subprocess encoder
            Self::ffmpeg_worker(receiver, config, error)
        }
    }

    fn ffmpeg_worker(
        receiver: Receiver<VideoMessage>,
        config: VideoConfig,
        error: SharedVideoError,
    ) -> io::Result<()> {
        let prog = config.ffmpeg_path.as_deref().unwrap_or("ffmpeg");
        let codec = config.resolve_codec();

        let mut cmd = Command::new(prog);
        cmd.arg("-y") // Overwrite output without prompting
            .arg("-f")
            .arg("rawvideo")
            .arg("-pix_fmt")
            .arg("rgba")
            .arg("-s")
            .arg(format!("{}x{}", config.width, config.height))
            .arg("-r")
            .arg(config.framerate.to_string())
            .arg("-i")
            .arg("-"); // Read raw frames from stdin

        match codec {
            VideoCodec::Auto | VideoCodec::H264 => {
                cmd.arg("-c:v").arg("libx264");
                cmd.arg("-pix_fmt").arg("yuv420p");
                cmd.arg("-preset")
                    .arg(config.preset.as_deref().unwrap_or("medium"));
                cmd.arg("-crf").arg(config.crf.unwrap_or(18).to_string());
            }
            VideoCodec::H265 => {
                cmd.arg("-c:v").arg("libx265");
                cmd.arg("-pix_fmt").arg("yuv420p");
                cmd.arg("-crf").arg(config.crf.unwrap_or(22).to_string());
            }
            VideoCodec::Vp8 => {
                cmd.arg("-c:v").arg("libvpx");
                cmd.arg("-pix_fmt").arg("yuv420p");
            }
            VideoCodec::Vp9 => {
                cmd.arg("-c:v").arg("libvpx-vp9");
                cmd.arg("-pix_fmt").arg("yuv420p");
                cmd.arg("-crf").arg(config.crf.unwrap_or(30).to_string());
                cmd.arg("-b:v")
                    .arg(config.bitrate.as_deref().unwrap_or("0"));
            }
            VideoCodec::Av1 => {
                // Try libsvtav1 or fallback
                cmd.arg("-c:v").arg("libsvtav1");
                cmd.arg("-pix_fmt").arg("yuv420p");
                cmd.arg("-crf").arg(config.crf.unwrap_or(32).to_string());
            }
            VideoCodec::ProRes => {
                cmd.arg("-c:v").arg("prores_ks");
                cmd.arg("-pix_fmt").arg("yuv422p10le");
            }
            VideoCodec::Gif => {
                cmd.arg("-vf")
                    .arg("split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse");
            }
        }

        if let Some(ref br) = config.bitrate
            && codec != VideoCodec::Vp9
        {
            cmd.arg("-b:v").arg(br);
        }

        cmd.arg(&config.path);
        cmd.stdin(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("failed to launch ffmpeg: {e}");
                *error.lock().unwrap() = Some((e.kind(), err_msg));
                return Err(e);
            }
        };

        let mut stdin = child.stdin.take().expect("piped stdin");
        let stderr = child.stderr.take();

        // Stderr reader thread to capture error messages if ffmpeg fails
        let stderr_handle = thread::spawn(move || {
            let mut err_str = String::new();
            if let Some(mut se) = stderr {
                let _ = std::io::Read::read_to_string(&mut se, &mut err_str);
            }
            err_str
        });

        let mut write_error = None;
        while let Ok(msg) = receiver.recv() {
            match msg {
                VideoMessage::WriteFrame(frame) => {
                    if let Err(e) = stdin.write_all(&frame.rgba) {
                        write_error = Some(e);
                        break;
                    }
                }
                VideoMessage::Flush(ack) => {
                    let res = stdin.flush();
                    let _ = ack.send(res);
                }
            }
        }

        // Close stdin so ffmpeg reaches EOF and finalizes container
        drop(stdin);

        let status = match child.wait() {
            Ok(s) => s,
            Err(e) => {
                let err_msg = format!("error waiting on ffmpeg: {e}");
                *error.lock().unwrap() = Some((e.kind(), err_msg));
                return Err(e);
            }
        };

        let stderr_out = stderr_handle.join().unwrap_or_default();

        if let Some(e) = write_error {
            let err_msg = format!(
                "failed to write frame to ffmpeg: {e}. Stderr logs: {}",
                stderr_out.trim()
            );
            *error.lock().unwrap() = Some((e.kind(), err_msg.clone()));
            return Err(io::Error::new(e.kind(), err_msg));
        }

        if !status.success() {
            let err_msg = format!(
                "ffmpeg encoding failed with exit code {:?}: {}",
                status.code(),
                stderr_out.trim()
            );
            *error.lock().unwrap() = Some((io::ErrorKind::Other, err_msg.clone()));
            return Err(io::Error::other(err_msg));
        }

        Ok(())
    }

    fn gif_worker(
        receiver: Receiver<VideoMessage>,
        config: VideoConfig,
        error: SharedVideoError,
    ) -> io::Result<()> {
        let file = match File::create(&config.path) {
            Ok(f) => f,
            Err(e) => {
                let err_msg = format!("failed to create GIF output file: {e}");
                *error.lock().unwrap() = Some((e.kind(), err_msg));
                return Err(e);
            }
        };
        let mut writer = BufWriter::new(file);

        use image::codecs::gif::{GifEncoder, Repeat};
        use image::{Delay, Frame as ImgFrame, RgbaImage};

        let delay_ms = 1000 / config.framerate.max(1);
        let delay = Delay::from_numer_denom_ms(delay_ms, 1);

        {
            let mut encoder = GifEncoder::new(&mut writer);
            let _ = encoder.set_repeat(Repeat::Infinite);

            while let Ok(msg) = receiver.recv() {
                match msg {
                    VideoMessage::WriteFrame(frame) => {
                        let rgba_img =
                            match RgbaImage::from_raw(frame.width, frame.height, frame.rgba) {
                                Some(img) => img,
                                None => {
                                    let err_msg = "invalid buffer dimension for gif".to_string();
                                    *error.lock().unwrap() =
                                        Some((io::ErrorKind::InvalidData, err_msg));
                                    continue;
                                }
                            };
                        let img_frame = ImgFrame::from_parts(rgba_img, 0, 0, delay);
                        if let Err(e) = encoder.encode_frame(img_frame) {
                            let err_msg = format!("failed to encode gif frame: {e}");
                            *error.lock().unwrap() = Some((io::ErrorKind::Other, err_msg));
                            return Err(io::Error::other(e.to_string()));
                        }
                    }
                    VideoMessage::Flush(ack) => {
                        let _ = ack.send(Ok(()));
                    }
                }
            }
        }

        writer.flush()?;
        Ok(())
    }

    /// Check for worker errors.
    fn check_error(&self) -> io::Result<()> {
        if let Some((kind, msg)) = self.error.lock().unwrap().clone() {
            Err(io::Error::new(kind, msg))
        } else {
            Ok(())
        }
    }

    /// Enqueue a frame for video encoding.
    pub fn write_frame_rgba(&mut self, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
        self.check_error()?;
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "buffer size mismatch: expected {} bytes, got {}",
                    expected,
                    rgba.len()
                ),
            ));
        }

        let frame = VideoFrame {
            width,
            height,
            rgba: rgba.to_vec(),
        };

        if let Some(ref sender) = self.sender {
            sender.send(VideoMessage::WriteFrame(frame)).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "video worker thread disconnected",
                )
            })?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "video exporter is closed",
            ));
        }

        self.check_error()
    }

    /// Wait until all queued frames have been processed.
    pub fn flush(&mut self) -> io::Result<()> {
        self.check_error()?;
        let (ack_tx, ack_rx) = sync_channel(1);
        if let Some(ref sender) = self.sender {
            sender.send(VideoMessage::Flush(ack_tx)).map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "video worker disconnected")
            })?;
            ack_rx
                .recv()
                .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "flush dropped"))??;
        }
        self.check_error()
    }

    /// Explicitly closes the stream channel without joining the worker thread immediately.
    pub fn close(&mut self) {
        self.sender.take();
    }

    /// Finalizes the stream and waits for the worker process to complete.
    pub fn finish(mut self) -> io::Result<()> {
        let _ = self.flush();
        drop(self.sender.take());
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(res) => res?,
                Err(_) => {
                    return Err(io::Error::other("video encoder thread panicked"));
                }
            }
        }
        self.check_error()
    }
}

impl VideoSink for VideoExporter {
    fn write_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
        self.write_frame_rgba(width, height, rgba)
    }

    fn flush(&mut self) -> io::Result<()> {
        VideoExporter::flush(self)
    }

    fn finish(self: Box<Self>) -> io::Result<()> {
        (*self).finish()
    }
}
