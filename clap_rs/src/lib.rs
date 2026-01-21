pub mod plugin;
pub mod plugin_instance;
pub mod entry;
pub mod events;
pub mod process;
pub mod ext;
pub mod gui;

pub use plugin::{Plugin, PluginInfo, HostHandle, CLAP_VERSION, CLAP_PROCESS_CONTINUE};
pub use ext::{AudioPortInfo, NotePortInfo, ParameterInfo};
pub use entry::PluginEntry;

/// Re-export clap_sys for macro and type compatibility
/// Users don't need to add clap_sys as a dependency
#[doc(hidden)]
pub use clap_sys;

/// Re-export commonly used clap_sys types
pub use clap_sys::process::clap_process_status;
