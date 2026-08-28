//! Procedural macros for SunMao.
//!
//! This crate provides:
//! 1. `#[derive(Params)]` - Automatically implement `Params` trait
//! 2. `sunmao_export!` - Generate plugin entry points

extern crate proc_macro;
use proc_macro::TokenStream;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_macro_input, DeriveInput, Expr, Ident, Lit, Meta, Token, Type};

fn core_crate_path() -> proc_macro2::TokenStream {
    if let Ok(found) = crate_name("sunmao_core") {
        return match found {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let ident = Ident::new(&name, proc_macro2::Span::call_site());
                quote!(::#ident)
            }
        };
    }

    if let Ok(FoundCrate::Name(name)) = crate_name("sunmao") {
        let ident = Ident::new(&name, proc_macro2::Span::call_site());
        return quote!(::#ident::__private::sunmao_core);
    }

    // Preserve the historical diagnostic for callers that have not declared
    // either supported dependency; the generated code then points directly at
    // the missing crate and rustc reports the actionable dependency error.
    quote!(::sunmao_core)
}

/// Resolve the facade crate under the dependency name chosen by the caller.
///
/// `sunmao_export!` is normally invoked through the `sunmao` facade, but
/// Cargo permits dependencies to be renamed (for example, `sm = { package =
/// "sunmao", ... }`).  Hard-coding `::sunmao` would make an otherwise valid
/// plugin fail to compile in that common workspace setup.
fn facade_crate_path() -> proc_macro2::TokenStream {
    if let Ok(found) = crate_name("sunmao") {
        return match found {
            FoundCrate::Itself => quote!(crate),
            FoundCrate::Name(name) => {
                let ident = Ident::new(&name, proc_macro2::Span::call_site());
                quote!(::#ident)
            }
        };
    }

    // Keep the old diagnostic for callers that did not declare the facade;
    // rustc will point at the missing dependency in the generated expansion.
    quote!(::sunmao)
}

/// Derives the `Params` trait for a parameter struct. Add the helper attribute
/// `#[sunmao_au]` when the consuming crate also wants the AU parameter-list
/// implementation; keeping that opt-in explicit prevents AU dependencies from
/// entering CLAP/VST3-only artifacts.
///
/// Supported field types: `FloatParam`, `IntParam`, `BoolParam`.
///
/// The stable parameter ID comes from the `FloatParam`/`IntParam`/`BoolParam`
/// constructor. This keeps host automation, DSP event matching, and GUI
/// binding on one source of truth even when the Rust field is renamed.
/// `#[unit = "LinearGain"]` sets the optional AU unit (defaults to `Generic`).
#[proc_macro_derive(Params, attributes(id, unit, param, name, nested, persist, sunmao_au))]
pub fn derive_params(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generate_au = input
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("sunmao_au"));
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

    let mut get_branches = Vec::new();
    let mut set_branches = Vec::new();
    let mut descriptor_exprs = Vec::new();
    let mut au_param_exprs = Vec::new();
    let core = core_crate_path();

    for (index, field) in fields.iter().enumerate() {
        let ident = field.ident.as_ref().unwrap();
        let type_ident = match &field.ty {
            Type::Path(path) if path.qself.is_none() => path.path.segments.last().map(|s| &s.ident),
            _ => None,
        };

        let mut unit_value: Option<String> = None;

        for attr in &field.attrs {
            match &attr.meta {
                Meta::NameValue(meta) if meta.path.is_ident("id") => {
                    return syn::Error::new_spanned(
                        attr,
                        "parameter IDs are declared in the parameter constructor; remove #[id]",
                    )
                    .to_compile_error()
                    .into();
                }
                Meta::NameValue(meta) if meta.path.is_ident("unit") => {
                    if let Expr::Lit(expr) = &meta.value {
                        if let Lit::Str(lit) = &expr.lit {
                            unit_value = Some(lit.value());
                        }
                    }
                }
                Meta::List(list) if list.path.is_ident("param") => {
                    if let Ok(nested) =
                        list.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                    {
                        for meta in nested {
                            if let Meta::NameValue(nv) = meta {
                                if nv.path.is_ident("id") {
                                    return syn::Error::new_spanned(
                                        nv,
                                        "parameter IDs are declared in the parameter constructor; remove `id = ...`",
                                    )
                                    .to_compile_error()
                                    .into();
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

        match type_ident.map(|ident| ident.to_string()).as_deref() {
            Some("FloatParam") => {
                get_branches.push(quote! {
                    if id == self.#ident.id {
                        return Some(self.#ident.get_normalized());
                    }
                });
                set_branches.push(quote! {
                    if id == self.#ident.id {
                        self.#ident.set_normalized(value);
                        return;
                    }
                });
                descriptor_exprs.push(quote! {
                    #core::params::ParamDescriptor {
                        id: self.#ident.id,
                        numeric_id: #core::params::stable_param_id(self.#ident.id),
                        name: self.#ident.name,
                        default_normalized: {
                            let range = self.#ident.max as f64 - self.#ident.min as f64;
                            if range == 0.0 {
                                0.0
                            } else {
                                ((self.#ident.default as f64 - self.#ident.min as f64) / range)
                                    .clamp(0.0, 1.0) as f32
                            }
                        },
                        step_count: 0,
                        kind: #core::params::ParamKind::Float,
                    }
                });
                let unit_ident =
                    syn::Ident::new(unit_value.as_deref().unwrap_or("Generic"), ident.span());
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
            Some("IntParam") => {
                get_branches.push(quote! {
                    if id == self.#ident.id {
                        return Some(self.#ident.get_normalized());
                    }
                });
                set_branches.push(quote! {
                    if id == self.#ident.id {
                        self.#ident.set_normalized(value);
                        return;
                    }
                });
                descriptor_exprs.push(quote! {
                    #core::params::ParamDescriptor {
                        id: self.#ident.id,
                        numeric_id: #core::params::stable_param_id(self.#ident.id),
                        name: self.#ident.name,
                        default_normalized: {
                            let min = self.#ident.min as f64;
                            let max = self.#ident.max as f64;
                            let range = max - min;
                            if range == 0.0 {
                                0.0
                            } else {
                                ((self.#ident.default as f64 - min) / range).clamp(0.0, 1.0)
                                    as f32
                            }
                        },
                        step_count: (self.#ident.max as i64 - self.#ident.min as i64)
                            .clamp(0, u32::MAX as i64) as u32,
                        kind: #core::params::ParamKind::Int,
                    }
                });
                let unit_ident =
                    syn::Ident::new(unit_value.as_deref().unwrap_or("Generic"), ident.span());
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
            Some("BoolParam") => {
                get_branches.push(quote! {
                    if id == self.#ident.id {
                        return Some(self.#ident.get_normalized());
                    }
                });
                set_branches.push(quote! {
                    if id == self.#ident.id {
                        self.#ident.set_normalized(value);
                        return;
                    }
                });
                descriptor_exprs.push(quote! {
                    #core::params::ParamDescriptor {
                        id: self.#ident.id,
                        numeric_id: #core::params::stable_param_id(self.#ident.id),
                        name: self.#ident.name,
                        default_normalized: if self.#ident.default { 1.0 } else { 0.0 },
                        step_count: 1,
                        kind: #core::params::ParamKind::Bool,
                    }
                });
                let unit_ident =
                    syn::Ident::new(unit_value.as_deref().unwrap_or("Generic"), ident.span());
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

    let au_impl = if generate_au {
        quote! {
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
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #core::params::Params for #name {
            fn get_normalized(&self, id: &str) -> Option<f32> {
                #(#get_branches)*
                None
            }

            fn set_normalized(&self, id: &str, value: f32) {
                #(#set_branches)*
            }

            fn descriptors(&self) -> Vec<#core::params::ParamDescriptor> {
                vec![#(#descriptor_exprs),*]
            }
        }

        #au_impl
    };

    TokenStream::from(expanded)
}

/// Generates VST3 and CLAP entry points from one plugin type.
///
/// Usage:
/// ```ignore
/// sunmao_export!(MyPlugin);
/// sunmao_export!(MyGuiPlugin, gui);
/// ```
struct ExportInput {
    plugin_type: Type,
    with_gui: bool,
}

impl Parse for ExportInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let plugin_type = input.parse()?;
        let with_gui = if input.is_empty() {
            false
        } else {
            input.parse::<Token![,]>()?;
            let option: Ident = input.parse()?;
            if option != "gui" {
                return Err(syn::Error::new(option.span(), "expected `gui`"));
            }
            true
        };
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after export option"));
        }
        Ok(Self {
            plugin_type,
            with_gui,
        })
    }
}

#[proc_macro]
pub fn sunmao_export(input: TokenStream) -> TokenStream {
    let ExportInput {
        plugin_type,
        with_gui,
    } = parse_macro_input!(input as ExportInput);

    let facade = facade_crate_path();
    let expanded = if with_gui {
        quote! {
            #facade::backend_vst3::export_vst3_plugin_with_gui!(
                #facade::backend_vst3::SunmaoVst3Wrapper<#plugin_type>
            );
            #facade::backend_clap::export_sunmao_clap_plugin_with_gui!(#plugin_type);
        }
    } else {
        quote! {
            #facade::backend_vst3::export_vst3_plugin!(
                #facade::backend_vst3::SunmaoVst3Wrapper<#plugin_type>
            );
            #facade::backend_clap::export_sunmao_clap_plugin!(#plugin_type);
        }
    };

    TokenStream::from(expanded)
}
