//! PPM video frame stream writer (port of `core/ppm.cpp`'s PPMExporter).
//!
//! Frames are written as consecutive binary PPM images (`P6`), which tools
//! like ffmpeg read with `-f image2pipe -vcodec ppm`. Writing happens on a
//! background thread so rendering is not blocked by I/O.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

struct Frame {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
}

enum Message {
    WriteFrame(Frame),
    Flush(SyncSender<io::Result<()>>),
}

/// The first write error of the worker thread (kind and message).
type SharedError = Arc<Mutex<Option<(io::ErrorKind, String)>>>;

/// Writes a stream of frames to a file, or to stdout when the path is `-`.
pub struct PpmExporter {
    sender: Option<SyncSender<Message>>,
    handle: Option<JoinHandle<io::Result<()>>>,
    error: SharedError,
}

impl PpmExporter {
    /// Open the output. Fails if the file cannot be created.
    pub fn new(path: &str) -> io::Result<Self> {
        let writer: Box<dyn Write + Send> = if path == "-" {
            Box::new(BufWriter::new(io::stdout()))
        } else {
            let file = File::create(path)?;
            Box::new(BufWriter::new(file))
        };

        // Bounded channel: allows up to 2 frames in-flight for back-pressure
        let (sender, receiver) = sync_channel::<Message>(2);

        let error = SharedError::default();
        let worker_error = error.clone();
        let handle = thread::spawn(move || Self::worker(receiver, writer, worker_error));

        Ok(Self {
            sender: Some(sender),
            handle: Some(handle),
            error,
        })
    }

    fn worker(
        receiver: Receiver<Message>,
        mut writer: Box<dyn Write + Send>,
        shared: SharedError,
    ) -> io::Result<()> {
        let mut last_err: Option<io::Error> = None;
        let record = |e: &io::Error| {
            let mut shared = shared.lock().unwrap_or_else(|p| p.into_inner());
            shared.get_or_insert_with(|| (e.kind(), e.to_string()));
        };

        while let Ok(msg) = receiver.recv() {
            match msg {
                Message::WriteFrame(frame) => {
                    if last_err.is_none() {
                        let header = format!("P6\n{} {} 255\n", frame.width, frame.height);
                        if let Err(e) = writer
                            .write_all(header.as_bytes())
                            .and_then(|_| writer.write_all(&frame.rgb))
                        {
                            record(&e);
                            last_err = Some(e);
                        }
                    }
                }
                Message::Flush(ack) => {
                    if let Some(ref e) = last_err {
                        let _ = ack.send(Err(io::Error::new(e.kind(), e.to_string())));
                    } else {
                        let res = writer.flush();
                        if let Err(ref e) = res {
                            record(e);
                            last_err = Some(io::Error::new(e.kind(), e.to_string()));
                        }
                        let _ = ack.send(res);
                    }
                }
            }
        }

        if let Some(e) = last_err {
            return Err(e);
        }

        writer.flush()?;
        Ok(())
    }

    /// The first error the writer thread ran into, if any. Once set, frames
    /// are no longer written and [`PpmExporter::write_frame_rgba`] fails.
    pub fn error(&self) -> Option<io::Error> {
        let shared = self.error.lock().unwrap_or_else(|p| p.into_inner());
        shared
            .as_ref()
            .map(|(kind, message)| io::Error::new(*kind, message.clone()))
    }

    /// Queue one frame. `rgba` is top-down RGBA8 (`width * height * 4`
    /// bytes); alpha is dropped. The header must match the C++ exporter
    /// byte for byte (see `PPMExporter` in core/ppm.cpp).
    pub fn write_frame_rgba(&mut self, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
        if let Some(e) = self.error() {
            return Err(e);
        }

        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "frame size overflow"))?;

        if rgba.len() != expected_len {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "buffer length {} does not match width * height * 4 = {} ({}x{})",
                    rgba.len(),
                    expected_len,
                    width,
                    height
                ),
            ));
        }

        // Convert RGBA to RGB (drop alpha)
        let pixel_count = (width * height) as usize;
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        for i in 0..pixel_count {
            let src = i * 4;
            rgb.push(rgba[src]);
            rgb.push(rgba[src + 1]);
            rgb.push(rgba[src + 2]);
        }

        let sender = self
            .sender
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "exporter already closed"))?;

        sender
            .send(Message::WriteFrame(Frame { width, height, rgb }))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "writer thread disconnected"))?;

        Ok(())
    }

    /// Flush all queued frames and close the output.
    pub fn finish(mut self) -> io::Result<()> {
        self.flush_and_close()
    }

    fn flush_and_close(&mut self) -> io::Result<()> {
        if let Some(sender) = self.sender.take() {
            let (ack_tx, ack_rx) = sync_channel(1);
            let _ = sender.send(Message::Flush(ack_tx));
            let flush_res = ack_rx.recv().unwrap_or(Ok(()));
            drop(sender);

            let join_res = if let Some(handle) = self.handle.take() {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(io::Error::other("writer thread panicked")))
            } else {
                Ok(())
            };

            flush_res.and(join_res)
        } else {
            Ok(())
        }
    }
}

impl Drop for PpmExporter {
    fn drop(&mut self) {
        let _ = self.flush_and_close();
    }
}
