#![allow(linker_messages)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Attribute, DeriveInput, Fields, Ident, LitStr, Type};

/// Derives `QObject` and generates a static `MetaObject` reflection descriptor (`QMetaObject`).
///
/// Automatically creates:
/// - `fn object_data(&self) -> &ObjectData`
/// - `fn object_data_mut(&mut self) -> &mut ObjectData`
/// - `fn as_qobject_any(&self) -> Option<&dyn Any>`
/// - `fn as_qobject_any_mut(&mut self) -> Option<&mut dyn Any>`
/// - `fn meta_object(&self) -> &'static MetaObject`
///
/// Supports field-level `#[property]` and `#[signal]` attributes:
/// ```ignore
/// #[derive(QObject)]
/// #[qobject(class_name = "MyButton")]
/// struct MyButton {
///     data: ObjectData,
///     #[property]
///     text: String,
///     #[property(readonly)]
///     count: i32,
///     #[signal]
///     clicked: Signal<()>,
/// }
/// ```
/// Signal fields contribute signature metadata; they are not dynamically invokable.
#[proc_macro_derive(QObject, attributes(qobject, property, signal))]
pub fn derive_qobject(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    if !input.generics.params.is_empty() {
        return syn::Error::new_spanned(
            &input.generics,
            "`#[derive(QObject)]` does not support generic structs",
        )
        .to_compile_error()
        .into();
    }
    if !matches!(
        &input.data,
        syn::Data::Struct(data) if matches!(&data.fields, Fields::Named(_))
    ) {
        return syn::Error::new_spanned(
            &input,
            "`#[derive(QObject)]` requires a struct with named fields",
        )
        .to_compile_error()
        .into();
    }
    let name = &input.ident;
    let name_str = name.to_string();

    // Check for class_name override in #[qobject(class_name = "...")]
    let mut class_name = name_str.clone();
    for attr in &input.attrs {
        if attr.path().is_ident("qobject") {
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("class_name") {
                    let s: LitStr = meta.value()?.parse()?;
                    class_name = s.value();
                }
                Ok(())
            });
        }
    }

    // Inspect fields for ObjectData and #[property]
    let mut data_field_ident: Option<Ident> = None;
    let mut properties = Vec::new();

    let mut signals = Vec::new();

    if let syn::Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields_named) = &data_struct.fields {
            for field in &fields_named.named {
                let f_ident = field.ident.clone().unwrap();
                let f_type = &field.ty;

                // Identify the ObjectData field (named `data` or with type ending in `ObjectData`)
                if f_ident == "data" || is_object_data_type(f_type) {
                    data_field_ident = Some(f_ident.clone());
                }

                // Check for #[property] attribute
                for attr in &field.attrs {
                    if attr.path().is_ident("property") {
                        let is_readonly = check_property_readonly(attr);
                        let prop_name = f_ident.to_string();
                        properties.push((f_ident.clone(), f_type.clone(), prop_name, is_readonly));
                    }
                }

                for attr in &field.attrs {
                    if attr.path().is_ident("signal") {
                        if let Some(payload_types) = signal_payload_types(f_type) {
                            signals.push((f_ident.clone(), payload_types));
                        } else {
                            return syn::Error::new_spanned(
                                f_type,
                                "`#[signal]` fields must have type `Signal<T>`",
                            )
                            .to_compile_error()
                            .into();
                        }
                    }
                }
            }
        }
    }
    if data_field_ident.is_none() {
        return syn::Error::new_spanned(
            name,
            "`#[derive(QObject)]` requires a field named `data` or a field of type `ObjectData`",
        )
        .to_compile_error()
        .into();
    }

    let (data_access, data_access_mut) = if let Some(field) = &data_field_ident {
        (quote! { &self.#field }, quote! { &mut self.#field })
    } else {
        (quote! { &self.data }, quote! { &mut self.data })
    };

    // Generate MetaProperty items
    let mut meta_properties = Vec::new();
    let meta_obj_ident = Ident::new(&format!("__{}_META_OBJECT", name), name.span());

    for (f_ident, f_type, _prop_name, is_readonly) in &properties {
        let is_writable = !*is_readonly;
        let getter_fn_ident = Ident::new(&format!("__qtrs_get_{}_{}", name, f_ident), name.span());
        let setter_fn_ident = Ident::new(&format!("__qtrs_set_{}_{}", name, f_ident), name.span());

        let (getter_impl, setter_impl) = generate_property_accessors(
            name,
            f_ident,
            f_type,
            &getter_fn_ident,
            &setter_fn_ident,
            is_writable,
        );

        meta_properties.push(quote! {
            #getter_impl
            #setter_impl
        });
    }

    let prop_descriptors: Vec<_> = properties
        .iter()
        .map(|(f_ident, f_type, prop_name, is_readonly)| {
            let is_writable = !*is_readonly;
            let getter_fn_ident =
                Ident::new(&format!("__qtrs_get_{}_{}", name, f_ident), name.span());
            let setter_fn_ident =
                Ident::new(&format!("__qtrs_set_{}_{}", name, f_ident), name.span());

            let setter_ref = if is_writable {
                quote! { Some(#setter_fn_ident) }
            } else {
                quote! { None }
            };

            let type_name_str = quote!(#f_type).to_string();

            quote! {
                ::qtrs_core::meta::MetaProperty::new(
                    #prop_name,
                    #type_name_str,
                    true,
                    #is_writable,
                    false,
                    false,
                    None,
                    Some(#getter_fn_ident),
                    #setter_ref,
                )
            }
        })
        .collect();

    let meta_props_slice = quote! {
        &[ #(#prop_descriptors),* ]
    };

    let signal_descriptors: Vec<_> = signals
        .iter()
        .map(|(field, payload_types)| {
            let name = field.to_string();
            let parameter_types: Vec<_> = payload_types
                .iter()
                .map(|ty| LitStr::new(&quote!(#ty).to_string().replace(' ', ""), field.span()))
                .collect();
            let signature = format!(
                "{}({})",
                name,
                parameter_types
                    .iter()
                    .map(|ty| ty.value())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let signature = LitStr::new(&signature, field.span());
            quote! {
                ::qtrs_core::meta::MetaMethod::new(
                    #name,
                    #signature,
                    "()",
                    &[#(#parameter_types),*],
                    &[],
                    ::qtrs_core::meta::MethodType::Signal,
                    ::qtrs_core::meta::Access::Public,
                    None,
                )
            }
        })
        .collect();

    let expanded = quote! {
        #(#meta_properties)*

        #[allow(non_upper_case_globals)]
        static #meta_obj_ident: ::qtrs_core::meta::MetaObject = ::qtrs_core::meta::MetaObject::new(
            #class_name,
            Some(&::qtrs_core::meta::QOBJECT_META_OBJECT),
            &[ #(#signal_descriptors),* ],
            #meta_props_slice,
            &[],
            &[],
        );

        impl ::qtrs_core::object::QObject for #name {
            fn object_data(&self) -> &::qtrs_core::object::ObjectData {
                #data_access
            }

            fn object_data_mut(&mut self) -> &mut ::qtrs_core::object::ObjectData {
                #data_access_mut
            }

            fn as_qobject_any(&self) -> Option<&dyn ::std::any::Any> {
                Some(self)
            }

            fn as_qobject_any_mut(&mut self) -> Option<&mut dyn ::std::any::Any> {
                Some(self)
            }

            fn meta_object(&self) -> &'static ::qtrs_core::meta::MetaObject {
                &#meta_obj_ident
            }
        }
    };

    TokenStream::from(expanded)
}

fn signal_payload_types(ty: &Type) -> Option<Vec<Type>> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Signal" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    let mut generic_args = args.args.iter();
    let Some(syn::GenericArgument::Type(payload)) = generic_args.next() else {
        return None;
    };
    if generic_args.next().is_some() {
        return None;
    }
    match payload {
        Type::Tuple(tuple) => Some(tuple.elems.iter().cloned().collect()),
        other => Some(vec![other.clone()]),
    }
}

fn is_object_data_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "ObjectData";
        }
    }
    false
}

fn check_property_readonly(attr: &Attribute) -> bool {
    let mut readonly = false;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("readonly") {
            readonly = true;
        }
        Ok(())
    });
    readonly
}

fn generate_property_accessors(
    struct_name: &Ident,
    field_ident: &Ident,
    field_type: &Type,
    getter_ident: &Ident,
    setter_ident: &Ident,
    is_writable: bool,
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    let getter = quote! {
        #[allow(non_snake_case)]
        fn #getter_ident(obj: &dyn ::std::any::Any) -> ::qtrs_core::variant::Variant {
            if let Some(s) = obj.downcast_ref::<#struct_name>() {
                ::qtrs_core::variant::Variant::from(s.#field_ident.clone())
            } else {
                ::qtrs_core::variant::Variant::Invalid
            }
        }
    };

    let setter = if is_writable {
        quote! {
            #[allow(non_snake_case)]
            fn #setter_ident(
                obj: &mut dyn ::std::any::Any,
                val: ::qtrs_core::variant::Variant,
            ) -> Result<(), ::qtrs_core::meta::InvokeError> {
                if let Some(s) = obj.downcast_mut::<#struct_name>() {
                    // Try to convert variant to property type
                    if let Some(converted) = ::qtrs_core::variant::convert_variant_to_type::<#field_type>(&val) {
                        s.#field_ident = converted;
                        Ok(())
                    } else {
                        Err(::qtrs_core::meta::InvokeError::TypeMismatch {
                            index: 0,
                            expected: stringify!(#field_type),
                        })
                    }
                } else {
                    Err(::qtrs_core::meta::InvokeError::TargetBorrowFailed)
                }
            }
        }
    } else {
        quote! {
            #[allow(non_snake_case)]
            fn #setter_ident(
                _obj: &mut dyn ::std::any::Any,
                _val: ::qtrs_core::variant::Variant,
            ) -> Result<(), ::qtrs_core::meta::InvokeError> {
                Err(::qtrs_core::meta::InvokeError::ExecutionFailed("Property is read-only".into()))
            }
        }
    };

    (getter, setter)
}
