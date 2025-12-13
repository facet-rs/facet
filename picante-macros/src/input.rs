use crate::struct_item::StructItem;
use crate::util::snake_ident;
use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};

pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let item: TokenStream2 = item.into();
    let parsed = match StructItem::parse(item) {
        Ok(v) => v,
        Err(e) => return crate::util::compile_error(&e),
    };

    if !parsed.generics.is_empty() {
        return crate::util::compile_error("picante: #[picante::input] does not support generics");
    }

    let Some((key_field, non_key_fields)) = split_key_field(&parsed) else {
        return crate::util::compile_error(
            "picante: #[picante::input] requires exactly one field marked #[key]",
        );
    };

    let name = parsed.name.clone();
    let vis = parsed.vis.clone();

    let snake = snake_ident(&name);
    let snake_s = snake.to_string();
    let upper = snake_s.to_uppercase();

    let keys_kind = Ident::new(&format!("{upper}_KEYS_KIND"), name.span());
    let data_kind = Ident::new(&format!("{upper}_DATA_KIND"), name.span());

    let keys_ty = format_ident!("{}KeysIngredient", name);
    let data_ty = format_ident!("{}DataIngredient", name);
    let data_struct = format_ident!("{}Data", name);
    let has_trait = format_ident!("Has{}Ingredient", name);

    let keys_accessor = format_ident!("{}_keys", snake);
    let data_accessor = format_ident!("{}_data", snake);

    let make_keys = format_ident!("make_{}_keys", snake);
    let make_data = format_ident!("make_{}_data", snake);

    let key_field_name = &key_field.name;
    let key_field_ty = &key_field.ty;

    let data_fields = non_key_fields.iter().map(|f| {
        let attrs = &f.attrs;
        let vis = &f.vis;
        let name = &f.name;
        let ty = &f.ty;
        quote! { #(#attrs)* #vis #name: #ty }
    });

    let ctor_params = non_key_fields.iter().map(|f| {
        let name = &f.name;
        let ty = &f.ty;
        quote! { #name: #ty }
    });

    let ctor_inits = non_key_fields.iter().map(|f| {
        let name = &f.name;
        quote! { #name }
    });

    let getters = non_key_fields.iter().map(|f| {
        let field_name = &f.name;
        let field_ty = &f.ty;
        quote! {
            #[allow(clippy::needless_question_mark)]
            pub fn #field_name<DB: #has_trait>(self, db: &DB) -> picante::PicanteResult<#field_ty> {
                Ok(self.data(db)?.#field_name.clone())
            }
        }
    });

    let doc_attrs = parsed.doc_attrs.clone();

    let field_ty_asserts = non_key_fields.iter().map(|f| {
        let ty = &f.ty;
        quote! { let _ = __picante_assert_field_traits::<#ty>; }
    });

    let expanded = quote! {
        /// Stable kind id for the key interner of `#name`.
        #vis const #keys_kind: picante::QueryKindId = picante::QueryKindId::from_str(concat!(
            module_path!(),
            "::",
            stringify!(#name),
            "::keys",
        ));

        /// Stable kind id for the input storage of `#name`.
        #vis const #data_kind: picante::QueryKindId = picante::QueryKindId::from_str(concat!(
            module_path!(),
            "::",
            stringify!(#name),
            "::data",
        ));

        #(#doc_attrs)*
        /// Key interner ingredient for `#name`.
        #vis type #keys_ty = picante::InternedIngredient<#key_field_ty>;

        #(#doc_attrs)*
        /// Input storage ingredient for `#name`.
        #vis type #data_ty = picante::InputIngredient<picante::InternId, ::std::sync::Arc<#data_struct>>;

        const _: () = {
            fn __picante_assert_key_traits<K>()
            where
                K: facet::Facet<'static> + Send + Sync + 'static,
            {
            }

            fn __picante_assert_field_traits<T>()
            where
                T: Clone + facet::Facet<'static> + Send + Sync + 'static,
            {
            }

            let _ = __picante_assert_key_traits::<#key_field_ty>;
            #(#field_ty_asserts)*
        };

        #(#doc_attrs)*
        /// Stored data for `#name` (non-key fields).
        #[derive(facet::Facet)]
        #vis struct #data_struct {
            #(#data_fields,)*
        }

        #(#doc_attrs)*
        /// Trait for databases that store `#name` ingredients.
        #vis trait #has_trait: picante::HasRuntime {
            /// Access the key interner for `#name`.
            fn #keys_accessor(&self) -> &#keys_ty;
            /// Access the data storage for `#name`.
            fn #data_accessor(&self) -> &#data_ty;
        }

        /// Construct a new key interner for `#name`.
        #vis fn #make_keys() -> ::std::sync::Arc<#keys_ty> {
            ::std::sync::Arc::new(picante::InternedIngredient::new(
                #keys_kind,
                concat!(stringify!(#name), "::keys"),
            ))
        }

        /// Construct a new input storage for `#name`.
        #vis fn #make_data() -> ::std::sync::Arc<#data_ty> {
            ::std::sync::Arc::new(picante::InputIngredient::new(
                #data_kind,
                concat!(stringify!(#name), "::data"),
            ))
        }

        #(#doc_attrs)*
        #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, facet::Facet)]
        #[repr(transparent)]
        #vis struct #name(pub picante::InternId);

        impl #name {
            /// Create or update an input record and return its id.
            #vis fn new<DB: #has_trait>(
                db: &DB,
                #key_field_name: #key_field_ty,
                #(#ctor_params,)*
            ) -> picante::PicanteResult<Self> {
                let id = db.#keys_accessor().intern(#key_field_name)?;
                let data = #data_struct { #(#ctor_inits),* };
                db.#data_accessor().set(db, id, ::std::sync::Arc::new(data));
                Ok(Self(id))
            }

            /// Read the key field.
            #vis fn #key_field_name<DB: #has_trait>(
                self,
                db: &DB,
            ) -> picante::PicanteResult<::std::sync::Arc<#key_field_ty>> {
                db.#keys_accessor().get(db, self.0)
            }

            /// Read the stored data (errors if missing/removed).
            #vis fn data<DB: #has_trait>(self, db: &DB) -> picante::PicanteResult<::std::sync::Arc<#data_struct>> {
                let Some(data) = db.#data_accessor().get(db, &self.0)? else {
                    let key = picante::Key::encode_facet(&self.0)?;
                    return Err(::std::sync::Arc::new(picante::PicanteError::MissingInputValue {
                        kind: #data_kind,
                        key_hash: key.hash(),
                    }));
                };
                Ok(data)
            }

            #(#getters)*
        }
    };

    expanded.into()
}

fn split_key_field(
    parsed: &StructItem,
) -> Option<(
    &crate::struct_item::StructField,
    Vec<&crate::struct_item::StructField>,
)> {
    let mut key: Option<&crate::struct_item::StructField> = None;
    let mut rest = Vec::new();

    for f in &parsed.fields {
        if f.is_key {
            if key.is_some() {
                return None;
            }
            key = Some(f);
        } else {
            rest.push(f);
        }
    }

    key.map(|k| (k, rest))
}
