//! Is there a usable Wayland compositor here?
//!
//! A Wayland backend is worth nothing without somewhere to run it, and a CI
//! runner has no compositor unless one is started explicitly. This connects,
//! enumerates the registry, and reports which globals are present — so a build
//! that "supports Wayland" can be told apart from one that merely compiled the
//! code.

use std::fmt;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};

/// What a compositor offers, as far as this backend cares.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WaylandCapabilities {
    /// Every global the compositor advertised, by interface name.
    pub globals: Vec<String>,
}

impl WaylandCapabilities {
    pub fn has(&self, interface: &str) -> bool {
        self.globals.iter().any(|name| name == interface)
    }

    /// Whether a top-level window could actually be created here.
    ///
    /// `wl_compositor` makes surfaces and `xdg_wm_base` turns one into a window
    /// with a title and a close button. Without both, there is nothing to put
    /// an editor in.
    pub fn can_open_a_window(&self) -> bool {
        self.has("wl_compositor") && self.has("xdg_wm_base")
    }

    /// Whether keyboard and pointer input can be received.
    pub fn has_input(&self) -> bool {
        self.has("wl_seat")
    }
}

/// Why a compositor could not be reached.
#[derive(Debug)]
pub enum WaylandProbeError {
    /// No `WAYLAND_DISPLAY`, or the socket refused the connection. This is the
    /// ordinary answer on X11 and on a bare CI runner, not a failure.
    NoCompositor(String),
    /// Connected, but the registry round trip failed.
    RegistryUnavailable(String),
}

impl fmt::Display for WaylandProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCompositor(reason) => write!(formatter, "no Wayland compositor: {reason}"),
            Self::RegistryUnavailable(reason) => {
                write!(formatter, "Wayland registry unavailable: {reason}")
            }
        }
    }
}

#[derive(Default)]
struct RegistryCollector {
    capabilities: WaylandCapabilities,
}

impl Dispatch<wayland_client::protocol::wl_callback::WlCallback, Arc<AtomicBool>>
    for RegistryCollector
{
    fn event(
        _: &mut Self,
        _: &wayland_client::protocol::wl_callback::WlCallback,
        _: wayland_client::protocol::wl_callback::Event,
        done: &Arc<AtomicBool>,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        done.store(true, Ordering::Release);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for RegistryCollector {
    fn event(
        state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        _handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { interface, .. } = event {
            state.capabilities.globals.push(interface);
        }
    }
}

/// Connect to the compositor named by `WAYLAND_DISPLAY` and list its globals.
///
/// Returns `Err(NoCompositor)` when there is no Wayland session, which is the
/// normal case on X11 — callers should treat it as "use the X11 backend", not
/// as an error.
pub fn probe() -> Result<WaylandCapabilities, WaylandProbeError> {
    let connection = Connection::connect_to_env()
        .map_err(|error| WaylandProbeError::NoCompositor(error.to_string()))?;
    let display = connection.display();

    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    display.get_registry(&handle, ());

    let mut collector = RegistryCollector::default();
    // One round trip is enough: the compositor advertises every global it has
    // immediately after `get_registry`.
    super::dispatch::roundtrip(
        &connection,
        &mut queue,
        &mut collector,
        Duration::from_secs(5),
    )
    .map_err(|error| WaylandProbeError::RegistryUnavailable(error.to_string()))?;

    collector.capabilities.globals.sort();
    collector.capabilities.globals.dedup();
    Ok(collector.capabilities)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Probing must never panic, whatever the session is. On X11 or a bare
    /// runner it reports "no compositor"; under one it lists the globals.
    #[test]
    fn probing_reports_a_verdict_instead_of_panicking() {
        match probe() {
            Ok(capabilities) => {
                println!(
                    "WAYLAND PROBE: connected, {} globals, window={} input={}",
                    capabilities.globals.len(),
                    capabilities.can_open_a_window(),
                    capabilities.has_input()
                );
                for global in &capabilities.globals {
                    println!("WAYLAND GLOBAL: {global}");
                }
                // A compositor that answered at all must offer surfaces.
                assert!(
                    capabilities.has("wl_compositor"),
                    "a compositor without wl_compositor is not usable"
                );
            }
            Err(error) => {
                println!("WAYLAND PROBE: {error}");
            }
        }
    }

    #[test]
    fn capability_queries_do_not_confuse_absent_globals() {
        let capabilities = WaylandCapabilities {
            globals: vec!["wl_compositor".into(), "wl_seat".into()],
        };
        assert!(capabilities.has("wl_seat"));
        assert!(capabilities.has_input());
        // `xdg_wm_base` is missing, so no window can be opened even though a
        // compositor is present — reporting otherwise would strand the caller.
        assert!(!capabilities.can_open_a_window());
        assert!(!WaylandCapabilities::default().has_input());
    }
}
