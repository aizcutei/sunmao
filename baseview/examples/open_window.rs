use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Duration;

use rtrb::{Consumer, RingBuffer};

#[cfg(target_os = "macos")]
use baseview::{copy_to_clipboard, MouseEvent};
use baseview::{
    Event, EventStatus, PhySize, Window, WindowEvent, WindowHandler, WindowScalePolicy,
};

#[derive(Debug, Clone)]
enum Message {
    Hello,
}

struct OpenWindowExample {
    rx: Consumer<Message>,

    // softbuffer 0.4 requires owned references or Arc - we use 'static lifetime
    _ctx: softbuffer::Context<Rc<WrappedWindow>>,
    surface: softbuffer::Surface<Rc<WrappedWindow>, Rc<WrappedWindow>>,
    current_size: PhySize,
    damaged: bool,
}

// Wrapper to own the window handle for softbuffer
struct WrappedWindow {
    raw_window_handle: raw_window_handle::RawWindowHandle,
    raw_display_handle: raw_window_handle::RawDisplayHandle,
}

impl raw_window_handle::HasWindowHandle for WrappedWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.raw_window_handle) })
    }
}

impl raw_window_handle::HasDisplayHandle for WrappedWindow {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.raw_display_handle) })
    }
}

impl WindowHandler for OpenWindowExample {
    fn on_frame(&mut self, _window: &mut Window) {
        let mut buf = self.surface.buffer_mut().unwrap();
        if self.damaged {
            buf.fill(0xFFAAAAAA);
            self.damaged = false;
        }
        buf.present().unwrap();

        while let Ok(message) = self.rx.pop() {
            println!("Message: {:?}", message);
        }
    }

    fn on_event(&mut self, _window: &mut Window, event: Event) -> EventStatus {
        match &event {
            #[cfg(target_os = "macos")]
            Event::Mouse(MouseEvent::ButtonPressed { .. }) => copy_to_clipboard("This is a test!"),
            Event::Window(WindowEvent::Resized(info)) => {
                println!("Resized: {:?}", info);
                let new_size = info.physical_size();
                self.current_size = new_size;

                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(new_size.width), NonZeroU32::new(new_size.height))
                {
                    self.surface.resize(width, height).unwrap();
                    self.damaged = true;
                }
            }
            _ => {}
        }

        log_event(&event);

        EventStatus::Captured
    }
}

fn main() {
    let window_open_options = baseview::WindowOpenOptions {
        title: "baseview".into(),
        size: baseview::Size::new(512.0, 512.0),
        scale: WindowScalePolicy::SystemScaleFactor,

        // TODO: Add an example that uses the OpenGL context
        #[cfg(feature = "opengl")]
        gl_config: None,
    };

    let (mut tx, rx) = RingBuffer::new(128);

    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));

        if tx.push(Message::Hello).is_err() {
            println!("Failed sending message");
        }
    });

    Window::open_blocking(window_open_options, |window| {
        use raw_window_handle::{HasWindowHandle, HasDisplayHandle};
        
        // Extract raw handles from baseview window
        let raw_window = window.window_handle().unwrap().as_raw();
        let raw_display = window.display_handle().unwrap().as_raw();
        
        // Create wrapped window for softbuffer (needs owned reference)
        let wrapped = Rc::new(WrappedWindow {
            raw_window_handle: raw_window,
            raw_display_handle: raw_display,
        });
        
        let ctx = softbuffer::Context::new(wrapped.clone()).unwrap();
        let mut surface = softbuffer::Surface::new(&ctx, wrapped.clone()).unwrap();
        surface.resize(NonZeroU32::new(512).unwrap(), NonZeroU32::new(512).unwrap()).unwrap();

        OpenWindowExample {
            _ctx: ctx,
            surface,
            rx,
            current_size: PhySize::new(512, 512),
            damaged: true,
        }
    });
}

fn log_event(event: &Event) {
    match event {
        Event::Mouse(e) => println!("Mouse event: {:?}", e),
        Event::Keyboard(e) => println!("Keyboard event: {:?}", e),
        Event::Window(e) => println!("Window event: {:?}", e),
    }
}
