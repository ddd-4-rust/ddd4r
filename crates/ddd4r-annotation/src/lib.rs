//! DDD-specific derive macros.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

fn find_id_field(input: &DeriveInput) -> syn::Result<(&syn::Ident, &syn::Type)> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            input,
            "DomainModel can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            input,
            "DomainModel requires named fields",
        ));
    };
    let mut fallback = None;
    for field in &fields.named {
        let Some(identifier) = &field.ident else {
            continue;
        };
        if identifier == "id" {
            fallback = Some((identifier, &field.ty));
        }
        for attribute in &field.attrs {
            if attribute.path().is_ident("ddd4r") {
                let mut is_id = false;
                attribute.parse_nested_meta(|meta| {
                    if meta.path.is_ident("id") {
                        is_id = true;
                        Ok(())
                    } else {
                        Err(meta.error("unsupported ddd4r field attribute"))
                    }
                })?;
                if is_id {
                    return Ok((identifier, &field.ty));
                }
            }
        }
    }
    fallback.ok_or_else(|| {
        syn::Error::new_spanned(input, "mark one field with #[ddd4r(id)] or name it `id`")
    })
}

/// Implements `ddd4r_core::DomainModel` using an `id` field.
#[proc_macro_derive(DomainModel, attributes(ddd4r))]
pub fn derive_domain_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let identifier = &input.ident;
    let (id_field, id_type) = match find_id_field(&input) {
        Ok(field) => field,
        Err(error) => return error.into_compile_error().into(),
    };
    quote! {
        impl ::ddd4r_core::domain::DomainModel for #identifier {
            type Id = #id_type;

            fn id(&self) -> &Self::Id {
                &self.#id_field
            }
        }
    }
    .into()
}

/// Implements the ddd4r entity marker.
#[proc_macro_derive(Entity)]
pub fn derive_entity(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let identifier = &input.ident;
    quote! {
        impl ::ddd4r_core::domain::Entity for #identifier {}
    }
    .into()
}

/// Implements the ddd4r value-object marker.
#[proc_macro_derive(ValueObject)]
pub fn derive_value_object(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let identifier = &input.ident;
    quote! {
        impl ::ddd4r_core::domain::ValueObject for #identifier {}
    }
    .into()
}
