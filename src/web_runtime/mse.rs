//! MediaSource Extensions (MSE) — adaptive streaming API.
//!
//! Spec: https://www.w3.org/TR/media-source/
//!
//! MSE allows JavaScript to dynamically construct media streams for playback
//! by `<video>` and `<audio>` elements. This is how YouTube, Netflix, and
//! Twitch deliver adaptive-bitrate video (DASH, HLS).
//!
//! Architecture:
//! ```js
//! var mediaSource = new MediaSource();
//! video.src = URL.createObjectURL(mediaSource);
//! mediaSource.addEventListener('sourceopen', () => {
//!   var sourceBuffer = mediaSource.addSourceBuffer('video/webm; codecs="vp9"');
//!   sourceBuffer.appendBuffer(arrayBuffer); // feed video chunks
//! });
//! ```
//!
//! # What this implements
//!
//! * `MediaSource` — represents a media stream being assembled.
//! * `SourceBuffer` — a buffer for one track (audio or video).
//! * `appendBuffer()` — appends media data (chunks from fetch).
//! * `remove()` — removes a time range from the buffer.
//! * `updateend` event — fires when append/remove completes.
//! * `Buffered` — time ranges currently buffered.
//! * `duration` — total duration of the media.
//!
//! # What this does NOT implement
//!
//! * Actual codec decoding (that's in `video.rs`).
//! * `URL.createObjectURL` integration with `<video>` playback.
//! * SourceBuffer list management for multiple tracks.
//! * MSE Object URL lifecycle.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// A MediaSource object — the entry point for MSE.
pub struct MediaSource {
    /// SourceBuffers attached to this MediaSource.
    pub source_buffers: Mutex<Vec<Arc<SourceBuffer>>>,
    /// Whether the MediaSource is ready (sourceopen has fired).
    pub ready_state: Mutex<ReadyState>,
    /// Total duration of the media in seconds.
    pub duration: Mutex<f64>,
    /// Callbacks for sourceopen event.
    on_source_open: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_source_ended: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_source_close: Mutex<Option<Box<dyn Fn() + Send>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyState {
    /// closed — MediaSource is not attached to a media element.
    Closed,
    /// open — MediaSource is attached and ready to accept data.
    Open,
    /// ended — endOfStream() has been called.
    Ended,
}

impl std::fmt::Debug for MediaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaSource")
            .field("source_buffers", &self.source_buffers.lock().unwrap().len())
            .field("ready_state", &*self.ready_state.lock().unwrap())
            .field("duration", &*self.duration.lock().unwrap())
            .finish()
    }
}

impl MediaSource {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            source_buffers: Mutex::new(Vec::new()),
            ready_state: Mutex::new(ReadyState::Closed),
            duration: Mutex::new(f64::NAN),
            on_source_open: Mutex::new(None),
            on_source_ended: Mutex::new(None),
            on_source_close: Mutex::new(None),
        })
    }

    /// Add a SourceBuffer with a specific MIME type and codec.
    /// Example: `addSourceBuffer('video/webm; codecs="vp9,opus"')`
    pub fn add_source_buffer(
        self: &Arc<Self>,
        mime_type: &str,
    ) -> Result<Arc<SourceBuffer>, String> {
        // Validate the MIME type.
        let codec = parse_mime_type(mime_type)?;
        let sb = SourceBuffer::new(codec);
        self.source_buffers.lock().unwrap().push(sb.clone());
        Ok(sb)
    }

    /// Remove a SourceBuffer.
    pub fn remove_source_buffer(self: &Arc<Self>, sb: &Arc<SourceBuffer>) {
        self.source_buffers
            .lock()
            .unwrap()
            .retain(|s| !Arc::ptr_eq(s, sb));
    }

    /// Signal that the stream has ended.
    pub fn end_of_stream(&self, _error: Option<EndOfStreamError>) {
        *self.ready_state.lock().unwrap() = ReadyState::Ended;
        if let Some(cb) = self.on_source_ended.lock().unwrap().as_ref() {
            cb();
        }
    }

    /// Set the duration of the media.
    pub fn set_duration(&self, seconds: f64) {
        *self.duration.lock().unwrap() = seconds;
    }

    /// Open the MediaSource (called when attached to a media element).
    pub fn open(&self) {
        *self.ready_state.lock().unwrap() = ReadyState::Open;
        if let Some(cb) = self.on_source_open.lock().unwrap().as_ref() {
            cb();
        }
    }

    /// Register a sourceopen callback.
    pub fn on_source_open<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_source_open.lock().unwrap() = Some(Box::new(callback));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndOfStreamError {
    Network,
    Decode,
}

/// A SourceBuffer — holds media data for one track.
pub struct SourceBuffer {
    /// The codec this buffer holds.
    pub codec: Codec,
    /// Buffered time ranges: [(start_seconds, end_seconds), ...]
    pub buffered: Mutex<Vec<(f64, f64)>>,
    /// Pending append operations.
    pub append_queue: Mutex<VecDeque<Vec<u8>>>,
    /// Whether an append is currently in progress.
    pub updating: Mutex<bool>,
    /// Timestamp offset for appended data (for splicing).
    pub timestamp_offset: Mutex<f64>,
    /// Append window (only data within this range is kept).
    pub append_window_start: Mutex<f64>,
    pub append_window_end: Mutex<f64>,
    /// Callbacks.
    on_update_start: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_update: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_update_end: Mutex<Option<Box<dyn Fn() + Send>>>,
    on_error: Mutex<Option<Box<dyn Fn(String) + Send>>>,
}

impl std::fmt::Debug for SourceBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceBuffer")
            .field("codec", &self.codec)
            .field("buffered", &*self.buffered.lock().unwrap())
            .field("updating", &*self.updating.lock().unwrap())
            .finish()
    }
}

impl SourceBuffer {
    pub fn new(codec: Codec) -> Arc<Self> {
        Arc::new(Self {
            codec,
            buffered: Mutex::new(Vec::new()),
            append_queue: Mutex::new(VecDeque::new()),
            updating: Mutex::new(false),
            timestamp_offset: Mutex::new(0.0),
            append_window_start: Mutex::new(0.0),
            append_window_end: Mutex::new(f64::INFINITY),
            on_update_start: Mutex::new(None),
            on_update: Mutex::new(None),
            on_update_end: Mutex::new(None),
            on_error: Mutex::new(None),
        })
    }

    /// Append media data (a chunk of encoded audio/video).
    ///
    /// The data is a raw byte array — typically a fragmented MP4 chunk
    /// or WebM cluster fetched via fetch() or XHR.
    pub fn append_buffer(&self, data: Vec<u8>) -> Result<(), String> {
        if *self.updating.lock().unwrap() {
            return Err("SourceBuffer is already updating".to_string());
        }
        *self.updating.lock().unwrap() = true;

        // Fire updatestart.
        if let Some(cb) = self.on_update_start.lock().unwrap().as_ref() {
            cb();
        }

        // In a real impl, we'd decode the chunk here and add it to the
        // buffered time ranges. For now, we just estimate based on data size.
        // A VP9 chunk at 2 Mbps is ~250 KB per second.
        let estimated_duration = (data.len() as f64) / 250_000.0;
        let mut buffered = self.buffered.lock().unwrap();
        let start = buffered.last().map(|(_, e)| *e).unwrap_or(0.0);
        let end = start + estimated_duration;
        buffered.push((start, end));
        drop(buffered);

        *self.updating.lock().unwrap() = false;

        // Fire update then updateend.
        if let Some(cb) = self.on_update.lock().unwrap().as_ref() {
            cb();
        }
        if let Some(cb) = self.on_update_end.lock().unwrap().as_ref() {
            cb();
        }
        Ok(())
    }

    /// Remove a time range from the buffer.
    pub fn remove(&self, start: f64, end: f64) -> Result<(), String> {
        let mut buffered = self.buffered.lock().unwrap();
        buffered.retain(|(s, e)| *e <= start || *s >= end);
        Ok(())
    }

    /// Abort the current append operation.
    pub fn abort(&self) {
        *self.updating.lock().unwrap() = false;
        self.append_queue.lock().unwrap().clear();
    }

    /// Set the onupdateend callback.
    pub fn on_update_end<F: Fn() + Send + 'static>(&self, callback: F) {
        *self.on_update_end.lock().unwrap() = Some(Box::new(callback));
    }
}

/// Supported codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    /// VP9 video (open, royalty-free).
    Vp9,
    /// VP8 video (older, open).
    Vp8,
    /// AV1 video (newest open codec).
    Av1,
    /// H.264 video (patent-encumbered; requires MPEG-LA license).
    H264,
    /// H.265/HEVC video (patent-encumbered).
    H265,
    /// Opus audio (open).
    Opus,
    /// Vorbis audio (open).
    Vorbis,
    /// AAC audio (patent-encumbered).
    Aac,
    /// MP3 audio.
    Mp3,
    /// FLAC audio (lossless, open).
    Flac,
}

/// Parse a MIME type string like `video/webm; codecs="vp9,opus"` into a Codec.
fn parse_mime_type(mime: &str) -> Result<Codec, String> {
    let lower = mime.to_lowercase();
    if lower.contains("vp9") {
        return Ok(Codec::Vp9);
    }
    if lower.contains("vp8") {
        return Ok(Codec::Vp8);
    }
    if lower.contains("av1") {
        return Ok(Codec::Av1);
    }
    if lower.contains("h264") || lower.contains("avc") {
        return Ok(Codec::H264);
    }
    if lower.contains("h265") || lower.contains("hevc") {
        return Ok(Codec::H265);
    }
    if lower.contains("opus") {
        return Ok(Codec::Opus);
    }
    if lower.contains("vorbis") {
        return Ok(Codec::Vorbis);
    }
    if lower.contains("aac") {
        return Ok(Codec::Aac);
    }
    if lower.contains("mp3") {
        return Ok(Codec::Mp3);
    }
    if lower.contains("flac") {
        return Ok(Codec::Flac);
    }
    Err(format!("Unsupported MIME type: {}", mime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_source_lifecycle() {
        let ms = MediaSource::new();
        assert_eq!(*ms.ready_state.lock().unwrap(), ReadyState::Closed);

        let opened = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let o = opened.clone();
        ms.on_source_open(move || {
            o.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        ms.open();
        assert!(opened.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(*ms.ready_state.lock().unwrap(), ReadyState::Open);
    }

    #[test]
    fn add_source_buffer() {
        let ms = MediaSource::new();
        let sb = ms.add_source_buffer("video/webm; codecs=\"vp9\"").unwrap();
        assert_eq!(sb.codec, Codec::Vp9);
        assert_eq!(ms.source_buffers.lock().unwrap().len(), 1);
    }

    #[test]
    fn append_buffer_estimates_duration() {
        let ms = MediaSource::new();
        let sb = ms.add_source_buffer("video/webm; codecs=\"vp9\"").unwrap();

        let updated = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let u = updated.clone();
        sb.on_update_end(move || {
            u.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        // Append 250KB → should add ~1 second to buffered.
        sb.append_buffer(vec![0u8; 250_000]).unwrap();
        assert!(updated.load(std::sync::atomic::Ordering::SeqCst));

        let buffered = sb.buffered.lock().unwrap();
        assert_eq!(buffered.len(), 1);
        assert!((buffered[0].1 - buffered[0].0 - 1.0).abs() < 0.1);
    }

    #[test]
    fn remove_time_range() {
        let ms = MediaSource::new();
        let sb = ms.add_source_buffer("video/webm; codecs=\"vp9\"").unwrap();
        sb.append_buffer(vec![0u8; 250_000]).unwrap(); // 0..1
        sb.append_buffer(vec![0u8; 250_000]).unwrap(); // 1..2
        sb.append_buffer(vec![0u8; 250_000]).unwrap(); // 2..3

        // Remove range 1..2.
        sb.remove(1.0, 2.0).unwrap();
        let buffered = sb.buffered.lock().unwrap();
        // Should keep 0..1 and 2..3.
        assert_eq!(buffered.len(), 2);
    }

    #[test]
    fn parse_mime_types() {
        assert_eq!(
            parse_mime_type("video/webm; codecs=\"vp9,opus\"").unwrap(),
            Codec::Vp9
        );
        assert_eq!(
            parse_mime_type("video/mp4; codecs=\"avc1.42E01E\"").unwrap(),
            Codec::H264
        );
        assert_eq!(
            parse_mime_type("audio/webm; codecs=\"opus\"").unwrap(),
            Codec::Opus
        );
        assert!(parse_mime_type("application/octet-stream").is_err());
    }

    #[test]
    fn end_of_stream() {
        let ms = MediaSource::new();
        ms.open();
        ms.end_of_stream(None);
        assert_eq!(*ms.ready_state.lock().unwrap(), ReadyState::Ended);
    }
}
