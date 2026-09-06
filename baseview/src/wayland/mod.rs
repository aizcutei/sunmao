//! Native Wayland support.
//!
//! Wayland is deliberately a *separate* backend rather than a variant of the
//! X11 one, because the two disagree on the thing baseview is built around:
//! embedding. X11 has XEmbed, so a plugin editor can be a child of the host's
//! window. Wayland has no equivalent, and upstream CLAP states it plainly —
//! *"embed is currently not supported, use floating windows"*. VST3 has no
//! Wayland platform type at all and always runs through XWayland.
//!
//! So this backend implements only the top-level window path, and only CLAP can
//! reach it. The X11 backend keeps serving every embedded editor, on Wayland
//! desktops too, via XWayland.

#[cfg(feature = "wayland")]
pub mod probe;
#[cfg(feature = "wayland")]
pub mod toplevel;

#[cfg(feature = "opengl")]
pub(crate) mod egl;
