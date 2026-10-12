//! Pieces shared by the generated invokers of `#[derive(QObject)]` and `#[slots]`.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{Ident, LitStr, Type};

/// Spelling of a parameter type in meta-object signatures, e.g. `Vec<u8>` -> `Vec<u8>`.
pub(crate) fn type_name(ty: &Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}

/// Converts `args[i]` into `__qtrs_arg{i}` for each parameter type. A value that does not
/// convert fails with `TypeMismatch` at its index. Returns the bound names and the statements.
pub(crate) fn argument_bindings(types: &[Type]) -> (Vec<Ident>, Vec<TokenStream>) {
    let idents: Vec<Ident> = (0..types.len())
        .map(|i| format_ident!("__qtrs_arg{}", i))
        .collect();
    let statements = types
        .iter()
        .zip(&idents)
        .enumerate()
        .map(|(index, (ty, ident))| {
            let expected = LitStr::new(&type_name(ty), Span::call_site());
            quote! {
                let #ident = ::qtrs_core::variant::convert_variant_to_type::<#ty>(&args[#index])
                    .ok_or(::qtrs_core::meta::InvokeError::TypeMismatch {
                        index: #index,
                        expected: #expected,
                    })?;
            }
        })
        .collect();
    (idents, statements)
}

/// Emits a module-level invoker: it downcasts `target` to `Target`, runs the argument `bindings`,
/// then evaluates `body`, a `Result<Variant, InvokeError>` expression with `this` in scope.
pub(crate) fn invoker_fn(
    invoker: &Ident,
    target: &impl ToTokens,
    bindings: &[TokenStream],
    body: TokenStream,
) -> TokenStream {
    quote! {
        #[allow(non_snake_case, unused_variables)]
        fn #invoker(
            object: &mut dyn ::std::any::Any,
            args: &[::qtrs_core::variant::Variant],
        ) -> ::std::result::Result<::qtrs_core::variant::Variant, ::qtrs_core::meta::InvokeError> {
            let this = object
                .downcast_mut::<#target>()
                .ok_or(::qtrs_core::meta::InvokeError::TargetBorrowFailed)?;
            #(#bindings)*
            #body
        }
    }
}
