use soundtouch::SoundTouch;

/// Wrapper around SoundTouch for tempo-changing without pitch shift.
pub struct Stretcher {
    st: SoundTouch,
    channels: u16,
}

#[allow(dead_code)]
impl Stretcher {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let mut st = SoundTouch::new();
        st.set_sample_rate(sample_rate);
        st.set_channels(channels as u32);
        st.set_tempo(1.0);
        Stretcher { st, channels }
    }

    pub fn set_tempo(&mut self, tempo: f32) {
        self.st.set_tempo(tempo as f64);
    }

    /// Feed interleaved input samples into SoundTouch.
    pub fn put_samples(&mut self, samples: &[f32]) {
        self.st
            .put_samples(samples, samples.len() / self.channels as usize);
    }

    /// Receive processed samples from SoundTouch.
    /// Returns the number of samples written (total, not per channel).
    pub fn receive_samples(&mut self, output: &mut [f32]) -> usize {
        let max_frames = output.len() / self.channels as usize;
        let received_frames = self.st.receive_samples(output, max_frames);
        received_frames * self.channels as usize
    }

    /// Flush remaining samples through the processor.
    pub fn flush(&mut self) {
        self.st.flush();
    }

    /// Clear all buffered data (use when seeking or changing loop).
    pub fn clear(&mut self) {
        self.st.clear();
    }
}
