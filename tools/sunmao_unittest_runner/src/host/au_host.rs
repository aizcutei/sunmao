use super::{HostPlugin, ParamInfo, PluginFormat, PluginInfo};
use crate::gui_window::PluginGuiWindow;
use au_sys::*;
use std::ffi::c_void;
use std::ffi::CStr;
use std::ptr;

pub struct AuHostPlugin {
    info: PluginInfo,
    unit: AudioComponentInstance,
    component: AudioComponent,
    input_buffers: Vec<Vec<f32>>,
    output_buffers: Vec<Vec<f32>>,
    sample_rate: f64,
    max_frames: u32,
    gui_view: *mut c_void,
}

unsafe impl Send for AuHostPlugin {}

impl AuHostPlugin {
    pub fn load(component: AudioComponent) -> Result<Self, String> {
        unsafe {
            let mut unit: AudioComponentInstance = ptr::null_mut();
            let status = AudioComponentInstanceNew(component, &mut unit);
            if status != 0 || unit.is_null() {
                return Err(format!("AudioComponentInstanceNew failed: {}", status));
            }

            let mut desc = AudioComponentDescription {
                componentType: 0,
                componentSubType: 0,
                componentManufacturer: 0,
                componentFlags: 0,
                componentFlagsMask: 0,
            };
            let _ = AudioComponentGetDescription(component, &mut desc);

            let name = au_component_name(component);
            let id = format!(
                "{}-{}-{}",
                fourcc_to_string(desc.componentType),
                fourcc_to_string(desc.componentSubType),
                fourcc_to_string(desc.componentManufacturer)
            );

            let info = PluginInfo {
                name,
                vendor: String::new(),
                version: String::new(),
                id,
                path: String::new(),
                format: PluginFormat::AU,
                class_index: 0,
                input_channels: 2,
                output_channels: 2,
                is_synth: desc.componentType == kAudioUnitType_MusicDevice
                    || desc.componentType == kAudioUnitType_MusicEffect,
            };

            Ok(Self {
                info,
                unit,
                component,
                input_buffers: vec![vec![0.0; 4096]; 2],
                output_buffers: vec![vec![0.0; 4096]; 2],
                sample_rate: 44100.0,
                max_frames: 4096,
                gui_view: ptr::null_mut(),
            })
        }
    }

    pub unsafe fn finish_gui_setup(
        &mut self,
        au_view: *mut c_void,
        requested_size: NSSize,
        window: &PluginGuiWindow,
    ) -> Result<(), String> {
        // Get the view's frame to determine its actual size
        let sel_frame = sel_registerName(b"frame\0".as_ptr() as *const _);
        let frame = objc_msg_send_rect(au_view, sel_frame);

        // Get intrinsic content size (plugin's preferred size)
        let sel_intrinsic = sel_registerName(b"intrinsicContentSize\0".as_ptr() as *const _);
        let intrinsic: NSSize = {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSSize =
                std::mem::transmute(objc_msgSend as *const c_void);
            f(au_view, sel_intrinsic)
        };

        // Log sizes for debugging
        eprintln!(
            "[AU GUI] Requested size: {}x{}",
            requested_size.width, requested_size.height
        );
        eprintln!(
            "[AU GUI] View frame: {}x{}",
            frame.size.width, frame.size.height
        );
        eprintln!(
            "[AU GUI] Intrinsic content size: {}x{}",
            intrinsic.width, intrinsic.height
        );

        // Use intrinsic size if available, otherwise use frame size
        let gui_width = if intrinsic.width > 1.0 {
            intrinsic.width
        } else {
            frame.size.width
        };
        let gui_height = if intrinsic.height > 1.0 {
            intrinsic.height
        } else {
            frame.size.height
        };

        // Resize window to fit the plugin view
        if gui_width > 0.0 && gui_height > 0.0 {
            eprintln!("[AU GUI] Resizing window to: {}x{}", gui_width, gui_height);
            window.set_content_view_size(gui_width, gui_height);
        }

        // Add the plugin view as subview of window's content view
        let sel_add_subview = sel_registerName(b"addSubview:\0".as_ptr() as *const _);
        objc_msg_send1(window.content_view(), sel_add_subview, au_view);

        self.gui_view = au_view;

        Ok(())
    }
}

impl HostPlugin for AuHostPlugin {
    fn info(&self) -> &PluginInfo {
        &self.info
    }

    fn initialize(&mut self, sample_rate: f64, max_frames: u32) -> Result<(), String> {
        self.sample_rate = sample_rate;
        self.max_frames = max_frames;
        self.input_buffers = vec![vec![0.0; max_frames as usize]; 2];
        self.output_buffers = vec![vec![0.0; max_frames as usize]; 2];

        unsafe {
            // Set sample rate
            let sr = sample_rate as Float64;
            let status = AudioUnitSetProperty(
                self.unit,
                kAudioUnitProperty_SampleRate,
                kAudioUnitScope_Global,
                0,
                &sr as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<Float64>() as UInt32,
            );
            if status != 0 {
                return Err(format!("Set SampleRate failed: {}", status));
            }

            // Set max frames per slice
            let max_frames_val = max_frames as UInt32;
            let status = AudioUnitSetProperty(
                self.unit,
                kAudioUnitProperty_MaximumFramesPerSlice,
                kAudioUnitScope_Global,
                0,
                &max_frames_val as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<UInt32>() as UInt32,
            );
            if status != 0 {
                return Err(format!("Set MaximumFramesPerSlice failed: {}", status));
            }

            // Set stream format (float32, non-interleaved, stereo)
            let format = AudioStreamBasicDescription {
                mSampleRate: sample_rate,
                mFormatID: kAudioFormatLinearPCM,
                mFormatFlags: kAudioFormatFlagIsFloat
                    | kAudioFormatFlagIsPacked
                    | kAudioFormatFlagIsNonInterleaved,
                mBytesPerPacket: 4,
                mFramesPerPacket: 1,
                mBytesPerFrame: 4,
                mChannelsPerFrame: 2,
                mBitsPerChannel: 32,
                mReserved: 0,
            };

            // Set output format
            let status = AudioUnitSetProperty(
                self.unit,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Output,
                0,
                &format as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<AudioStreamBasicDescription>() as UInt32,
            );
            if status != 0 {
                // Not fatal
            }

            // Set input format
            let status = AudioUnitSetProperty(
                self.unit,
                kAudioUnitProperty_StreamFormat,
                kAudioUnitScope_Input,
                0,
                &format as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<AudioStreamBasicDescription>() as UInt32,
            );
            if status != 0 {
                // Not fatal
            }

            // Set render callback
            let callback = AURenderCallbackStruct {
                inputProc: Some(render_callback),
                inputProcRefCon: self as *mut Self as *mut std::ffi::c_void,
            };
            let status = AudioUnitSetProperty(
                self.unit,
                kAudioUnitProperty_SetRenderCallback,
                kAudioUnitScope_Input,
                0,
                &callback as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<AURenderCallbackStruct>() as UInt32,
            );
            if status != 0 {
                return Err(format!("Set RenderCallback failed: {}", status));
            }

            // Initialize
            let status = AudioUnitInitialize(self.unit);
            if status != 0 {
                return Err(format!("AudioUnitInitialize failed: {}", status));
            }
        }
        Ok(())
    }

    fn param_count(&self) -> u32 {
        unsafe {
            let mut data_size: UInt32 = 0;
            let status = AudioUnitGetPropertyInfo(
                self.unit,
                kAudioUnitProperty_ParameterList,
                kAudioUnitScope_Global,
                0,
                &mut data_size,
                ptr::null_mut(),
            );
            if status != 0 {
                return 0;
            }
            data_size / std::mem::size_of::<AudioUnitParameterID>() as UInt32
        }
    }

    fn param_info(&self, index: u32) -> Option<ParamInfo> {
        unsafe {
            // Get parameter list
            let mut data_size: UInt32 = 0;
            let status = AudioUnitGetPropertyInfo(
                self.unit,
                kAudioUnitProperty_ParameterList,
                kAudioUnitScope_Global,
                0,
                &mut data_size,
                ptr::null_mut(),
            );
            if status != 0 {
                return None;
            }

            let count = data_size / std::mem::size_of::<AudioUnitParameterID>() as UInt32;
            if index >= count {
                return None;
            }

            let mut param_ids = vec![0u32; count as usize];
            let status = AudioUnitGetProperty(
                self.unit,
                kAudioUnitProperty_ParameterList,
                kAudioUnitScope_Global,
                0,
                param_ids.as_mut_ptr() as *mut std::ffi::c_void,
                &mut data_size,
            );
            if status != 0 {
                return None;
            }

            let param_id = param_ids[index as usize];

            // Get parameter info
            let mut info = std::mem::zeroed::<AudioUnitParameterInfo>();
            let mut info_size = std::mem::size_of::<AudioUnitParameterInfo>() as UInt32;
            let status = AudioUnitGetProperty(
                self.unit,
                kAudioUnitProperty_ParameterInfo,
                kAudioUnitScope_Global,
                param_id,
                &mut info as *mut _ as *mut std::ffi::c_void,
                &mut info_size,
            );
            if status != 0 {
                return Some(ParamInfo {
                    id: param_id,
                    name: format!("param_{}", param_id),
                    min: 0.0,
                    max: 1.0,
                    default: 0.0,
                    is_stepped: false,
                    can_automate: false,
                });
            }

            let name = if info.name[0] != 0 {
                CStr::from_ptr(info.name.as_ptr())
                    .to_str()
                    .unwrap_or("")
                    .to_string()
            } else {
                format!("param_{}", param_id)
            };

            Some(ParamInfo {
                id: param_id,
                name,
                min: info.minValue as f64,
                max: info.maxValue as f64,
                default: info.defaultValue as f64,
                is_stepped: info.unit == kAudioUnitParameterUnit_Indexed,
                can_automate: false,
            })
        }
    }

    fn param_get(&self, id: u32) -> Option<f64> {
        unsafe {
            let mut value: AudioUnitParameterValue = 0.0;
            let status =
                AudioUnitGetParameter(self.unit, id, kAudioUnitScope_Global, 0, &mut value);
            if status == 0 {
                Some(value as f64)
            } else {
                None
            }
        }
    }

    fn param_set(&mut self, id: u32, value: f64) -> Result<(), String> {
        unsafe {
            let status = AudioUnitSetParameter(
                self.unit,
                id,
                kAudioUnitScope_Global,
                0,
                value as AudioUnitParameterValue,
                0,
            );
            if status == 0 {
                Ok(())
            } else {
                Err(format!("AudioUnitSetParameter failed: {}", status))
            }
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), String> {
        let frames = (input.len() / 2)
            .min(output.len() / 2)
            .min(self.max_frames as usize);

        // Store input for render callback
        for i in 0..frames {
            self.input_buffers[0][i] = input[i * 2];
            self.input_buffers[1][i] = input[i * 2 + 1];
            self.output_buffers[0][i] = 0.0;
            self.output_buffers[1][i] = 0.0;
        }

        unsafe {
            // Allocate AudioBufferList with 2 buffers (flexible array member)
            let list_size =
                std::mem::size_of::<AudioBufferList>() + std::mem::size_of::<AudioBuffer>(); // +1 extra buffer
            let mut list_buf = vec![0u8; list_size];
            let list_ptr = list_buf.as_mut_ptr() as *mut AudioBufferList;
            (*list_ptr).mNumberBuffers = 2;
            (*list_ptr).mBuffers[0] = AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: (frames * 4) as UInt32,
                mData: self.output_buffers[0].as_mut_ptr() as *mut std::ffi::c_void,
            };
            // Write second buffer at offset past the first
            let buf_ptr = (&mut (*list_ptr).mBuffers[0] as *mut AudioBuffer).add(1);
            *buf_ptr = AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: (frames * 4) as UInt32,
                mData: self.output_buffers[1].as_mut_ptr() as *mut std::ffi::c_void,
            };

            let status = AudioUnitRender(
                self.unit,
                ptr::null_mut(),
                ptr::null(),
                0,
                frames as UInt32,
                list_ptr,
            );
            if status != 0 {
                return Err(format!("AudioUnitRender failed: {}", status));
            }
        }

        // Interleave output
        for i in 0..frames {
            output[i * 2] = self.output_buffers[0][i];
            output[i * 2 + 1] = self.output_buffers[1][i];
        }

        Ok(())
    }

    fn reset(&mut self) -> Result<(), String> {
        unsafe {
            let status = AudioUnitReset(self.unit, kAudioUnitScope_Global, 0);
            if status != 0 {
                return Err(format!("AudioUnitReset failed: {status}"));
            }
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, String> {
        unsafe {
            let mut data_size: UInt32 = 0;
            let status = AudioUnitGetPropertyInfo(
                self.unit,
                kAudioUnitProperty_ClassInfo,
                kAudioUnitScope_Global,
                0,
                &mut data_size,
                ptr::null_mut(),
            );
            if status != 0 {
                return Err(format!("GetPropertyInfo(ClassInfo) failed: {}", status));
            }

            let mut property_list: CFTypeRef = ptr::null();
            let status = AudioUnitGetProperty(
                self.unit,
                kAudioUnitProperty_ClassInfo,
                kAudioUnitScope_Global,
                0,
                &mut property_list as *mut _ as *mut std::ffi::c_void,
                &mut data_size,
            );
            if status != 0 {
                return Err(format!("GetProperty(ClassInfo) failed: {}", status));
            }

            // For now, just indicate success with size info
            // Full plist serialization would need CFPropertyListSerialization
            CFRelease(property_list);
            Ok(vec![])
        }
    }

    fn load_state(&mut self, _data: &[u8]) -> Result<(), String> {
        Err("AU state load not yet implemented".into())
    }

    fn shutdown(&mut self) {
        self.close_gui();
        unsafe {
            AudioUnitUninitialize(self.unit);
            AudioComponentInstanceDispose(self.unit);
        }
    }

    fn open_gui(&mut self, window: &PluginGuiWindow) -> Result<(), String> {
        unsafe {
            // Query kAudioUnitProperty_CocoaUI
            let mut data_size: UInt32 = 0;
            let status = AudioUnitGetPropertyInfo(
                self.unit,
                kAudioUnitProperty_CocoaUI,
                kAudioUnitScope_Global,
                0,
                &mut data_size,
                ptr::null_mut(),
            );
            if status != 0 {
                return Err(format!("Plugin has no Cocoa UI (status: {})", status));
            }

            let mut view_info: AudioUnitCocoaViewInfo = std::mem::zeroed();
            let status = AudioUnitGetProperty(
                self.unit,
                kAudioUnitProperty_CocoaUI,
                kAudioUnitScope_Global,
                0,
                &mut view_info as *mut _ as *mut c_void,
                &mut data_size,
            );
            if status != 0 {
                return Err(format!("Get CocoaUI property failed: {}", status));
            }

            // Get the class name from the CocoaUI property
            let class_name = {
                let cf_str = view_info.mCocoaAUViewClass[0];
                let len = CFStringGetLength(cf_str);
                let mut buf = vec![0u8; (len as usize) * 4 + 1];
                let ok = CFStringGetCString(
                    cf_str,
                    buf.as_mut_ptr() as *mut libc::c_char,
                    buf.len() as CFIndex,
                    kCFStringEncodingUTF8,
                );
                if !ok {
                    return Err("Failed to get class name from CocoaUI".into());
                }
                CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                    .to_str()
                    .map_err(|e| format!("Invalid class name: {}", e))?
                    .to_string()
            };

            eprintln!("[AU GUI] CocoaUI class: {}", class_name);

            // Try to get the class directly using NSClassFromString
            // (works for dynamically registered classes in the same process)
            let ns_string_cls = objc_getClass(b"NSString\0".as_ptr() as *const _);
            let sel_utf8 = sel_registerName(b"stringWithUTF8String:\0".as_ptr() as *const _);
            let ns_class_name =
                objc_msg_send1(ns_string_cls, sel_utf8, class_name.as_ptr() as *mut c_void);

            let ns_class = objc_getClass(class_name.as_ptr() as *const _);
            if ns_class.is_null() {
                // Fallback: try loading from bundle
                eprintln!("[AU GUI] Class not found in process, trying bundle...");
                let ns_bundle_cls = objc_getClass(b"NSBundle\0".as_ptr() as *const _);
                let sel_alloc = sel_registerName(b"alloc\0".as_ptr() as *const _);
                let sel_init_url = sel_registerName(b"initWithURL:\0".as_ptr() as *const _);
                let sel_principal_class =
                    sel_registerName(b"principalClass\0".as_ptr() as *const _);
                let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);

                let bundle_alloc = objc_msg_send0(ns_bundle_cls, sel_alloc);
                let bundle = objc_msg_send1(
                    bundle_alloc,
                    sel_init_url,
                    view_info.mCocoaAUViewBundleLocation as *mut c_void,
                );
                if bundle.is_null() {
                    return Err("NSBundle initWithURL returned null".into());
                }

                let factory_class = objc_msg_send0(bundle, sel_principal_class);
                if factory_class.is_null() {
                    objc_msg_send_void(bundle, sel_release);
                    return Err(format!(
                        "Bundle has no principal class (URL: {:?})",
                        view_info.mCocoaAUViewBundleLocation
                    ));
                }

                // Call uiViewForAudioUnit:withSize: on the class
                let sel_uiview =
                    sel_registerName(b"uiViewForAudioUnit:withSize:\0".as_ptr() as *const _);
                let size = NSSize {
                    width: 400.0,
                    height: 300.0,
                };
                let au_view =
                    objc_msg_send_uiview(factory_class, sel_uiview, self.unit as *mut c_void, size);

                objc_msg_send_void(bundle, sel_release);

                if au_view.is_null() {
                    return Err("uiViewForAudioUnit returned null".into());
                }

                self.finish_gui_setup(au_view, size, window)?;
            } else {
                // Class found in process, call directly
                eprintln!("[AU GUI] Using class from process");
                let sel_uiview =
                    sel_registerName(b"uiViewForAudioUnit:withSize:\0".as_ptr() as *const _);

                // Probe with small size to get plugin's preferred size
                let probe_size = NSSize {
                    width: 1.0,
                    height: 1.0,
                };
                let probe_view = objc_msg_send_uiview(
                    ns_class,
                    sel_uiview,
                    self.unit as *mut c_void,
                    probe_size,
                );

                if probe_view.is_null() {
                    return Err("uiViewForAudioUnit returned null".into());
                }

                // Get intrinsic content size (plugin's preferred size)
                let sel_intrinsic =
                    sel_registerName(b"intrinsicContentSize\0".as_ptr() as *const _);
                let intrinsic: NSSize = {
                    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSSize =
                        std::mem::transmute(objc_msgSend as *const c_void);
                    f(probe_view, sel_intrinsic)
                };

                // Release the probe view
                let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);
                objc_msg_send_void(probe_view, sel_release);

                // Create the view with the preferred size if available
                let size = if intrinsic.width > 1.0 && intrinsic.height > 1.0 {
                    eprintln!(
                        "[AU GUI] Plugin preferred size: {}x{}",
                        intrinsic.width, intrinsic.height
                    );
                    intrinsic
                } else {
                    let default_size = NSSize {
                        width: 400.0,
                        height: 300.0,
                    };
                    eprintln!(
                        "[AU GUI] Using default size: {}x{}",
                        default_size.width, default_size.height
                    );
                    default_size
                };

                let au_view =
                    objc_msg_send_uiview(ns_class, sel_uiview, self.unit as *mut c_void, size);
                if au_view.is_null() {
                    return Err("uiViewForAudioUnit returned null (2nd call)".into());
                }

                self.finish_gui_setup(au_view, size, window)?;
            }

            Ok(())
        }
    }

    fn close_gui(&mut self) {
        if !self.gui_view.is_null() {
            unsafe {
                let sel_remove = sel_registerName(b"removeFromSuperview\0".as_ptr() as *const _);
                objc_msg_send_void(self.gui_view, sel_remove);
                let sel_release = sel_registerName(b"release\0".as_ptr() as *const _);
                objc_msg_send_void(self.gui_view, sel_release);
            }
            self.gui_view = ptr::null_mut();
        }
    }
}

unsafe extern "C" fn render_callback(
    in_ref_con: *mut std::ffi::c_void,
    _io_action_flags: *mut u32,
    _in_time_stamp: *const AudioTimeStamp,
    _in_bus_number: u32,
    in_number_frames: u32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    // Feed the test input (stored in AuHostPlugin::input_buffers by process())
    // into the AU's input bus. If we don't have a refcon, fall back to silence.
    let host = if in_ref_con.is_null() {
        None
    } else {
        Some(&*(in_ref_con as *const AuHostPlugin))
    };

    let list = &*io_data;
    let buf_ptr = list.mBuffers.as_ptr();
    for i in 0..list.mNumberBuffers as usize {
        let buf = &*buf_ptr.add(i);
        if buf.mData.is_null() {
            continue;
        }
        let n = (buf.mDataByteSize as usize / 4).min(in_number_frames as usize);
        let slice = std::slice::from_raw_parts_mut(buf.mData as *mut f32, n);
        // Channel i maps to input_buffers[i] (non-interleaved, mono per buffer).
        // For the common 2-buffer stereo case this gives L/R; buffers beyond the
        // available input channels are zeroed.
        if let Some(h) = host {
            if let Some(src) = h.input_buffers.get(i) {
                for (dst, &s) in slice.iter_mut().zip(src.iter()) {
                    *dst = s;
                }
                // Zero any remaining samples if src is shorter than n
                for dst in slice.iter_mut().skip(src.len()) {
                    *dst = 0.0;
                }
            } else {
                for s in slice.iter_mut() {
                    *s = 0.0;
                }
            }
        } else {
            for s in slice.iter_mut() {
                *s = 0.0;
            }
        }
    }
    0 // noErr
}

// ---- ObjC runtime helpers for GUI ----

extern "C" {
    fn objc_msgSend() -> *mut c_void;
}

unsafe fn objc_getClass(name: *const std::ffi::c_char) -> *mut c_void {
    extern "C" {
        fn objc_getClass(name: *const std::ffi::c_char) -> *mut c_void;
    }
    objc_getClass(name)
}

unsafe fn sel_registerName(name: *const std::ffi::c_char) -> *mut c_void {
    extern "C" {
        fn sel_registerName(name: *const std::ffi::c_char) -> *mut c_void;
    }
    sel_registerName(name)
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

unsafe fn objc_msg_send0(obj: *mut c_void, sel: *mut c_void) -> *mut c_void {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel)
}

unsafe fn objc_msg_send1(obj: *mut c_void, sel: *mut c_void, a: *mut c_void) -> *mut c_void {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, a)
}

unsafe fn objc_msg_send_void(obj: *mut c_void, sel: *mut c_void) {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel);
}

unsafe fn objc_msg_send_rect(obj: *mut c_void, sel: *mut c_void) -> NSRect {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> NSRect =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel)
}

unsafe fn objc_msg_send_uiview(
    obj: *mut c_void,
    sel: *mut c_void,
    au: *mut c_void,
    size: NSSize,
) -> *mut c_void {
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, NSSize) -> *mut c_void =
        std::mem::transmute(objc_msgSend as *const c_void);
    f(obj, sel, au, size)
}

// ---- Scanner support ----

/// Scan for AU plugins using AudioComponentFindNext.
pub fn scan_au_components() -> Vec<(AudioComponent, AudioComponentDescription, String)> {
    let mut results = Vec::new();

    unsafe {
        let mut component: AudioComponent = ptr::null_mut();
        loop {
            component = AudioComponentFindNext(component, ptr::null());
            if component.is_null() {
                break;
            }

            let mut desc = AudioComponentDescription {
                componentType: 0,
                componentSubType: 0,
                componentManufacturer: 0,
                componentFlags: 0,
                componentFlagsMask: 0,
            };
            let status = AudioComponentGetDescription(component, &mut desc);
            if status != 0 {
                continue;
            }

            // Only audio units (effects, instruments, generators, music devices)
            match desc.componentType {
                kAudioUnitType_Effect
                | kAudioUnitType_MusicEffect
                | kAudioUnitType_MusicDevice
                | kAudioUnitType_Generator
                | kAudioUnitType_MIDIProcessor => {}
                _ => continue,
            }

            let name = au_component_name(component);
            results.push((component, desc, name));
        }
    }

    results
}

/// Find a specific AU component by type/subtype/manufacturer.
/// Uses a specific descriptor with AudioComponentFindNext (works on macOS Sequoia).
pub fn find_au_by_desc(
    component_type: u32,
    component_subtype: u32,
    component_manufacturer: u32,
) -> Option<AudioComponent> {
    unsafe {
        let desc = AudioComponentDescription {
            componentType: component_type,
            componentSubType: component_subtype,
            componentManufacturer: component_manufacturer,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        let component = AudioComponentFindNext(ptr::null_mut(), &desc);
        if component.is_null() {
            None
        } else {
            Some(component)
        }
    }
}

pub fn au_component_name(component: AudioComponent) -> String {
    unsafe {
        let mut cf_name: CFStringRef = ptr::null();
        let status = AudioComponentCopyName(component, &mut cf_name);
        if status != 0 || cf_name.is_null() {
            return String::new();
        }
        let name = cfstring_to_string(cf_name);
        CFRelease(cf_name);
        name
    }
}

fn cfstring_to_string(cf_str: CFStringRef) -> String {
    unsafe {
        let len = CFStringGetLength(cf_str);
        let mut buf = vec![0u8; (len as usize) * 4 + 1];
        let ok = CFStringGetCString(
            cf_str,
            buf.as_mut_ptr() as *mut libc::c_char,
            buf.len() as CFIndex,
            kCFStringEncodingUTF8,
        );
        if ok {
            CStr::from_ptr(buf.as_ptr() as *const libc::c_char)
                .to_str()
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        }
    }
}

fn fourcc_to_string(fourcc: OSType) -> String {
    let bytes = [
        (fourcc >> 24) as u8,
        (fourcc >> 16) as u8,
        (fourcc >> 8) as u8,
        fourcc as u8,
    ];
    String::from_utf8_lossy(&bytes).to_string()
}
