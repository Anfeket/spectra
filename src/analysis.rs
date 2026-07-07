use std::f32::consts::PI;

use crate::frame::{AudioFrame, SharedFrame};
use rtrb::Consumer;
use rustfft::{FftPlanner, num_complex::Complex};

const PITCH_SIZE: usize = 1024 * 14;
const BEAT_SIZE: usize = 1024;
const HOP: usize = 512;

pub const NOTE_MIN: u8 = 33; // A1
pub const NOTE_MAX: u8 = 120; // C8
pub const N_NOTES: usize = (NOTE_MAX - NOTE_MIN) as usize;

pub fn note_to_freq(note: u8) -> f32 {
    440.0 * 2.0f32.powf((note as f32 - 69.0) / 12.0)
}

pub fn run_analysis(mut consumer: Consumer<f32>, shared: SharedFrame, sample_rate: f32) {
    let mut pitch_window = [0.0f32; PITCH_SIZE];
    let mut beat_window = [0.0f32; BEAT_SIZE];
    let mut prev_note_energy = [0.0f32; N_NOTES];

    let mut planner = FftPlanner::new();
    let _beat_fft = planner.plan_fft_forward(BEAT_SIZE);
    let pitch_fft = planner.plan_fft_forward(PITCH_SIZE);

    // Pre-calculate the CQT kernels once before entering the real-time loop
    let cqt_kernels = build_cqt_kernel(sample_rate, PITCH_SIZE);

    let mut hop_buf = [0.0f32; HOP];

    loop {
        let mut filled = 0;
        while filled < HOP {
            if let Ok(s) = consumer.pop() {
                hop_buf[filled] = s;
                filled += 1;
            } else {
                std::thread::park_timeout(std::time::Duration::from_millis(5));
            }
        }

        // Slide both windows
        beat_window.copy_within(HOP.., 0);
        beat_window[BEAT_SIZE - HOP..].copy_from_slice(&hop_buf);

        pitch_window.copy_within(HOP.., 0);
        pitch_window[PITCH_SIZE - HOP..].copy_from_slice(&hop_buf);

        // Compute CQT energy directly across note bins
        let mut fft_buf = vec![Complex::new(0.0, 0.0); PITCH_SIZE];
        let current_note_energy = compute_cqt(&pitch_fft, &pitch_window, &cqt_kernels, &mut fft_buf);

        let mut note_energy = [0.0f32; N_NOTES];
        let mut note_flux = [0.0f32; N_NOTES];

        for i in 0..N_NOTES {
            note_energy[i] = current_note_energy[i];
            note_flux[i] = (current_note_energy[i] - prev_note_energy[i]).max(0.0);
        }
        prev_note_energy = current_note_energy;

        let new_frame = AudioFrame {
            note_energy,
            note_flux,
        };

        shared.store(std::sync::Arc::new(new_frame));
    }
}

/// Computes the Constant-Q Transform by projecting the FFT of the input signal
/// onto the pre-computed sparse spectral kernels.
pub fn compute_cqt(
    fft: &std::sync::Arc<dyn rustfft::Fft<f32>>,
    window: &[f32],
    kernels: &[CqtKernel],
    fft_buf: &mut [Complex<f32>]
) -> [f32; N_NOTES] {
    for (slot, &sample) in fft_buf.iter_mut().zip(window.iter()) {
        *slot = Complex::new(sample, 0.0);
    }

    fft.process(fft_buf);

    let mut notes = [0.0f32; N_NOTES];
    for i in 0..N_NOTES {
        let mut sum = Complex::new(0.0, 0.0);
        for &(bin, kernel_val_conj) in &kernels[i].bins {
            if bin < fft_buf.len() {
                sum += fft_buf[bin] * kernel_val_conj;
            }
        }
        notes[i] = sum.norm();
    }
    notes
}

pub struct CqtKernel {
    pub bins: Vec<(usize, Complex<f32>)>,
    pub fft_size: usize,
}

fn build_cqt_kernel(sample_rate: f32, fft_size: usize) -> Vec<CqtKernel> {
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_size);
    let q = 1.0 / (2f32.powf(1.0 / 12.0) - 1.0);

    (0..N_NOTES)
        .map(|i| {
            let freq = note_to_freq(NOTE_MIN + i as u8);
            let kernel_len = (q * sample_rate / freq).ceil() as usize;
            let mut time_kernel = vec![Complex::new(0.0, 0.0); fft_size];

            let start_idx = fft_size - kernel_len.min(fft_size);

            for n in 0..kernel_len.min(fft_size) {
                let hann = 0.5 - 0.5 * (2.0 * PI * n as f32 / (kernel_len as f32 - 1.0)).cos();
                let phase = -2.0 * PI * freq * n as f32 / sample_rate;

                time_kernel[start_idx + n] = Complex::from_polar(hann / kernel_len as f32, phase);
            }

            fft.process(&mut time_kernel);

            let bins = time_kernel
                .iter()
                .enumerate()
                .filter(|(_, c)| c.norm() > 1e-4)
                .map(|(b, c)| (b, c.conj()))
                .collect();

            CqtKernel { bins, fft_size }
        })
        .collect()
}
