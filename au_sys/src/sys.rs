#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]

use libc::{c_char, c_long, c_void};

pub type Boolean = u8;
pub type OSStatus = i32;
pub type OSType = u32;
pub type UInt8 = u8;
pub type SInt16 = i16;
pub type SInt32 = i32;
pub type UInt16 = u16;
pub type UInt32 = u32;
pub type UInt64 = u64;
pub type Float32 = f32;
pub type Float64 = f64;

pub type AudioUnitPropertyID = UInt32;
pub type AudioUnitParameterID = UInt32;
pub type AudioUnitScope = UInt32;
pub type AudioUnitElement = UInt32;
pub type AudioUnitParameterValue = Float32;

pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFAllocatorRef = *const c_void;
pub type CFTypeRef = *const c_void;
pub type CFNumberRef = *const c_void;
pub type CFBundleRef = *const c_void;
pub type CFURLRef = *const c_void;

pub type CFIndex = c_long;
pub type CFHashCode = u64;

#[repr(C)]
pub struct CFDictionaryKeyCallBacks {
    pub version: CFIndex,
    pub retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copyDescription: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
    pub hash: Option<unsafe extern "C" fn(*const c_void) -> CFHashCode>,
}

#[repr(C)]
pub struct CFDictionaryValueCallBacks {
    pub version: CFIndex,
    pub retain: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void) -> *const c_void>,
    pub release: Option<unsafe extern "C" fn(CFAllocatorRef, *const c_void)>,
    pub copyDescription: Option<unsafe extern "C" fn(*const c_void) -> CFStringRef>,
    pub equal: Option<unsafe extern "C" fn(*const c_void, *const c_void) -> Boolean>,
}

#[repr(C)]
pub struct AudioComponentDescription {
    pub componentType: OSType,
    pub componentSubType: OSType,
    pub componentManufacturer: OSType,
    pub componentFlags: UInt32,
    pub componentFlagsMask: UInt32,
}

#[repr(C)]
pub struct OpaqueAudioComponent {
    _private: [u8; 0],
}

#[repr(C)]
pub struct OpaqueAudioComponentInstance {
    _private: [u8; 0],
}

pub type AudioComponent = *mut OpaqueAudioComponent;

#[cfg(target_os = "macos")]
pub type AudioComponentInstance = *mut c_void;

pub type AudioUnit = AudioComponentInstance;

pub type AudioComponentMethod = *const c_void;

#[repr(C)]
pub struct AudioComponentPlugInInterface {
    pub Open: Option<unsafe extern "C" fn(*mut c_void, AudioComponentInstance) -> OSStatus>,
    pub Close: Option<unsafe extern "C" fn(*mut c_void) -> OSStatus>,
    pub Lookup: Option<unsafe extern "C" fn(SInt16) -> AudioComponentMethod>,
    pub reserved: *mut c_void,
}

pub type AudioComponentFactoryFunction = Option<
    unsafe extern "C" fn(*const AudioComponentDescription) -> *mut AudioComponentPlugInInterface,
>;

#[repr(C)]
pub struct AudioBuffer {
    pub mNumberChannels: UInt32,
    pub mDataByteSize: UInt32,
    pub mData: *mut c_void,
}

#[repr(C)]
pub struct AudioBufferList {
    pub mNumberBuffers: UInt32,
    pub mBuffers: [AudioBuffer; 1],
}

pub type AudioFormatID = UInt32;
pub type AudioFormatFlags = UInt32;

#[repr(C)]
pub struct AudioStreamBasicDescription {
    pub mSampleRate: Float64,
    pub mFormatID: AudioFormatID,
    pub mFormatFlags: AudioFormatFlags,
    pub mBytesPerPacket: UInt32,
    pub mFramesPerPacket: UInt32,
    pub mBytesPerFrame: UInt32,
    pub mChannelsPerFrame: UInt32,
    pub mBitsPerChannel: UInt32,
    pub mReserved: UInt32,
}

pub type SMPTETimeType = UInt32;
pub type SMPTETimeFlags = UInt32;

#[repr(C)]
pub struct SMPTETime {
    pub mSubframes: SInt16,
    pub mSubframeDivisor: SInt16,
    pub mCounter: UInt32,
    pub mType: SMPTETimeType,
    pub mFlags: SMPTETimeFlags,
    pub mHours: SInt16,
    pub mMinutes: SInt16,
    pub mSeconds: SInt16,
    pub mFrames: SInt16,
}

pub type AudioTimeStampFlags = UInt32;

#[repr(C)]
pub struct AudioTimeStamp {
    pub mSampleTime: Float64,
    pub mHostTime: UInt64,
    pub mRateScalar: Float64,
    pub mWordClockTime: UInt64,
    pub mSMPTETime: SMPTETime,
    pub mFlags: AudioTimeStampFlags,
    pub mReserved: UInt32,
}

pub type AudioUnitRenderActionFlags = UInt32;

pub type AudioUnitPropertyListenerProc = Option<
    unsafe extern "C" fn(
        *mut c_void,
        AudioUnit,
        AudioUnitPropertyID,
        AudioUnitScope,
        AudioUnitElement,
    ),
>;

pub type AURenderCallback = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *mut AudioUnitRenderActionFlags,
        *const AudioTimeStamp,
        UInt32,
        UInt32,
        *mut AudioBufferList,
    ) -> OSStatus,
>;

pub type HostCallback_GetBeatAndTempo =
    Option<unsafe extern "C" fn(*mut c_void, *mut Float64, *mut Float64) -> OSStatus>;

pub type HostCallback_GetMusicalTimeLocation = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *mut SInt32,
        *mut Float32,
        *mut SInt32,
        *mut Float64,
    ) -> OSStatus,
>;

pub type HostCallback_GetTransportState = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *mut Boolean,
        *mut Boolean,
        *mut Float64,
        *mut Boolean,
        *mut Float64,
        *mut Float64,
    ) -> OSStatus,
>;

pub type HostCallback_GetTransportState2 = Option<
    unsafe extern "C" fn(
        *mut c_void,
        *mut Boolean,
        *mut Boolean,
        *mut Boolean,
        *mut Float64,
        *mut Boolean,
        *mut Float64,
        *mut Float64,
    ) -> OSStatus,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostCallbackInfo {
    pub hostUserData: *mut c_void,
    pub beatAndTempoProc: HostCallback_GetBeatAndTempo,
    pub musicalTimeLocationProc: HostCallback_GetMusicalTimeLocation,
    pub transportStateProc: HostCallback_GetTransportState,
    pub transportStateProc2: HostCallback_GetTransportState2,
}

#[repr(C)]
pub struct AURenderCallbackStruct {
    pub inputProc: AURenderCallback,
    pub inputProcRefCon: *mut c_void,
}

#[repr(C)]
pub struct AUChannelInfo {
    pub inChannels: SInt16,
    pub outChannels: SInt16,
}

#[repr(C)]
pub struct AUPreset {
    pub presetNumber: SInt32,
    pub presetName: CFStringRef,
}

#[repr(C)]
pub struct AudioUnitConnection {
    pub sourceAudioUnit: AudioUnit,
    pub sourceOutputNumber: UInt32,
    pub destInputNumber: UInt32,
}

#[repr(C)]
pub struct AudioUnitCocoaViewInfo {
    pub mCocoaAUViewBundleLocation: CFURLRef,
    pub mCocoaAUViewClass: [CFStringRef; 1],
}

pub type AudioUnitParameterUnit = UInt32;
pub type AudioUnitParameterOptions = UInt32;

#[repr(C)]
pub struct AudioUnitParameterInfo {
    pub name: [c_char; 52],
    pub unitName: CFStringRef,
    pub clumpID: UInt32,
    pub cfNameString: CFStringRef,
    pub unit: AudioUnitParameterUnit,
    pub minValue: AudioUnitParameterValue,
    pub maxValue: AudioUnitParameterValue,
    pub defaultValue: AudioUnitParameterValue,
    pub flags: AudioUnitParameterOptions,
}

#[repr(C)]
pub struct MusicDeviceStdNoteParams {
    pub argCount: UInt32,
    pub mPitch: Float32,
    pub mVelocity: Float32,
}

pub type MusicDeviceGroupID = UInt32;
pub type NoteInstanceID = UInt32;

pub const kAudioUnitRange: SInt16 = 0x0000;
pub const kAudioUnitInitializeSelect: SInt16 = 0x0001;
pub const kAudioUnitUninitializeSelect: SInt16 = 0x0002;
pub const kAudioUnitGetPropertyInfoSelect: SInt16 = 0x0003;
pub const kAudioUnitGetPropertySelect: SInt16 = 0x0004;
pub const kAudioUnitSetPropertySelect: SInt16 = 0x0005;
pub const kAudioUnitAddPropertyListenerSelect: SInt16 = 0x000A;
pub const kAudioUnitRemovePropertyListenerSelect: SInt16 = 0x000B;
pub const kAudioUnitRemovePropertyListenerWithUserDataSelect: SInt16 = 0x0012;
pub const kAudioUnitAddRenderNotifySelect: SInt16 = 0x000F;
pub const kAudioUnitRemoveRenderNotifySelect: SInt16 = 0x0010;
pub const kAudioUnitGetParameterSelect: SInt16 = 0x0006;
pub const kAudioUnitSetParameterSelect: SInt16 = 0x0007;
pub const kAudioUnitRenderSelect: SInt16 = 0x000E;
pub const kAudioUnitResetSelect: SInt16 = 0x0009;

pub const kMusicDeviceRange: SInt16 = 0x0100;
pub const kMusicDeviceMIDIEventSelect: SInt16 = 0x0101;
pub const kMusicDeviceStartNoteSelect: SInt16 = 0x0105;
pub const kMusicDeviceStopNoteSelect: SInt16 = 0x0106;

pub const kAudioUnitScope_Global: AudioUnitScope = 0;
pub const kAudioUnitScope_Input: AudioUnitScope = 1;
pub const kAudioUnitScope_Output: AudioUnitScope = 2;

pub const kAudioUnitProperty_ClassInfo: AudioUnitPropertyID = 0;
pub const kAudioUnitProperty_MakeConnection: AudioUnitPropertyID = 1;
pub const kAudioUnitProperty_SampleRate: AudioUnitPropertyID = 2;
pub const kAudioUnitProperty_ParameterList: AudioUnitPropertyID = 3;
pub const kAudioUnitProperty_ParameterInfo: AudioUnitPropertyID = 4;
pub const kAudioUnitProperty_HostCallbacks: AudioUnitPropertyID = 27;
pub const kAudioUnitProperty_StreamFormat: AudioUnitPropertyID = 8;
pub const kAudioUnitProperty_ElementCount: AudioUnitPropertyID = 11;
pub const kAudioUnitProperty_Latency: AudioUnitPropertyID = 12;
pub const kAudioUnitProperty_SupportedNumChannels: AudioUnitPropertyID = 13;
pub const kAudioUnitProperty_MaximumFramesPerSlice: AudioUnitPropertyID = 14;
pub const kAudioUnitProperty_TailTime: AudioUnitPropertyID = 20;
pub const kAudioUnitProperty_BypassEffect: AudioUnitPropertyID = 21;
pub const kAudioUnitProperty_AuRsInstance: AudioUnitPropertyID = 0x61755253;
pub const kAudioUnitProperty_SetRenderCallback: AudioUnitPropertyID = 23;
pub const kAudioUnitProperty_InPlaceProcessing: AudioUnitPropertyID = 29;
pub const kAudioUnitProperty_CocoaUI: AudioUnitPropertyID = 31;
pub const kAudioUnitProperty_PresentPreset: AudioUnitPropertyID = 36;

pub const kAudioUnitParameterUnit_Generic: AudioUnitParameterUnit = 0;
pub const kAudioUnitParameterUnit_LinearGain: AudioUnitParameterUnit = 13;
pub const kAudioUnitParameterUnit_Indexed: AudioUnitParameterUnit = 6;

// AudioUnit component types
pub const kAudioUnitType_Effect: OSType = 0x61756678; // 'aufx'
pub const kAudioUnitType_MusicEffect: OSType = 0x61756D66; // 'aumf'
pub const kAudioUnitType_MusicDevice: OSType = 0x61756D75; // 'aumu'
pub const kAudioUnitType_Generator: OSType = 0x6175676E; // 'augn'
pub const kAudioUnitType_MIDIProcessor: OSType = 0x61756D69; // 'aumi'
pub const kAudioUnitParameterFlag_IsWritable: AudioUnitParameterOptions = 1 << 31;
pub const kAudioUnitParameterFlag_IsReadable: AudioUnitParameterOptions = 1 << 30;

pub const kAudioFormatLinearPCM: AudioFormatID = u32::from_be_bytes(*b"lpcm");

pub const kAudioFormatFlagIsFloat: AudioFormatFlags = 1 << 0;
pub const kAudioFormatFlagIsPacked: AudioFormatFlags = 1 << 3;
pub const kAudioFormatFlagIsNonInterleaved: AudioFormatFlags = 1 << 5;

pub const kAudioUnitRenderAction_OutputIsSilence: AudioUnitRenderActionFlags = 1 << 4;

pub const kAudioUnitErr_InvalidProperty: OSStatus = -10879;
pub const kAudioUnitErr_InvalidParameter: OSStatus = -10878;
pub const kAudioUnitErr_InvalidElement: OSStatus = -10877;
pub const kAudioUnitErr_NoConnection: OSStatus = -10876;
pub const kAudioUnitErr_FailedInitialization: OSStatus = -10875;
pub const kAudioUnitErr_TooManyFramesToProcess: OSStatus = -10874;
pub const kAudioUnitErr_FormatNotSupported: OSStatus = -10868;
pub const kAudioUnitErr_Uninitialized: OSStatus = -10867;
pub const kAudioUnitErr_InvalidScope: OSStatus = -10866;
pub const kAudioUnitErr_PropertyNotWritable: OSStatus = -10865;
pub const kAudioUnitErr_InvalidPropertyValue: OSStatus = -10851;
pub const kAudioUnitErr_Initialized: OSStatus = -10849;

pub const noErr: OSStatus = 0;

pub type Handle = *mut *mut c_void;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    pub fn GetComponentInstanceStorage(instance: AudioComponentInstance) -> Handle;
    pub fn SetComponentInstanceStorage(instance: AudioComponentInstance, storage: Handle);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub static kCFAllocatorDefault: CFAllocatorRef;
    pub static kCFTypeDictionaryKeyCallBacks: CFDictionaryKeyCallBacks;
    pub static kCFTypeDictionaryValueCallBacks: CFDictionaryValueCallBacks;
    pub fn CFStringCreateWithCString(
        allocator: CFAllocatorRef,
        c_str: *const c_char,
        encoding: u32,
    ) -> CFStringRef;
    pub fn CFDictionaryCreate(
        allocator: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        num_values: c_long,
        key_callbacks: *const CFDictionaryKeyCallBacks,
        value_callbacks: *const CFDictionaryValueCallBacks,
    ) -> CFDictionaryRef;
    pub fn CFNumberCreate(
        allocator: CFAllocatorRef,
        the_type: i32,
        value: *const c_void,
    ) -> CFNumberRef;
    pub fn CFBundleGetBundleWithIdentifier(bundle_id: CFStringRef) -> CFBundleRef;
    pub fn CFBundleCreate(allocator: CFAllocatorRef, url: CFURLRef) -> CFBundleRef;
    pub fn CFBundleCopyBundleURL(bundle: CFBundleRef) -> CFURLRef;
    pub fn CFURLCreateWithFileSystemPath(
        allocator: CFAllocatorRef,
        file_path: CFStringRef,
        path_style: i32,
        is_directory: bool,
    ) -> CFURLRef;
    pub fn CFRelease(obj: CFTypeRef);
    pub fn CFStringGetCString(
        the_string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> bool;
    pub fn CFStringGetLength(the_string: CFStringRef) -> CFIndex;
}

#[link(name = "AudioUnit", kind = "framework")]
unsafe extern "C" {
    pub fn AudioUnitGetProperty(
        in_unit: AudioUnit,
        in_id: AudioUnitPropertyID,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
        out_data: *mut c_void,
        io_data_size: *mut UInt32,
    ) -> OSStatus;
    pub fn AudioUnitGetParameter(
        in_unit: AudioUnit,
        in_id: AudioUnitParameterID,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
        out_value: *mut AudioUnitParameterValue,
    ) -> OSStatus;
    pub fn AudioUnitSetParameter(
        in_unit: AudioUnit,
        in_id: AudioUnitParameterID,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
        in_value: AudioUnitParameterValue,
        in_buffer_offset_in_frames: UInt32,
    ) -> OSStatus;
    pub fn AudioUnitSetProperty(
        in_unit: AudioUnit,
        in_id: AudioUnitPropertyID,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
        in_data: *const c_void,
        in_data_size: UInt32,
    ) -> OSStatus;
    pub fn AudioUnitGetPropertyInfo(
        in_unit: AudioUnit,
        in_id: AudioUnitPropertyID,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
        out_data_size: *mut UInt32,
        out_writable: *mut Boolean,
    ) -> OSStatus;
    pub fn AudioUnitInitialize(in_unit: AudioUnit) -> OSStatus;
    pub fn AudioUnitUninitialize(in_unit: AudioUnit) -> OSStatus;
    pub fn AudioUnitReset(
        in_unit: AudioUnit,
        in_scope: AudioUnitScope,
        in_element: AudioUnitElement,
    ) -> OSStatus;
    pub fn AudioUnitRender(
        in_unit: AudioUnit,
        io_action_flags: *mut UInt32,
        in_time_stamp: *const AudioTimeStamp,
        in_output_bus_number: UInt32,
        in_number_frames: UInt32,
        io_data: *mut AudioBufferList,
    ) -> OSStatus;
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    pub fn AudioComponentFindNext(
        in_component: AudioComponent,
        in_desc: *const AudioComponentDescription,
    ) -> AudioComponent;
    pub fn AudioComponentCopyName(
        in_component: AudioComponent,
        out_name: *mut CFStringRef,
    ) -> OSStatus;
    pub fn AudioComponentGetDescription(
        in_component: AudioComponent,
        out_desc: *mut AudioComponentDescription,
    ) -> OSStatus;
    pub fn AudioComponentInstanceNew(
        in_component: AudioComponent,
        out_instance: *mut AudioComponentInstance,
    ) -> OSStatus;
    pub fn AudioComponentInstanceDispose(in_instance: AudioComponentInstance) -> OSStatus;
}

pub const kCFStringEncodingUTF8: u32 = 0x08000100;
pub const kCFNumberSInt32Type: i32 = 3;
