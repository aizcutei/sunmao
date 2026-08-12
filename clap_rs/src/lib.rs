pub mod entry;
pub mod events;
pub mod ext;
pub mod gui;
pub mod plugin;
pub mod plugin_instance;
pub mod process;

pub use entry::PluginEntry;
pub use ext::{AudioPortInfo, NotePortInfo, ParameterInfo};
pub use plugin::{
    AudioProcessor, CLAP_PROCESS_CONTINUE, CLAP_VERSION, HostHandle, Plugin, PluginInfo,
};

/// Re-export clap_sys for macro and type compatibility
/// Users don't need to add clap_sys as a dependency
#[doc(hidden)]
pub use clap_sys;

/// Re-export commonly used clap_sys types
pub use clap_sys::process::clap_process_status;
