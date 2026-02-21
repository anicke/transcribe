use iced::widget::{button, container, row, slider, text, Column, Row};
use iced::{Alignment, Element, Length};

use crate::audio::types::PlaybackStatus;

#[derive(Debug, Clone)]
pub enum ControlMessage {
    PlayPause,
    Stop,
    TempoChanged(f32),
    ClearLoop,
    OpenFile,
}

/// Format seconds as MM:SS.
fn format_time(seconds: f64) -> String {
    let total_secs = seconds as u64;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}:{secs:02}")
}

/// Build the transport controls view.
pub fn view_controls<'a>(
    status: PlaybackStatus,
    position: f64,
    duration: f64,
    tempo: f32,
    has_loop: bool,
) -> Element<'a, ControlMessage> {
    let play_label = match status {
        PlaybackStatus::Playing => "Pause",
        _ => "Play",
    };

    let play_btn = button(text(play_label)).on_press(ControlMessage::PlayPause);
    let stop_btn = button(text("Stop")).on_press(ControlMessage::Stop);
    let open_btn = button(text("Open File")).on_press(ControlMessage::OpenFile);

    let time_display = text(format!(
        "{} / {}",
        format_time(position),
        format_time(duration)
    ))
    .size(16);

    let tempo_label = text(format!("Tempo: {:.0}%", tempo * 100.0)).size(14);
    let tempo_slider = slider(0.25..=2.0, tempo, ControlMessage::TempoChanged).step(0.05);

    let mut controls_row = Row::new()
        .spacing(10)
        .align_y(Alignment::Center)
        .push(open_btn)
        .push(play_btn)
        .push(stop_btn)
        .push(time_display);

    if has_loop {
        controls_row =
            controls_row.push(button(text("Clear Loop")).on_press(ControlMessage::ClearLoop));
    }

    let tempo_row = row![tempo_label, tempo_slider]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::Fixed(300.0));

    let full_row = Row::new()
        .spacing(20)
        .align_y(Alignment::Center)
        .push(controls_row)
        .push(tempo_row);

    container(Column::new().push(full_row))
        .padding(10)
        .into()
}
