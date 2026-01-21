# SunMao

![logo](./assets/sunmao.png)

SunMao(榫卯) is an audio plug-in framework in Rust. It provide a unified interface for AudioUnit, CLAP, and VST3.

This framework is based on bindings and wrapper of CLAP, AudioUnit, and VST3. 

## Structure of the framework

[baseview](./baseview) : a manual fork of [RustAudio/baseview](https://github.com/RustAudio/baseview)

[clap_rs](./clap_rs) : safe wrapper for clap_sys

[clap_sys](./clap_sys) : bindings for clap

[au_rs](./au_rs) : safe wrapper for au_sys

[au_sys](./au_sys) : bindings for AudioUnit v2

[vst3_rs](./vst3_rs) : safe wrapper for vst3_sys

[vst3_sys](./vst3_sys) : bindings for VST3

[examples](./examples) : examples for the framework

## Current Status

| Platform | VST3 | CLAP | AudioUnit | Sunmao |
|----------|------|------|-----------|--------|
| macOS    |🟡|🟡|🟡|🔵|
| Windows  |🟡|🟡|➖|🔵|
| Linux    |🟡|🟡|➖|🔵|

➖ does not exist

🌑 not started

🔵 in progress

🟡 partially available

🏁 completed but not tested

🟢 completed and tested

## Instructions

TODO！

## Alternatives

This work is heavily inspired by the great work of [clap-sys](https://github.com/micahrj/clap-sys), [clack](https://github.com/prokopyl/clack), [vst3-sys](https://github.com/RustAudio/vst3-sys), [baseview](https://github.com/RustAudio/baseview), [nih-plug](https://github.com/robbert-vdh/nih-plug) and so on, check them out if you need a more complete solution!


## License
[AudioUnit SDK](https://github.com/apple/AudioUnitSDK) original license is Apache-2.0. AudioUnit is a trademark of Apple Inc.

[VST3 SDK](https://www.steinberg.net/en/company/developer.html) original license is MIT. VST is a trademark of Steinberg Media Technologies GmbH.

[CLAP](https://github.com/free-audio/clap) original license is MIT.

SunMao is licensed under the MIT License and Apache-2.0.