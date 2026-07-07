use std::sync::Arc;
use arc_swap::ArcSwap;

use crate::analysis::N_NOTES;

#[derive(Clone)]
pub struct AudioFrame {
    pub note_energy: [f32; N_NOTES],
    pub note_flux: [f32; N_NOTES],
}

pub type SharedFrame = Arc<ArcSwap<AudioFrame>>;

pub fn new_shared_frame() -> SharedFrame {
    Arc::new(ArcSwap::from_pointee(AudioFrame::default()))
}

impl Default for AudioFrame {
    fn default() -> Self {
        AudioFrame {
            note_energy: [0.0; N_NOTES],
            note_flux: [0.0; N_NOTES],
        }
    }
}
