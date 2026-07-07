mod analysis;
mod audio;
mod frame;
mod render;

fn main() {
    let (device, config, sample_rate) = audio::get_default_config();

    let (producer, consumer) = rtrb::RingBuffer::<f32>::new(2048 * 4);
    let shared = frame::new_shared_frame();
    let shared_frame = shared.clone();

    let analysis_handle = std::thread::spawn(move || {
        analysis::run_analysis(consumer, shared_frame.clone(), sample_rate as f32);
    });

    let wake = analysis_handle.thread().clone();
    let _stream = audio::start_capture(device, config, producer, wake);

    render::run(shared);
}
