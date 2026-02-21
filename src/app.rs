use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender};
use iced::keyboard;
use iced::widget::{canvas, center, column, container, text};
use iced::{Element, Length, Subscription, Task, Theme};

use crate::audio::decoder;
use crate::audio::engine;
use crate::audio::types::*;
use crate::ui::controls::{self, ControlMessage};
use crate::ui::waveform::{WaveformMessage, WaveformView};
use crate::waveform_cache::WaveformPeaks;

pub struct App {
    // Audio engine channels
    cmd_tx: Option<Sender<AudioCommand>>,
    event_rx: Option<Receiver<AudioEvent>>,

    // State
    status: PlaybackStatus,
    position: f64,
    duration: f64,
    tempo: f32,
    loop_region: Option<(f64, f64)>,
    filename: Option<String>,
    error: Option<String>,

    // Waveform
    waveform_view: WaveformView,
    audio_data: Option<Arc<AudioData>>,

    // Drag state for loop selection
    drag_start: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum Message {
    EngineReady(Result<(Sender<AudioCommand>, Receiver<AudioEvent>), String>),
    FileLoaded(Result<(AudioData, String), String>),
    Control(ControlMessage),
    Waveform(WaveformMessage),
    Tick,
    KeyEvent(keyboard::Event),
    FileDialogResult(Option<std::path::PathBuf>),
}

fn boot() -> (App, Task<Message>) {
    let app = App {
        cmd_tx: None,
        event_rx: None,
        status: PlaybackStatus::Stopped,
        position: 0.0,
        duration: 0.0,
        tempo: 1.0,
        loop_region: None,
        filename: None,
        error: None,
        waveform_view: WaveformView::new(),
        audio_data: None,
        drag_start: None,
    };

    let task = Task::perform(
        async {
            tokio::task::spawn_blocking(engine::spawn_engine)
                .await
                .unwrap()
        },
        Message::EngineReady,
    );

    (app, task)
}

fn title(app: &App) -> String {
    match &app.filename {
        Some(name) => format!("Transcribe - {name}"),
        None => "Transcribe".to_string(),
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::EngineReady(result) => match result {
            Ok((tx, rx)) => {
                app.cmd_tx = Some(tx);
                app.event_rx = Some(rx);
                Task::none()
            }
            Err(e) => {
                app.error = Some(format!("Audio engine error: {e}"));
                Task::none()
            }
        },
        Message::Control(ctrl) => match ctrl {
            ControlMessage::OpenFile => Task::perform(
                async {
                    let handle = rfd::AsyncFileDialog::new()
                        .add_filter("Audio", &["mp3", "wav", "flac", "ogg", "aac"])
                        .pick_file()
                        .await;
                    handle.map(|h| h.path().to_path_buf())
                },
                Message::FileDialogResult,
            ),
            ControlMessage::PlayPause => {
                if let Some(tx) = &app.cmd_tx {
                    match app.status {
                        PlaybackStatus::Playing => {
                            let _ = tx.send(AudioCommand::Pause);
                            app.status = PlaybackStatus::Paused;
                        }
                        _ => {
                            let _ = tx.send(AudioCommand::Play);
                            app.status = PlaybackStatus::Playing;
                        }
                    }
                }
                Task::none()
            }
            ControlMessage::Stop => {
                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::Stop);
                    app.status = PlaybackStatus::Stopped;
                    app.position = 0.0;
                    app.waveform_view.playback_position = 0.0;
                }
                Task::none()
            }
            ControlMessage::TempoChanged(t) => {
                app.tempo = t;
                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::SetTempo(t));
                }
                Task::none()
            }
            ControlMessage::ClearLoop => {
                app.loop_region = None;
                app.waveform_view.loop_region = None;
                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::SetLoopRegion(None));
                }
                Task::none()
            }
        },
        Message::FileDialogResult(path) => {
            if let Some(path) = path {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            decoder::decode_file(&path).map(|data| (data, filename))
                        })
                        .await
                        .unwrap()
                    },
                    Message::FileLoaded,
                )
            } else {
                Task::none()
            }
        }
        Message::FileLoaded(result) => match result {
            Ok((data, filename)) => {
                let peaks = WaveformPeaks::compute(&data);
                let total_frames = data.num_frames();
                let duration = data.duration;

                app.waveform_view.set_peaks(peaks, total_frames, duration);
                app.duration = duration;
                app.filename = Some(filename);
                app.position = 0.0;
                app.loop_region = None;
                app.waveform_view.loop_region = None;
                app.waveform_view.playback_position = 0.0;
                app.status = PlaybackStatus::Stopped;
                app.error = None;

                let arc_data = Arc::new(data);
                app.audio_data = Some(arc_data.clone());

                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::LoadAudio(arc_data));
                }

                Task::none()
            }
            Err(e) => {
                app.error = Some(e);
                Task::none()
            }
        },
        Message::Waveform(wm) => match wm {
            WaveformMessage::Seek(time) => {
                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::Seek(time));
                    app.position = time;
                    if app.duration > 0.0 {
                        app.waveform_view.playback_position = time / app.duration;
                    }
                }
                Task::none()
            }
            WaveformMessage::LoopSelected(start, end) => {
                app.loop_region = Some((start, end));
                if app.duration > 0.0 {
                    app.waveform_view.loop_region =
                        Some((start / app.duration, end / app.duration));
                }
                if let Some(tx) = &app.cmd_tx {
                    let _ = tx.send(AudioCommand::SetLoopRegion(Some((start, end))));
                }
                Task::none()
            }
            WaveformMessage::DragStarted(frac) => {
                app.drag_start = Some(frac);
                Task::none()
            }
            WaveformMessage::DragMoved(frac) => {
                if let Some(start) = app.drag_start {
                    let (lo, hi) = if start < frac {
                        (start, frac)
                    } else {
                        (frac, start)
                    };
                    if (hi - lo).abs() > 0.005 {
                        app.waveform_view.loop_region = Some((lo, hi));
                    }
                }
                Task::none()
            }
        },
        Message::Tick => {
            if let Some(rx) = &app.event_rx {
                while let Ok(event) = rx.try_recv() {
                    match event {
                        AudioEvent::PositionChanged(pos) => {
                            app.position = pos;
                            if app.duration > 0.0 {
                                app.waveform_view.playback_position = pos / app.duration;
                            }
                        }
                        AudioEvent::PlaybackFinished => {
                            app.status = PlaybackStatus::Stopped;
                            app.position = 0.0;
                            app.waveform_view.playback_position = 0.0;
                        }
                        AudioEvent::Error(e) => {
                            app.error = Some(e);
                        }
                    }
                }
            }
            Task::none()
        }
        Message::KeyEvent(key_event) => match key_event {
            keyboard::Event::KeyPressed {
                key, modifiers: _, ..
            } => match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Space) => {
                    update(app, Message::Control(ControlMessage::PlayPause))
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                    let new_pos = (app.position - 5.0).max(0.0);
                    if let Some(tx) = &app.cmd_tx {
                        let _ = tx.send(AudioCommand::Seek(new_pos));
                    }
                    Task::none()
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                    let new_pos = (app.position + 5.0).min(app.duration);
                    if let Some(tx) = &app.cmd_tx {
                        let _ = tx.send(AudioCommand::Seek(new_pos));
                    }
                    Task::none()
                }
                _ => Task::none(),
            },
            _ => Task::none(),
        },
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let controls = controls::view_controls(
        app.status,
        app.position,
        app.duration,
        app.tempo,
        app.loop_region.is_some(),
    )
    .map(Message::Control);

    let waveform: Element<Message> = if app.audio_data.is_some() {
        let canvas_el: Element<WaveformMessage> = canvas::Canvas::new(&app.waveform_view)
            .width(Length::Fill)
            .height(Length::Fixed(200.0))
            .into();
        canvas_el.map(Message::Waveform)
    } else {
        center(text("Open an audio file to begin").size(18))
            .width(Length::Fill)
            .height(Length::Fixed(200.0))
            .into()
    };

    let mut content = column![controls, waveform].spacing(5);

    if let Some(err) = &app.error {
        content = content.push(
            container(text(format!("Error: {err}")).color(iced::Color::from_rgb(1.0, 0.3, 0.3)))
                .padding(10),
        );
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn subscription(_app: &App) -> Subscription<Message> {
    let tick =
        iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick);

    let keys = keyboard::listen().map(Message::KeyEvent);

    Subscription::batch([tick, keys])
}

fn theme(_app: &App) -> Theme {
    Theme::Dark
}

pub fn run() -> iced::Result {
    iced::application(boot, update, view)
        .title(title)
        .subscription(subscription)
        .theme(theme)
        .window_size((1000.0, 400.0))
        .run()
}
