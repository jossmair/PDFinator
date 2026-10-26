//! Check the native cursor backend on an owned hidden window without user input.
#[allow(deprecated)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    let window = event_loop.create_window(
        winit::window::Window::default_attributes()
            .with_visible(false)
            .with_title("PDFinator native cursor check"),
    )?;
    window.set_cursor(winit::window::CursorIcon::Grab);
    window.set_cursor(winit::window::CursorIcon::Grabbing);
    Ok(())
}
