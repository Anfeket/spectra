use std::sync::Arc;

use wgpu::DeviceDescriptor;
use wgpu::{
    Instance, InstanceDescriptor, RenderPassColorAttachment, RenderPassDescriptor,
    RequestAdapterOptions, SurfaceColorSpace, SurfaceConfiguration, TextureViewDescriptor,
};
use winit::event::WindowEvent;
use winit::event_loop::ControlFlow;
use winit::keyboard::KeyCode::KeyQ;
use winit::keyboard::PhysicalKey;
use winit::{
    application::ApplicationHandler, dpi::PhysicalSize, event_loop::OwnedDisplayHandle,
    window::Window,
};

use crate::analysis::N_NOTES;
use crate::frame::SharedFrame;

static DECAY: f32 = 0.6;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BarInstance {
    x: f32,
    width: f32,
    height: f32,
    color: [f32; 3],
}

struct State {
    instance: wgpu::Instance,
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    shared: SharedFrame,
    smooth_energy: [f32; crate::analysis::N_NOTES],
    frame_count: usize,
    last_fps_update: std::time::Instant,
    decay: f32,
}

const fn note_color(i: usize) -> [f32; 3] {
    let hue = (i % N_NOTES) as f32 / N_NOTES as f32;

    let h = hue * 6.0;
    let c = 1.0;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b]
}

fn build_instances(energy: &[f32; N_NOTES]) -> Vec<BarInstance> {
    let bar_w = 1.0 / crate::analysis::N_NOTES as f32;
    let max_energy = 300.0;
    energy
        .iter()
        .enumerate()
        .map(|(i, &e)| BarInstance {
            x: i as f32 * bar_w,
            width: bar_w * 0.85,
            height: (e / max_energy).clamp(0.0, 1.0),
            color: note_color(i),
        })
        .collect()
}

impl State {
    async fn new(display: OwnedDisplayHandle, window: Arc<Window>, shared: SharedFrame) -> Self {
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle(Box::new(
            display,
        )));
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .expect("Failed to find an appropriate adapter");
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .expect("Failed to create device");

        let size = window.inner_size();

        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bars shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("bars.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<[f32; 2]>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            }],
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BarInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                1 => Float32,   // x
                2 => Float32,   // width
                3 => Float32,   // height
                4 => Float32x3, // color
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bars pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(vertex_layout), Some(instance_layout)],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        use wgpu::util::DeviceExt;
        const QUAD_VERTS: &[[f32; 2]] = &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad"),
            contents: bytemuck::cast_slice(QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bar instances"),
            size: (crate::analysis::N_NOTES * std::mem::size_of::<BarInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let smooth_energy = [0.0; crate::analysis::N_NOTES];

        let mut state = Self {
            instance,
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            shared,
            smooth_energy,
            pipeline,
            vertex_buffer,
            instance_buffer,
            frame_count: 0,
            last_fps_update: std::time::Instant::now(),
            decay: DECAY,
        };

        state.configure_surface();
        state
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();
    }

    fn get_window(&self) -> &Window {
        &self.window
    }

    fn configure_surface(&mut self) {
        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            color_space: SurfaceColorSpace::Auto,
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    fn render(&mut self) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => texture,
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                self.configure_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = self
                    .instance
                    .create_surface(self.window.clone())
                    .expect("Failed to recreate surface");
                self.configure_surface();
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                eprintln!("Surface validation error");
                return;
            }
        };
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.surface_format.add_srgb_suffix()),
            ..Default::default()
        });

        let frame = self.shared.load();
        for i in 0..crate::analysis::N_NOTES {
            self.smooth_energy[i] =
                self.smooth_energy[i] * self.decay + frame.note_energy[i] * (1.0 - self.decay);
        }
        let instances = build_instances(&self.smooth_energy);

        self.queue
            .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&instances));

        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut renderpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        renderpass.set_pipeline(&self.pipeline);
        renderpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        renderpass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        renderpass.draw(0..4, 0..instances.len() as u32);

        drop(renderpass);

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.queue.present(surface_texture);

        self.frame_count += 1;
        let elapsed = self.last_fps_update.elapsed();
        if elapsed.as_secs_f32() >= 0.5 {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.window
                .set_title(&format!("Audio Visualizer - {:.2} FPS", fps));
            self.frame_count = 0;
            self.last_fps_update = std::time::Instant::now();
        }
    }
}

#[derive(Default)]
struct App {
    state: Option<State>,
    frame: SharedFrame,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .expect("Failed to create window"),
        );
        let state = pollster::block_on(State::new(
            event_loop.owned_display_handle(),
            window.clone(),
            self.frame.clone(),
        ));
        self.state = Some(state);
        window.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                state.render();
                state.get_window().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                state.resize(new_size);
            }
            WindowEvent::KeyboardInput {
                event,
                device_id: _,
                is_synthetic: _,
            } => {
                if let winit::event::ElementState::Pressed = event.state {
                    if let PhysicalKey::Code(KeyQ) = event.physical_key {
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::MouseWheel { device_id, delta, phase } => {
                if let winit::event::MouseScrollDelta::LineDelta(_, y) = delta {
                    state.decay = (state.decay + y * 0.05).clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }
}

impl App {
    pub fn new(frame: SharedFrame) -> Self {
        Self { state: None, frame }
    }
}

pub fn run(frame: SharedFrame) {
    let event_loop = winit::event_loop::EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(frame);
    event_loop.run_app(&mut app).expect("Failed to run app");
}
