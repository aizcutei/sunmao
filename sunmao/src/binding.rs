//! Adapter from the host-facing [`ViewContext`] to the GUI layer's
//! [`ParamHost`].
//!
//! `sunmao_gui` does not depend on `sunmao_core` — it is a renderer-agnostic
//! widget crate and keeping it free of the plugin contract is what lets it be
//! tested without a host. The two meet here, in the facade, which already
//! depends on both.

use std::sync::Arc;
use sunmao_core::view::ViewContext;
use sunmao_gui::{ParamBinder, ParamHost};

/// Wraps a [`ViewContext`] so a [`ParamBinder`] can drive it.
///
/// ```no_run
/// # use std::sync::Arc;
/// # use sunmao::prelude::*;
/// # fn build(context: Arc<dyn ViewContext>) {
/// // One line replaces the per-control sync/set/begin/end boilerplate.
/// let mut binder = ParamBinder::new(ViewContextHost::shared(context));
/// # let _ = &mut binder;
/// # }
/// ```
pub struct ViewContextHost {
    context: Arc<dyn ViewContext>,
}

impl ViewContextHost {
    pub fn new(context: Arc<dyn ViewContext>) -> Self {
        Self { context }
    }

    /// The form [`ParamBinder::new`] wants.
    pub fn shared(context: Arc<dyn ViewContext>) -> Arc<dyn ParamHost> {
        Arc::new(Self::new(context))
    }
}

impl ParamHost for ViewContextHost {
    fn get(&self, id: &str) -> Option<f32> {
        self.context.get_param(id)
    }

    fn set(&self, id: &str, value: f32) {
        self.context.set_param(id, value);
    }

    fn begin_edit(&self, id: &str) {
        self.context.begin_edit(id);
    }

    fn end_edit(&self, id: &str) {
        self.context.end_edit(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyContext {
        log: Mutex<Vec<String>>,
    }

    impl ViewContext for SpyContext {
        fn get_param(&self, id: &str) -> Option<f32> {
            self.log.lock().unwrap().push(format!("get {id}"));
            Some(0.25)
        }
        fn set_param(&self, id: &str, value: f32) {
            self.log.lock().unwrap().push(format!("set {id}={value}"));
        }
        fn begin_edit(&self, id: &str) {
            self.log.lock().unwrap().push(format!("begin {id}"));
        }
        fn end_edit(&self, id: &str) {
            self.log.lock().unwrap().push(format!("end {id}"));
        }
        fn request_resize(&self, _width: u32, _height: u32) -> bool {
            false
        }
    }

    #[test]
    fn every_param_host_call_reaches_the_view_context_unchanged() {
        let spy = Arc::new(SpyContext::default());
        let host = ViewContextHost::new(spy.clone() as Arc<dyn ViewContext>);

        assert_eq!(host.get("gain"), Some(0.25));
        host.set("gain", 0.75);
        host.begin_edit("gain");
        host.end_edit("gain");

        assert_eq!(
            *spy.log.lock().unwrap(),
            vec![
                "get gain".to_string(),
                "set gain=0.75".to_string(),
                "begin gain".to_string(),
                "end gain".to_string(),
            ]
        );
    }
}
