use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{Receiver, Sender};

use super::stretcher::Stretcher;
use super::types::{AudioCommand, AudioData, AudioEvent};

/// Size of chunks fed into SoundTouch at a time.
const CHUNK_SIZE: usize = 1024;
/// How often (in output frames) to send position updates.
const POSITION_UPDATE_INTERVAL: usize = 2048;

#[allow(dead_code)]
struct EngineState {
    audio: Option<Arc<AudioData>>,
    position: usize, // current frame position
    playing: bool,
    tempo: f32,
    loop_region: Option<(usize, usize)>, // frame range
    stretcher: Option<Stretcher>,
    output_sample_rate: u32,
    frames_since_update: usize,
}

impl EngineState {
    fn new(output_sample_rate: u32) -> Self {
        Self {
            audio: None,
            position: 0,
            playing: false,
            tempo: 1.0,
            loop_region: None,
            stretcher: None,
            output_sample_rate,
            frames_since_update: 0,
        }
    }

    fn handle_command(&mut self, cmd: AudioCommand, event_tx: &Sender<AudioEvent>) {
        match cmd {
            AudioCommand::LoadAudio(data) => {
                let sr = data.sample_rate;
                let ch = data.channels;
                self.audio = Some(data);
                self.position = 0;
                self.playing = false;
                self.loop_region = None;
                let mut stretcher = Stretcher::new(sr, ch);
                stretcher.set_tempo(self.tempo);
                self.stretcher = Some(stretcher);
            }
            AudioCommand::Play => {
                if self.audio.is_some() {
                    self.playing = true;
                }
            }
            AudioCommand::Pause => {
                self.playing = false;
            }
            AudioCommand::Stop => {
                self.playing = false;
                self.position = 0;
                if let Some(s) = &mut self.stretcher {
                    s.clear();
                }
                let _ = event_tx.send(AudioEvent::PositionChanged(0.0));
            }
            AudioCommand::Seek(time) => {
                if let Some(audio) = &self.audio {
                    let frame = (time * audio.sample_rate as f64) as usize;
                    self.position = frame.min(audio.num_frames());
                    if let Some(s) = &mut self.stretcher {
                        s.clear();
                    }
                    let pos_secs = self.position as f64 / audio.sample_rate as f64;
                    let _ = event_tx.send(AudioEvent::PositionChanged(pos_secs));
                }
            }
            AudioCommand::SetTempo(tempo) => {
                self.tempo = tempo;
                if let Some(s) = &mut self.stretcher {
                    s.set_tempo(tempo);
                }
            }
            AudioCommand::SetLoopRegion(region) => {
                if let Some(audio) = &self.audio {
                    self.loop_region = region.map(|(start, end)| {
                        let sr = audio.sample_rate as f64;
                        let start_frame = (start * sr) as usize;
                        let end_frame = (end * sr) as usize;
                        (start_frame, end_frame.min(audio.num_frames()))
                    });
                }
            }
            AudioCommand::Shutdown => {}
        }
    }

    /// Fill the output buffer with processed audio.
    fn fill_buffer(&mut self, output: &mut [f32], channels: u16, event_tx: &Sender<AudioEvent>) {
        if !self.playing {
            output.fill(0.0);
            return;
        }

        let audio = match &self.audio {
            Some(a) => a.clone(),
            None => {
                output.fill(0.0);
                return;
            }
        };

        let stretcher = match &mut self.stretcher {
            Some(s) => s,
            None => {
                output.fill(0.0);
                return;
            }
        };

        let audio_channels = audio.channels as usize;
        let out_channels = channels as usize;
        let total_frames = audio.num_frames();
        let mut out_pos = 0;
        let out_frames = output.len() / out_channels;

        // Temporary buffer for receiving from SoundTouch
        let mut recv_buf = vec![0.0f32; out_frames * audio_channels];

        while out_pos < out_frames {
            // Try to receive from SoundTouch first
            let needed = out_frames - out_pos;
            let recv_slice = &mut recv_buf[..needed * audio_channels];
            let got_samples = stretcher.receive_samples(recv_slice);
            let got_frames = got_samples / audio_channels;

            if got_frames > 0 {
                // Write received frames to output, handling channel conversion
                for f in 0..got_frames {
                    for c in 0..out_channels {
                        let src_c = c % audio_channels;
                        output[(out_pos + f) * out_channels + c] =
                            recv_slice[f * audio_channels + src_c];
                    }
                }
                out_pos += got_frames;
                self.frames_since_update += got_frames;

                if self.frames_since_update >= POSITION_UPDATE_INTERVAL {
                    self.frames_since_update = 0;
                    let pos_secs = self.position as f64 / audio.sample_rate as f64;
                    let _ = event_tx.send(AudioEvent::PositionChanged(pos_secs));
                }
                continue;
            }

            // Need to feed more samples to SoundTouch
            if self.position >= total_frames {
                // Check for loop
                if let Some((start, _)) = self.loop_region {
                    self.position = start;
                    stretcher.clear();
                    continue;
                } else {
                    // Playback finished
                    self.playing = false;
                    let _ = event_tx.send(AudioEvent::PlaybackFinished);
                    // Fill rest with silence
                    for i in out_pos * out_channels..output.len() {
                        output[i] = 0.0;
                    }
                    return;
                }
            }

            // Determine how many frames to feed
            let mut feed_frames = CHUNK_SIZE.min(total_frames - self.position);

            // Respect loop end boundary
            if let Some((start, end)) = self.loop_region {
                if self.position >= end {
                    self.position = start;
                    stretcher.clear();
                    continue;
                }
                feed_frames = feed_frames.min(end - self.position);
            }

            let start_sample = self.position * audio_channels;
            let end_sample = start_sample + feed_frames * audio_channels;
            stretcher.put_samples(&audio.samples[start_sample..end_sample]);
            self.position += feed_frames;
        }
    }
}

/// Spawn the audio engine thread and return command/event channels.
pub fn spawn_engine() -> Result<(Sender<AudioCommand>, Receiver<AudioEvent>), String> {
    let (cmd_tx, cmd_rx) = crossbeam_channel::bounded::<AudioCommand>(64);
    let (event_tx, event_rx) = crossbeam_channel::bounded::<AudioEvent>(256);

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No audio output device found")?;

    let config = device
        .default_output_config()
        .map_err(|e| format!("Failed to get output config: {e}"))?;

    let sample_rate = config.sample_rate();
    let channels = config.channels();
    let sample_format = config.sample_format();

    let mut state = EngineState::new(sample_rate);
    let event_tx_clone = event_tx.clone();

    let stream = match sample_format {
        cpal::SampleFormat::F32 => device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Process commands
                    while let Ok(cmd) = cmd_rx.try_recv() {
                        state.handle_command(cmd, &event_tx_clone);
                    }
                    state.fill_buffer(data, channels, &event_tx_clone);
                },
                |err| {
                    eprintln!("Audio stream error: {err}");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?,
        _ => return Err(format!("Unsupported sample format: {sample_format:?}")),
    };

    stream
        .play()
        .map_err(|e| format!("Failed to start stream: {e}"))?;

    // Keep stream alive by moving it into a thread
    std::thread::Builder::new()
        .name("audio-keepalive".into())
        .spawn(move || {
            let _stream = stream;
            loop {
                std::thread::park();
            }
        })
        .map_err(|e| format!("Failed to spawn keepalive thread: {e}"))?;

    Ok((cmd_tx, event_rx))
}
