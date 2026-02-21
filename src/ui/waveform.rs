use iced::mouse;
use iced::widget::canvas::{self, Action, Cache, Event, Frame, Geometry, Path, Stroke};
use iced::{Color, Rectangle, Renderer, Theme};

use crate::waveform_cache::WaveformPeaks;

/// State for the waveform canvas widget.
pub struct WaveformView {
    waveform_cache: Cache,
    pub peaks: Option<WaveformPeaks>,
    pub total_frames: usize,
    pub playback_position: f64, // 0.0 to 1.0 fraction
    pub loop_region: Option<(f64, f64)>, // fractions
    pub duration: f64,
}

/// Interactions on the waveform.
#[derive(Debug, Clone)]
pub enum WaveformMessage {
    Seek(f64),              // time in seconds
    LoopSelected(f64, f64), // start, end in seconds
    DragStarted(f64),       // x fraction
    DragMoved(f64),         // x fraction
}

#[allow(dead_code)]
impl WaveformView {
    pub fn new() -> Self {
        Self {
            waveform_cache: Cache::new(),
            peaks: None,
            total_frames: 0,
            playback_position: 0.0,
            loop_region: None,
            duration: 0.0,
        }
    }

    pub fn set_peaks(&mut self, peaks: WaveformPeaks, total_frames: usize, duration: f64) {
        self.peaks = Some(peaks);
        self.total_frames = total_frames;
        self.duration = duration;
        self.waveform_cache.clear();
    }

    pub fn clear_cache(&mut self) {
        self.waveform_cache.clear();
    }
}

impl canvas::Program<WaveformMessage> for WaveformView {
    type State = Option<f64>; // drag start x fraction

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let width = bounds.width;
        let height = bounds.height;

        // Layer 1: Cached waveform
        let waveform = self.waveform_cache.draw(renderer, bounds.size(), |frame| {
            // Background
            frame.fill_rectangle(
                iced::Point::ORIGIN,
                bounds.size(),
                Color::from_rgb(0.12, 0.12, 0.15),
            );

            // Center line
            let center_y = height / 2.0;
            let center_line = Path::line(
                iced::Point::new(0.0, center_y),
                iced::Point::new(width, center_y),
            );
            frame.stroke(
                &center_line,
                Stroke::default()
                    .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.15))
                    .with_width(1.0),
            );

            if let Some(peaks) = &self.peaks {
                let display_peaks = peaks.peaks_for_width(width, self.total_frames);
                let waveform_color = Color::from_rgb(0.3, 0.7, 1.0);

                for (i, peak) in display_peaks.iter().enumerate() {
                    let x = i as f32;
                    let min_y = center_y - peak.max * center_y;
                    let max_y = center_y - peak.min * center_y;

                    let line = Path::line(
                        iced::Point::new(x, min_y),
                        iced::Point::new(x, max_y),
                    );
                    frame.stroke(
                        &line,
                        Stroke::default()
                            .with_color(waveform_color)
                            .with_width(1.0),
                    );
                }
            }
        });

        // Layer 2: Dynamic overlay (playhead + loop region)
        let overlay = {
            let mut frame = Frame::new(renderer, bounds.size());

            // Draw loop region
            if let Some((start, end)) = self.loop_region {
                let x_start = (start * width as f64) as f32;
                let x_end = (end * width as f64) as f32;
                let loop_width = x_end - x_start;

                frame.fill_rectangle(
                    iced::Point::new(x_start, 0.0),
                    iced::Size::new(loop_width, height),
                    Color::from_rgba(1.0, 0.8, 0.0, 0.15),
                );

                // Loop region borders
                for &x in &[x_start, x_end] {
                    let line = Path::line(
                        iced::Point::new(x, 0.0),
                        iced::Point::new(x, height),
                    );
                    frame.stroke(
                        &line,
                        Stroke::default()
                            .with_color(Color::from_rgba(1.0, 0.8, 0.0, 0.7))
                            .with_width(1.0),
                    );
                }
            }

            // Draw playhead
            let playhead_x = (self.playback_position * width as f64) as f32;
            let playhead = Path::line(
                iced::Point::new(playhead_x, 0.0),
                iced::Point::new(playhead_x, height),
            );
            frame.stroke(
                &playhead,
                Stroke::default()
                    .with_color(Color::from_rgb(1.0, 1.0, 1.0))
                    .with_width(2.0),
            );

            frame.into_geometry()
        };

        vec![waveform, overlay]
    }

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<WaveformMessage>> {
        let cursor_pos = cursor.position_in(bounds)?;

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let frac = (cursor_pos.x / bounds.width) as f64;
                let frac = frac.clamp(0.0, 1.0);
                *state = Some(frac);
                Some(Action::publish(WaveformMessage::DragStarted(frac)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_some() {
                    let frac = (cursor_pos.x / bounds.width) as f64;
                    let frac = frac.clamp(0.0, 1.0);
                    Some(Action::publish(WaveformMessage::DragMoved(frac)).and_capture())
                } else {
                    None
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(start) = state.take() {
                    let end = (cursor_pos.x / bounds.width) as f64;
                    let end = end.clamp(0.0, 1.0);
                    let diff = (end - start).abs();

                    if diff < 0.005 {
                        // Click: seek
                        let time = start * self.duration;
                        Some(Action::publish(WaveformMessage::Seek(time)).and_capture())
                    } else {
                        // Drag: loop selection
                        let (lo, hi) = if start < end {
                            (start, end)
                        } else {
                            (end, start)
                        };
                        let t_start = lo * self.duration;
                        let t_end = hi * self.duration;
                        Some(
                            Action::publish(WaveformMessage::LoopSelected(t_start, t_end))
                                .and_capture(),
                        )
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
