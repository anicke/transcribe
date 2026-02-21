use crate::audio::types::AudioData;

/// A single peak entry: min and max sample values for a range of frames.
#[derive(Clone, Copy, Debug)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

/// Pre-computed peaks at multiple resolutions for efficient waveform rendering.
#[derive(Clone, Debug)]
pub struct WaveformPeaks {
    /// Each entry is (samples_per_peak, peaks).
    pub levels: Vec<(usize, Vec<Peak>)>,
}

/// Resolution levels: number of mono samples per peak.
const RESOLUTIONS: &[usize] = &[64, 256, 1024, 4096];

#[allow(dead_code)]
impl WaveformPeaks {
    /// Compute peaks from audio data at multiple resolutions.
    pub fn compute(audio: &AudioData) -> Self {
        let mono = audio.to_mono();
        let levels = RESOLUTIONS
            .iter()
            .map(|&spp| {
                let peaks = compute_peaks_at_resolution(&mono, spp);
                (spp, peaks)
            })
            .collect();
        WaveformPeaks { levels }
    }

    /// Get the best resolution level for the given canvas width and audio length.
    pub fn best_level(&self, canvas_width: f32, total_frames: usize) -> &[(usize, Vec<Peak>)] {
        // We want roughly 1-2 peaks per pixel
        let target_spp = total_frames as f32 / canvas_width;

        // Find the level with samples_per_peak closest to target
        // but not so fine that we'd have way too many peaks
        let _ = target_spp;
        &self.levels
    }

    /// Get peaks for rendering at the given width.
    /// Returns a Vec of peaks, one per pixel column.
    pub fn peaks_for_width(&self, canvas_width: f32, total_frames: usize) -> Vec<Peak> {
        if total_frames == 0 || canvas_width <= 0.0 {
            return Vec::new();
        }

        let target_spp = total_frames as f32 / canvas_width;

        // Find the best resolution level
        let (_, base_peaks) = self
            .levels
            .iter()
            .rev()
            .find(|(spp, _)| (*spp as f32) <= target_spp * 2.0)
            .unwrap_or(&self.levels[0]);

        // Resample peaks to exactly canvas_width entries
        let width = canvas_width as usize;
        let mut result = Vec::with_capacity(width);

        for i in 0..width {
            let frac_start = i as f64 / width as f64;
            let frac_end = (i + 1) as f64 / width as f64;
            let peak_start = (frac_start * base_peaks.len() as f64) as usize;
            let peak_end =
                ((frac_end * base_peaks.len() as f64) as usize).min(base_peaks.len());

            if peak_start >= base_peaks.len() {
                result.push(Peak { min: 0.0, max: 0.0 });
                continue;
            }

            let mut min = f32::MAX;
            let mut max = f32::MIN;
            for p in &base_peaks[peak_start..peak_end.max(peak_start + 1)] {
                min = min.min(p.min);
                max = max.max(p.max);
            }
            result.push(Peak { min, max });
        }

        result
    }
}

fn compute_peaks_at_resolution(mono: &[f32], samples_per_peak: usize) -> Vec<Peak> {
    mono.chunks(samples_per_peak)
        .map(|chunk| {
            let mut min = f32::MAX;
            let mut max = f32::MIN;
            for &s in chunk {
                if s < min {
                    min = s;
                }
                if s > max {
                    max = s;
                }
            }
            Peak { min, max }
        })
        .collect()
}
