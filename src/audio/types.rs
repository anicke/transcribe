use std::sync::Arc;

/// Decoded audio data stored entirely in memory.
#[derive(Clone, Debug)]
pub struct AudioData {
    /// Interleaved samples normalized to [-1.0, 1.0].
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Duration in seconds.
    pub duration: f64,
}

impl AudioData {
    /// Total number of frames (samples per channel).
    pub fn num_frames(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    /// Mix down to mono, returning one sample per frame.
    pub fn to_mono(&self) -> Vec<f32> {
        let ch = self.channels as usize;
        if ch == 1 {
            return self.samples.clone();
        }
        self.samples
            .chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    }
}

/// Commands sent from the UI thread to the audio thread.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AudioCommand {
    LoadAudio(Arc<AudioData>),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetTempo(f32),
    SetLoopRegion(Option<(f64, f64)>),
    Shutdown,
}

/// Events sent from the audio thread to the UI thread.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AudioEvent {
    PositionChanged(f64),
    PlaybackFinished,
    Error(String),
}

/// Current playback status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}
