//! Procedural macros for SunMao.
//!
//! This crate provides:
//! 1. `#[derive(Params)]` - Automatically implement `Params` trait
//! 2. `sunmao_export!` - Generate plugin entry points

extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{parse_macro_input, DeriveInput, Expr, Ident, Lit, Meta, Token, Type};

/// Derives the `Params` trait for a parameter struct. Add the helper attribute
/// `#[sunmao_au]` when the consuming crate also wants the AU parameter-list
/// implementation; keeping that opt-in explicit prevents AU dependencies from
/// entering CLAP/VST3-only artifacts.
///
/// Supported field types: `FloatParam`, `IntParam`, `BoolParam`.
///
/// Optional attributes:
/// - `#[id = "gain"]` overrides the parameter id (defaults to field name)
/// - `#[unit = "LinearGain"]` sets AU unit (defaults to `Generic`)
#[proc_macro_derive(Params, attributes(id, unit, name, nested, persist, sunmao_au))]
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

    let mut ids = Vec::new();
    let mut numeric_id_exprs = Vec::new();
    let mut get_arms = Vec::new();
    let mut set_arms = Vec::new();
    let mut descriptor_exprs = Vec::new();
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
                    if let Ok(nested) =
                        list.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                    {
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
        numeric_id_exprs.push(quote! { sunmao_core::params::stable_param_id(#id_lit) });

        match type_ident.as_str() {
            "FloatParam" => {
                get_arms.push(quote! { #id_lit => Some(self.#ident.get_normalized()) });
                set_arms.push(quote! { #id_lit => self.#ident.set_normalized(value) });
                descriptor_exprs.push(quote! {
                    sunmao_core::params::ParamDescriptor {
                        id: #id_lit,
                        numeric_id: sunmao_core::params::stable_param_id(#id_lit),
                        name: self.#ident.name,
                        default_normalized: {
                            let range = self.#ident.max - self.#ident.min;
                            if range.abs() <= f32::EPSILON {
                                0.0
                            } else {
                                ((self.#ident.default - self.#ident.min) / range).clamp(0.0, 1.0)
                            }
                        },
                        step_count: 0,
                        kind: sunmao_core::params::ParamKind::Float,
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
            "IntParam" => {
                get_arms.push(quote! { #id_lit => Some(self.#ident.get_normalized()) });
                set_arms.push(quote! { #id_lit => self.#ident.set_normalized(value) });
                descriptor_exprs.push(quote! {
                    sunmao_core::params::ParamDescriptor {
                        id: #id_lit,
                        numeric_id: sunmao_core::params::stable_param_id(#id_lit),
                        name: self.#ident.name,
                        default_normalized: {
                            let min = self.#ident.min as f32;
                            let max = self.#ident.max as f32;
                            if (max - min).abs() <= f32::EPSILON {
                                0.0
                            } else {
                                ((self.#ident.default as f32 - min) / (max - min)).clamp(0.0, 1.0)
                            }
                        },
                        step_count: (self.#ident.max as i64 - self.#ident.min as i64)
                            .clamp(0, u32::MAX as i64) as u32,
                        kind: sunmao_core::params::ParamKind::Int,
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
            "BoolParam" => {
                get_arms.push(quote! { #id_lit => Some(self.#ident.get_normalized()) });
                set_arms.push(quote! { #id_lit => self.#ident.set_normalized(value) });
                descriptor_exprs.push(quote! {
                    sunmao_core::params::ParamDescriptor {
                        id: #id_lit,
                        numeric_id: sunmao_core::params::stable_param_id(#id_lit),
                        name: self.#ident.name,
                        default_normalized: if self.#ident.default { 1.0 } else { 0.0 },
                        step_count: 1,
                        kind: sunmao_core::params::ParamKind::Bool,
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
        const _: () = {
            const NUMERIC_IDS: &[u32] = &[#(#numeric_id_exprs),*];
            let mut left = 0;
            while left < NUMERIC_IDS.len() {
                let mut right = left + 1;
                while right < NUMERIC_IDS.len() {
                    if NUMERIC_IDS[left] == NUMERIC_IDS[right] {
                        panic!("Params contains duplicate or colliding numeric parameter IDs");
                    }
                    right += 1;
                }
                left += 1;
            }
        };

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

            fn descriptors(&self) -> Vec<sunmao_core::params::ParamDescriptor> {
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

    let expanded = if with_gui {
        quote! {
            ::sunmao::backend_vst3::export_vst3_plugin_with_gui!(
                ::sunmao::backend_vst3::SunmaoVst3Wrapper<#plugin_type>
            );
            ::sunmao::backend_clap::export_sunmao_clap_plugin_with_gui!(#plugin_type);
        }
    } else {
        quote! {
            ::sunmao::backend_vst3::export_vst3_plugin!(
                ::sunmao::backend_vst3::SunmaoVst3Wrapper<#plugin_type>
            );
            ::sunmao::backend_clap::export_sunmao_clap_plugin!(#plugin_type);
        }
    };

    TokenStream::from(expanded)
}
