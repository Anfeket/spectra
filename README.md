# spectra

A real-time audio visualizer that captures system audio, analyzes it with a
Constant-Q Transform (CQT), and renders a GPU-accelerated piano-roll of
per-note energy — 87 bars, one per semitone from A1 to C8.

Unlike a standard FFT-based visualizer, spectra bins energy directly onto
musical notes using logarithmically-spaced kernels, so the bars line up with
actual pitches rather than evenly-spaced linear frequency bins.

## How it works

```
cpal capture ──▶ rtrb ring buffer ──▶ analysis thread (CQT) ──▶ ArcSwap<AudioFrame> ──▶ wgpu render thread
```

- **`audio.rs`** — opens the default output device as a loopback/input
  stream via [cpal](https://github.com/RustAudio/cpal), downmixes to mono,
  and pushes samples into a lock-free [`rtrb`](https://github.com/mgeier/rtrb)
  ring buffer.
- **`analysis.rs`** — pulls samples off the ring buffer in fixed hops,
  slides them into a rolling analysis window, and computes a Constant-Q
  Transform: instead of a plain FFT, each note's energy is found by
  projecting the FFT output onto a pre-computed Hann-windowed complex kernel
  centered on that note's frequency. This gives proper logarithmic frequency
  resolution — narrow, precise bins for high notes and low notes alike —
  rather than the linear binning you'd get from FFT bins directly. Frame-to-
  frame energy deltas (flux) are computed here too, ready for onset/beat
  detection features later.
- **`frame.rs`** — defines `AudioFrame` (per-note energy + flux) and
  `SharedFrame`, an `Arc<ArcSwap<AudioFrame>>` used to hand data from the
  analysis thread to the render thread without locking.
- **`render.rs`** — a [`wgpu`](https://github.com/gfx-rs/wgpu) +
  [`winit`](https://github.com/rust-windowing/winit) app that reads the
  latest `AudioFrame` every frame, applies exponential decay smoothing so
  bars don't flicker, and draws one instanced quad per note with analytic
  (SDF-style) anti-aliased edges computed in the fragment shader
  (`bars.wgsl`) — no MSAA needed.

## Requirements

- Rust (edition 2024)
- A GPU with Vulkan, Metal, or DX12 support (via wgpu)
- On Linux, an audio setup that exposes a loopback/monitor source for the
  default output device (e.g. PulseAudio/PipeWire monitor), since `cpal` is
  told to open the **output** device as an input stream for system-audio
  capture

## Running

```bash
cargo run --release
```

The window title updates every 0.5s with the current FPS.

## Usage

- **Q** — Quit the application
- **Scroll wheel** — Adjust the decay factor (smooths bar falloff)

## Dependencies

| Crate | Purpose |
|---|---|
| `cpal` | Cross-platform audio capture |
| `rtrb` | Lock-free SPSC ring buffer between capture and analysis threads |
| `rustfft` | FFT backend for both the CQT kernels and per-frame transform |
| `arc-swap` | Lock-free hand-off of the latest analysis frame to the render thread |
| `wgpu` | GPU rendering |
| `winit` | Windowing / event loop |
| `bytemuck` | Safe casting of Rust structs to GPU buffer bytes |
| `pollster` | Blocking on wgpu's async device/adapter setup at startup |
