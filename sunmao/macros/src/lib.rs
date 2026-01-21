//! Procedural macros for SunMao.
//!
//! This crate provides:
//! 1. `#[derive(Params)]` - Automatically implement `Params` trait
//! 2. `sunmao_export!` - Generate plugin entry points

extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, DeriveInput, Expr, Ident, Lit, Meta};
use syn::punctuated::Punctuated;
use syn::token::Comma;

/// Derives the `Params` trait for a parameter struct and (on macOS) generates AU param metadata.
///
/// Supported field types: `FloatParam`, `IntParam`, `BoolParam`.
///
/// Optional attributes:
/// - `#[id = "gain"]` overrides the parameter id (defaults to field name)
/// - `#[unit = "LinearGain"]` sets AU unit (defaults to `Generic`)
#[proc_macro_derive(Params, attributes(id, unit, name, nested, persist))]
pub fn derive_params(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let data = match &input.data {
        syn::Data::Struct(data) => data,
        _ => {
            return syn::Error::new_spanned(&input, "Params can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let fields = match &data.fields {
        syn::Fields::Named(fields) => &fields.named,
        _ => {
            return syn::Error::new_spanned(&input, "Params requires named fields")
                .to_compile_error()
                .into();
        }
    };

    let mut ids = Vec::new();
    let mut get_arms = Vec::new();
    let mut set_arms = Vec::new();
    let mut au_param_exprs = Vec::new();

    for (index, field) in fields.iter().enumerate() {
        let ident = field.ident.as_ref().unwrap();
        let field_ty = field.ty.to_token_stream().to_string();
        let type_ident = field_ty.split("::").last().unwrap_or("").to_string();

        let mut id_value: Option<String> = None;
        let mut unit_value: Option<String> = None;

        for attr in &field.attrs {
            match &attr.meta {
                Meta::NameValue(meta) if meta.path.is_ident("id") => {
                    if let Expr::Lit(expr) = &meta.value {
                        if let Lit::Str(lit) = &expr.lit {
                            id_value = Some(lit.value());
                        }
                    }
                }
                Meta::NameValue(meta) if meta.path.is_ident("unit") => {
                    if let Expr::Lit(expr) = &meta.value {
                        if let Lit::Str(lit) = &expr.lit {
                            unit_value = Some(lit.value());
                        }
                    }
                }
                Meta::List(list) if list.path.is_ident("param") => {
                    if let Ok(nested) = list.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated) {
                        for meta in nested {
                            if let Meta::NameValue(nv) = meta {
                                if nv.path.is_ident("id") {
                                    if let Expr::Lit(expr) = &nv.value {
                                        if let Lit::Str(lit) = &expr.lit {
                                            id_value = Some(lit.value());
                                        }
                                    }
                                }
                                if nv.path.is_ident("unit") {
                                    if let Expr::Lit(expr) = &nv.value {
                                        if let Lit::Str(lit) = &expr.lit {
                                            unit_value = Some(lit.value());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let id_str = id_value.unwrap_or_else(|| ident.to_string());
        let id_lit = syn::LitStr::new(&id_str, ident.span());
        ids.push(quote! { #id_lit });

        match type_ident.as_str() {
            "FloatParam" => {
                get_arms.push(quote! { #id_lit => Some(self.#ident.get_normalized()) });
                set_arms.push(quote! { #id_lit => self.#ident.set_normalized(value) });
                let unit_ident = syn::Ident::new(
                    unit_value.as_deref().unwrap_or("Generic"),
                    ident.span(),
                );
                let index_lit = index as u32;
                au_param_exprs.push(quote! {
                    sunmao_backend_au::ParameterInfo {
                        id: #index_lit,
                        name: params.#ident.name,
                        min: params.#ident.min,
                        max: params.#ident.max,
                        default: params.#ident.default,
                        unit: sunmao_backend_au::ParameterUnit::#unit_ident,
                    }
                });
            }
            "IntParam" => {
                get_arms.push(quote! {
                    #id_lit => {
                        let min = self.#ident.min as f32;
                        let max = self.#ident.max as f32;
                        if (max - min).abs() <= f32::EPSILON {
                            Some(0.0)
                        } else {
                            Some((self.#ident.get() as f32 - min) / (max - min))
                        }
                    }
                });
                set_arms.push(quote! {
                    #id_lit => {
                        let min = self.#ident.min as f32;
                        let max = self.#ident.max as f32;
                        let scaled = min + value.clamp(0.0, 1.0) * (max - min);
                        self.#ident.set(scaled.round() as i32);
                    }
                });
                let unit_ident = syn::Ident::new(
                    unit_value.as_deref().unwrap_or("Generic"),
                    ident.span(),
                );
                let index_lit = index as u32;
                au_param_exprs.push(quote! {
                    sunmao_backend_au::ParameterInfo {
                        id: #index_lit,
                        name: params.#ident.name,
                        min: params.#ident.min as f32,
                        max: params.#ident.max as f32,
                        default: params.#ident.default as f32,
                        unit: sunmao_backend_au::ParameterUnit::#unit_ident,
                    }
                });
            }
            "BoolParam" => {
                get_arms.push(quote! { #id_lit => Some(if self.#ident.get() { 1.0 } else { 0.0 }) });
                set_arms.push(quote! { #id_lit => self.#ident.set(value >= 0.5) });
                let unit_ident = syn::Ident::new(
                    unit_value.as_deref().unwrap_or("Generic"),
                    ident.span(),
                );
                let index_lit = index as u32;
                au_param_exprs.push(quote! {
                    sunmao_backend_au::ParameterInfo {
                        id: #index_lit,
                        name: params.#ident.name,
                        min: 0.0,
                        max: 1.0,
                        default: if params.#ident.default { 1.0 } else { 0.0 },
                        unit: sunmao_backend_au::ParameterUnit::#unit_ident,
                    }
                });
            }
            _ => {
                return syn::Error::new_spanned(
                    &field.ty,
                    "Unsupported parameter type for #[derive(Params)]",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    let expanded = quote! {
        impl sunmao_core::params::Params for #name {
            fn ids() -> &'static [&'static str] {
                static IDS: &[&str] = &[#(#ids),*];
                IDS
            }

            fn get_normalized(&self, id: &str) -> Option<f32> {
                match id {
                    #(#get_arms,)*
                    _ => None,
                }
            }

            fn set_normalized(&self, id: &str, value: f32) {
                match id {
                    #(#set_arms,)*
                    _ => {}
                }
            }
        }

        #[cfg(target_os = "macos")]
        impl sunmao_backend_au::SunmaoAuParamList for #name {
            fn au_params() -> &'static [sunmao_backend_au::ParameterInfo] {
                use std::sync::OnceLock;
                static PARAMS: OnceLock<Vec<sunmao_backend_au::ParameterInfo>> = OnceLock::new();
                PARAMS.get_or_init(|| {
                    let params = #name::default();
                    vec![#(#au_param_exprs),*]
                }).as_slice()
            }
        }
    };

    TokenStream::from(expanded)
}

/// Generates plugin entry points for the specified formats.
///
/// Usage:
/// ```ignore
/// sunmao_export!(MyPlugin);
/// // Or specify formats:
/// sunmao_export!(MyPlugin, Vst3, Clap);
/// ```
#[proc_macro]
pub fn sunmao_export(input: TokenStream) -> TokenStream {
    let plugin_name = parse_macro_input!(input as Ident);

    // Generate entry points for all formats
    let expanded = quote! {
        // VST3 entry point
        #[cfg(feature = "vst3")]
        #[no_mangle]
        pub extern "system" fn GetPluginFactory() -> *mut ::std::ffi::c_void {
            sunmao_backend_vst3::create_factory::<#plugin_name>()
        }

        // CLAP entry point
        #[cfg(feature = "clap")]
        #[no_mangle]
        pub static clap_entry: sunmao_backend_clap::ClapEntry = 
            sunmao_backend_clap::make_entry::<#plugin_name>();

        // AU entry point (macOS only)
        #[cfg(all(feature = "au", target_os = "macos"))]
        #[no_mangle]
        pub extern "C" fn RustAUFactory(
            desc: *mut ::std::ffi::c_void
        ) -> *mut ::std::ffi::c_void {
            sunmao_backend_au::create_instance::<#plugin_name>(desc)
        }

        // Standalone main
        #[cfg(feature = "standalone")]
        fn main() -> Result<(), Box<dyn std::error::Error>> {
            sunmao_runtime::run_standalone::<#plugin_name>()
        }
    };

    TokenStream::from(expanded)
}
