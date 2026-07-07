use std::{thread::Thread, time::Duration};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::Producer;

pub fn get_default_config() -> (cpal::Device, cpal::StreamConfig, u32) {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("Failed to get default output device");
    let config: cpal::StreamConfig = device
        .default_output_config()
        .expect("Failed to get default output config")
        .into();

    let sample_rate = config.sample_rate;
    (device, config, sample_rate)
}

pub fn start_capture(
    device: cpal::Device,
    config: cpal::StreamConfig,
    mut producer: Producer<f32>,
    wake: Thread,
) -> cpal::Stream {
    let stream = device
        .build_input_stream(
            config.into(),
            move |data: &[f32], _| {
                for frame in data.chunks_exact(config.channels as usize) {
                    let mono = frame.iter().sum::<f32>() / frame.len() as f32;
                    let _ = producer.push(mono);
                }
                wake.unpark();
            },
            |err| eprintln!("Error in audio stream: {:?}", err),
            Some(Duration::from_secs(3)),
        )
        .expect("Failed to build input stream");

    stream.play().expect("Failed to start stream");
    stream
}
