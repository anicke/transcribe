use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::types::AudioData;

/// Decode an audio file into an `AudioData` struct with all samples in memory.
pub fn decode_file(path: &Path) -> Result<AudioData, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| format!("Failed to probe format: {e}"))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or("No default track found")?
        .clone();

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or("No sample rate in track")?;
    let channels = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create decoder: {e}"))?;

    let mut samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(e) => return Err(format!("Error reading packet: {e}")),
        };

        if packet.track_id() != track.id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(e)) => {
                eprintln!("Decode warning: {e}");
                continue;
            }
            Err(e) => return Err(format!("Decode error: {e}")),
        };

        let spec = *decoded.spec();
        let num_frames = decoded.capacity();

        let mut sample_buf = SampleBuffer::<f32>::new(num_frames as u64, spec);
        sample_buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(sample_buf.samples());
    }

    let num_frames = samples.len() / channels as usize;
    let duration = num_frames as f64 / sample_rate as f64;

    Ok(AudioData {
        samples,
        sample_rate,
        channels,
        duration,
    })
}
