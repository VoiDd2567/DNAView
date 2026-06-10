mod app;
mod config;
mod dna;
mod renderer;

use app::App;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("failed to create event loop");
    let window = WindowBuilder::new()
        .with_title("DNAView")
        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800))
        .build(&event_loop)
        .expect("failed to create window");

    let mut app = pollster::block_on(App::new(window));

    event_loop
        .run(move |event, target| {
            target.set_control_flow(ControlFlow::Poll);

            match event {
                Event::WindowEvent { event, window_id } if window_id == app.window().id() => {
                    if app.handle_window_event(&event) {
                        return;
                    }

                    match event {
                        WindowEvent::CloseRequested => target.exit(),
                        WindowEvent::Resized(size) => app.resize(size),
                        WindowEvent::ScaleFactorChanged { .. } => {
                            app.resize(app.window().inner_size());
                        }
                        WindowEvent::RedrawRequested => {
                            app.update();
                            match app.render() {
                                Ok(()) => {}
                                Err(wgpu::SurfaceError::Lost) => {
                                    app.resize(app.window().inner_size())
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => target.exit(),
                                Err(err) => log::warn!("render error: {err:?}"),
                            }
                        }
                        _ => {}
                    }
                }
                Event::AboutToWait => {
                    app.window().request_redraw();
                }
                _ => {}
            }
        })
        .expect("event loop failed");
}
