//! A real Wayland top-level window.
//!
//! Only the top-level path exists here, and that is not a shortcut. Wayland has
//! no XEmbed equivalent, upstream CLAP says so outright — *"embed is currently
//! not supported, use floating windows"* — and VST3 has no Wayland platform
//! type at all. So an embedded editor keeps going through the X11 backend
//! (under XWayland on a Wayland desktop), and this serves the floating case.
//!
//! What this does *not* do yet is host a renderer. Attaching an EGL surface for
//! GL comes next; the window here paints a single solid colour through
//! `wl_shm`, which is enough to prove the protocol handshake end to end and is
//! testable on a headless compositor with no GPU.

use std::convert::TryFrom;
use std::os::fd::AsFd;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_registry, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

/// How far the window got through the protocol handshake.
///
/// A Wayland window is not "created" in one call: the compositor must send a
/// `configure` and the client must acknowledge it before anything is on screen.
/// Reporting the stage reached is what makes a failure diagnosable instead of
/// just "no window".
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ToplevelProgress {
    /// `xdg_wm_base` and `wl_compositor` were both bound.
    pub globals_bound: bool,
    /// The compositor sent `xdg_surface::configure` and it was acknowledged.
    pub configured: bool,
    /// A buffer was attached and committed.
    pub buffer_attached: bool,
    /// The compositor asked the window to close.
    pub close_requested: bool,
}

impl ToplevelProgress {
    /// Whether a window actually made it onto the compositor.
    pub fn is_mapped(&self) -> bool {
        self.globals_bound && self.configured && self.buffer_attached
    }
}

/// Why a top-level window could not be created.
#[derive(Debug)]
pub enum ToplevelError {
    NoCompositor(String),
    /// The compositor is missing a global this backend needs. Carries the
    /// interface name, so the message says what to look for.
    MissingGlobal(&'static str),
    Protocol(String),
    /// The shared-memory buffer could not be created.
    Buffer(String),
}

impl std::fmt::Display for ToplevelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCompositor(reason) => write!(formatter, "no Wayland compositor: {reason}"),
            Self::MissingGlobal(interface) => {
                write!(formatter, "compositor does not offer {interface}")
            }
            Self::Protocol(reason) => write!(formatter, "Wayland protocol error: {reason}"),
            Self::Buffer(reason) => write!(formatter, "could not create a buffer: {reason}"),
        }
    }
}

#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    wm_base: Option<xdg_wm_base::XdgWmBase>,
    shm: Option<wl_shm::WlShm>,
    xdg_surface: Option<xdg_surface::XdgSurface>,
    surface: Option<wl_surface::WlSurface>,
    progress: ToplevelProgress,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                state.compositor = Some(registry.bind(name, version.min(4), handle, ()));
            }
            "xdg_wm_base" => {
                state.wm_base = Some(registry.bind(name, version.min(3), handle, ()));
            }
            "wl_shm" => {
                state.shm = Some(registry.bind(name, version.min(1), handle, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _state: &mut Self,
        wm_base: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        // Answering ping is mandatory: a client that stays silent is declared
        // unresponsive and may be killed by the compositor.
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        xdg_surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            // The handshake: nothing is shown until this is acknowledged.
            xdg_surface.ack_configure(serial);
            state.progress.configured = true;
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, ()> for State {
    fn event(
        state: &mut Self,
        _toplevel: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let xdg_toplevel::Event::Close = event {
            state.progress.close_requested = true;
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_shm::WlShm);
delegate_noop!(State: ignore wl_shm_pool::WlShmPool);
delegate_noop!(State: ignore wl_buffer::WlBuffer);

/// Open a top-level window, drive the handshake, and report how far it got.
///
/// Synchronous and short-lived by design: this is the protocol path a real
/// backend needs, exercised end to end without an event loop or a renderer
/// attached yet.
pub fn open_toplevel(
    title: &str,
    width: u32,
    height: u32,
) -> Result<ToplevelProgress, ToplevelError> {
    let connection = Connection::connect_to_env()
        .map_err(|error| ToplevelError::NoCompositor(error.to_string()))?;
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    connection.display().get_registry(&handle, ());

    let mut state = State::default();
    queue
        .roundtrip(&mut state)
        .map_err(|error| ToplevelError::Protocol(error.to_string()))?;

    let compositor = state
        .compositor
        .clone()
        .ok_or(ToplevelError::MissingGlobal("wl_compositor"))?;
    let wm_base = state
        .wm_base
        .clone()
        .ok_or(ToplevelError::MissingGlobal("xdg_wm_base"))?;
    let shm = state
        .shm
        .clone()
        .ok_or(ToplevelError::MissingGlobal("wl_shm"))?;
    state.progress.globals_bound = true;

    let surface = compositor.create_surface(&handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
    let toplevel = xdg_surface.get_toplevel(&handle, ());
    toplevel.set_title(title.to_string());
    toplevel.set_app_id("sunmao".to_string());
    // The first commit asks for a configure; nothing may be attached before it.
    surface.commit();
    state.surface = Some(surface.clone());
    state.xdg_surface = Some(xdg_surface.clone());

    queue
        .roundtrip(&mut state)
        .map_err(|error| ToplevelError::Protocol(error.to_string()))?;

    if state.progress.configured {
        let buffer = solid_buffer(&shm, &handle, width, height)?;
        surface.attach(Some(&buffer), 0, 0);
        surface.damage(0, 0, width as i32, height as i32);
        surface.commit();
        state.progress.buffer_attached = true;
        queue
            .roundtrip(&mut state)
            .map_err(|error| ToplevelError::Protocol(error.to_string()))?;
    }

    let progress = state.progress;
    toplevel.destroy();
    xdg_surface.destroy();
    surface.destroy();
    Ok(progress)
}

/// A single-colour ARGB buffer in shared memory.
///
/// `wl_shm` rather than EGL because a headless compositor has no GPU: this
/// proves the protocol path on a runner where GL cannot be created at all.
fn solid_buffer(
    shm: &wl_shm::WlShm,
    handle: &QueueHandle<State>,
    width: u32,
    height: u32,
) -> Result<wl_buffer::WlBuffer, ToplevelError> {
    use std::io::Write;
    let (stride, size) = buffer_layout(width, height)?;

    let mut file =
        tempfile_of(size as usize).map_err(|error| ToplevelError::Buffer(error.to_string()))?;
    // ARGB is a native-endian packed integer; alpha must be opaque.
    let pixel = 0xff305060_u32.to_ne_bytes();
    let row: Vec<u8> = pixel
        .iter()
        .copied()
        .cycle()
        .take(stride as usize)
        .collect();
    for _ in 0..height {
        file.write_all(&row)
            .map_err(|error| ToplevelError::Buffer(error.to_string()))?;
    }
    let pool = shm.create_pool(file.as_fd(), size as i32, handle, ());
    let buffer = pool.create_buffer(
        0,
        width as i32,
        height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
        handle,
        (),
    );
    // The compositor keeps its own reference to the mapping, so the pool can go
    // as soon as the buffer exists.
    pool.destroy();
    Ok(buffer)
}

fn buffer_layout(width: u32, height: u32) -> Result<(i32, i32), ToplevelError> {
    let invalid = || ToplevelError::Buffer("dimensions exceed the wl_shm signed size range".into());
    if width == 0 || height == 0 {
        return Err(invalid());
    }
    let stride = width.checked_mul(4).ok_or_else(invalid)?;
    let size = stride.checked_mul(height).ok_or_else(invalid)?;
    Ok((
        i32::try_from(stride).map_err(|_| invalid())?,
        i32::try_from(size).map_err(|_| invalid())?,
    ))
}

/// An anonymous, unlinked file of exactly `size` bytes to share with the
/// compositor.
fn tempfile_of(size: usize) -> std::io::Result<std::fs::File> {
    use std::io::{Seek, SeekFrom, Write};

    let mut path = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    // The name only has to be unique within this process for the instant
    // between creation and unlinking.
    path.push(format!("sunmao-wl-{}", std::process::id()));

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    // Unlink immediately: the descriptor keeps it alive, and nothing should be
    // left behind if this process dies.
    let _ = std::fs::remove_file(&path);

    let mut file = file;
    file.seek(SeekFrom::Start(size as u64 - 1))?;
    file.write_all(&[0])?;
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "opengl")]
    #[test]
    fn egl_renders_and_resizes_a_wayland_surface() {
        use glow::HasContext;
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            println!("WAYLAND EGL SKIPPED: WAYLAND_DISPLAY is unset");
            return;
        }
        let connection = Connection::connect_to_env().expect("Wayland connection");
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let mut state = State::default();
        queue.roundtrip(&mut state).unwrap();
        let surface = state
            .compositor
            .as_ref()
            .unwrap()
            .create_surface(&handle, ());
        let shell_surface = state
            .wm_base
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, &handle, ());
        let toplevel = shell_surface.get_toplevel(&handle, ());
        toplevel.set_title("SunMao EGL acceptance".into());
        surface.commit();
        queue.roundtrip(&mut state).unwrap();
        assert!(state.progress.configured);
        let config = crate::gl::GlConfig {
            srgb: false,
            ..Default::default()
        };
        let context =
            super::super::egl::Context::new(&connection, &surface, 64, 48, &config).unwrap();
        context.make_current().unwrap();
        let gl =
            unsafe { glow::Context::from_loader_function(|name| context.get_proc_address(name)) };
        for (width, height, expected) in [(64, 48, [255_u8, 0, 0, 255]), (96, 72, [0, 255, 0, 255])]
        {
            context.resize(width, height);
            // Swap once to apply the native resize before inspecting the new buffer.
            context.swap_buffers().unwrap();
            unsafe {
                gl.viewport(0, 0, width, height);
                gl.clear_color(
                    expected[0] as f32 / 255.0,
                    expected[1] as f32 / 255.0,
                    0.0,
                    1.0,
                );
                gl.clear(glow::COLOR_BUFFER_BIT);
                let mut pixel = [0_u8; 4];
                gl.read_pixels(
                    width - 1,
                    height - 1,
                    1,
                    1,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelPackData::Slice(Some(&mut pixel)),
                );
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                assert_eq!(pixel, expected, "rendered pixel after resize");
            }
            context.swap_buffers().unwrap();
            queue.roundtrip(&mut state).unwrap();
        }
        context.make_not_current().unwrap();
        drop(gl);
        drop(context);
        toplevel.destroy();
        shell_surface.destroy();
        surface.destroy();
        queue.roundtrip(&mut state).unwrap();
        println!("WAYLAND EGL VERIFIED: pixels, resize, swap and teardown");
    }

    #[test]
    fn shm_layout_rejects_zero_and_overflowing_dimensions() {
        assert_eq!(buffer_layout(320, 180).unwrap(), (1280, 230400));
        for (width, height) in [(0, 1), (1, 0), (u32::MAX, 1), (1, u32::MAX), (32768, 32768)] {
            assert!(buffer_layout(width, height).is_err());
        }
    }

    /// Open a real window on whatever compositor is present.
    ///
    /// Skips loudly when there is none — on X11 and on a runner without one
    /// that is the expected answer, and a silent skip would read exactly like a
    /// pass.
    #[test]
    fn a_toplevel_window_reaches_the_compositor() {
        // `connect_to_env` may consult the compositor socket selected by the
        // desktop environment.  On a normal X11/macOS development shell
        // there is no Wayland session at all; avoid probing an implicit
        // socket in that case, which can block while the socket is absent.
        if std::env::var_os("WAYLAND_DISPLAY").is_none() {
            println!("WAYLAND TOPLEVEL SKIPPED: WAYLAND_DISPLAY is unset");
            return;
        }
        match open_toplevel("SunMao Wayland", 320, 180) {
            Ok(progress) => {
                println!("WAYLAND TOPLEVEL: {progress:?}");
                assert!(progress.globals_bound, "globals were not bound");
                assert!(
                    progress.configured,
                    "the compositor never sent xdg_surface::configure"
                );
                assert!(progress.buffer_attached, "no buffer was attached");
                assert!(progress.is_mapped());
                println!("WAYLAND TOPLEVEL VERIFIED: window mapped on the compositor");
            }
            Err(error) => {
                panic!("WAYLAND TOPLEVEL FAILED: {error}");
            }
        }
    }

    #[test]
    fn progress_only_counts_as_mapped_when_every_stage_completed() {
        let mut progress = ToplevelProgress {
            globals_bound: true,
            configured: true,
            buffer_attached: false,
            close_requested: false,
        };
        // Configured but nothing attached is a window with no content — the
        // compositor shows nothing, so calling it mapped would be a lie.
        assert!(!progress.is_mapped());
        progress.buffer_attached = true;
        assert!(progress.is_mapped());
        assert!(!ToplevelProgress::default().is_mapped());
    }
}
