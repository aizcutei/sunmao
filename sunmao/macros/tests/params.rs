use sunmao_core::{
    stable_param_id, BoolParam, FloatParam, IntParam, ParamKind, Params as ParamsTrait,
};
use sunmao_macros::Params;

#[derive(Params)]
struct AllParamTypes {
    mix: FloatParam,
    voices: IntParam,
    bypass: BoolParam,
}

#[derive(Params)]
struct ParamAttributeSyntax {
    #[param(unit = "Generic")]
    value: FloatParam,
}

#[derive(Params)]
struct FullyQualifiedParamType {
    gain: sunmao_core::params::FloatParam,
}

impl Default for FullyQualifiedParamType {
    fn default() -> Self {
        Self {
            gain: sunmao_core::params::FloatParam::new("gain", "Gain", 0.5, 0.0, 1.0),
        }
    }
}

impl Default for ParamAttributeSyntax {
    fn default() -> Self {
        Self {
            value: FloatParam::new("renamed", "Value", 0.25, 0.0, 1.0),
        }
    }
}

impl Default for AllParamTypes {
    fn default() -> Self {
        Self {
            mix: FloatParam::new("mix", "Dry/Wet", 0.25, 0.0, 1.0),
            voices: IntParam::new("voices", "Voices", 3, 1, 5),
            bypass: BoolParam::new("bypass", "Bypass", false),
        }
    }
}

#[test]
fn derive_describes_float_int_and_bool_parameters() {
    let params = AllParamTypes::default();
    let descriptors = params.descriptors();

    assert_eq!(descriptors.len(), 3);
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>(),
        ["mix", "voices", "bypass"]
    );

    assert_eq!(descriptors[0].name, "Dry/Wet");
    assert_eq!(descriptors[0].numeric_id, stable_param_id("mix"));
    assert_eq!(descriptors[0].default_normalized, 0.25);
    assert_eq!(descriptors[0].step_count, 0);
    assert_eq!(descriptors[0].kind, ParamKind::Float);

    assert_eq!(descriptors[1].name, "Voices");
    assert_eq!(descriptors[1].numeric_id, stable_param_id("voices"));
    assert_eq!(descriptors[1].default_normalized, 0.5);
    assert_eq!(descriptors[1].step_count, 4);
    assert_eq!(descriptors[1].kind, ParamKind::Int);

    assert_eq!(descriptors[2].name, "Bypass");
    assert_eq!(descriptors[2].numeric_id, stable_param_id("bypass"));
    assert_eq!(descriptors[2].default_normalized, 0.0);
    assert_eq!(descriptors[2].step_count, 1);
    assert_eq!(descriptors[2].kind, ParamKind::Bool);
}

#[test]
fn derive_clamps_normalized_writes_for_discrete_parameters() {
    let params = AllParamTypes::default();

    params.set_normalized("voices", 2.0);
    params.set_normalized("bypass", 0.75);

    assert_eq!(params.voices.get(), 5);
    assert!(params.bypass.get());
}

#[test]
fn derive_accepts_param_namespace_attribute() {
    let params = ParamAttributeSyntax::default();
    assert_eq!(params.get_normalized("renamed"), Some(0.25));
    assert_eq!(params.descriptors()[0].id, "renamed");
}

#[test]
fn derive_accepts_fully_qualified_parameter_types() {
    let params = FullyQualifiedParamType::default();
    assert_eq!(params.get_normalized("gain"), Some(0.5));
}
