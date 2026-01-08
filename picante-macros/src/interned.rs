use crate::struct_item::StructItem;
use crate::util::snake_ident;
use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};

// r[macro.interned.purpose]
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let item: TokenStream2 = item.into();
    let parsed = match StructItem::parse(item) {
        Ok(v) => v,
        Err(e) => return crate::util::compile_error(&e),
    };

    if !parsed.generics.is_empty() {
        return crate::util::compile_error(
            "picante: #[picante::interned] does not support generics",
        );
    }

    let name = parsed.name;
    let vis = parsed.vis;

    let snake = snake_ident(&name);
    let kind_const = Ident::new(
        &format!("{}_KIND", snake.to_string().to_uppercase()),
        name.span(),
    );

    let ingredient_ty = format_ident!("{}Ingredient", name);
    let value_struct = format_ident!("{}Data", name);
    let has_trait = format_ident!("Has{}Ingredient", name);
    let accessor = format_ident!("{}_ingredient", snake);
    let make_fn = format_ident!("make_{}_ingredient", snake);

    let ctor_params = parsed.fields.iter().map(|f| {
        let name = &f.name;
        let ty = &f.ty;
        quote! { #name: #ty }
    });

    let ctor_inits = parsed.fields.iter().map(|f| {
        let name = &f.name;
        quote! { #name }
    });

    let value_fields = parsed.fields.iter().map(|f| {
        let attrs = &f.attrs;
        let vis = &f.vis;
        let name = &f.name;
        let ty = &f.ty;
        quote! { #(#attrs)* #vis #name: #ty }
    });

    let getters = parsed.fields.iter().map(|f| {
        let field_name = &f.name;
        let field_ty = &f.ty;
        quote! {
            pub fn #field_name<DB: #has_trait>(self, db: &DB) -> picante::PicanteResult<#field_ty> {
                Ok(self.value(db)?.#field_name.clone())
            }
        }
    });

    let doc_attrs = parsed.doc_attrs;

    let field_ty_asserts = parsed.fields.iter().map(|f| {
        let ty = &f.ty;
        quote! { let _ = __picante_assert_field_traits::<#ty>; }
    });

    // r[macro.interned.output]
    let expanded = quote! {
        /// Stable kind id for interned `#name` values.
        #vis const #kind_const: picante::QueryKindId =
            picante::QueryKindId::from_str(concat!(module_path!(), "::", stringify!(#name)));

        const _: () = {
            fn __picante_assert_field_traits<T>()
            where
                T: Clone + facet::Facet<'static> + Send + Sync + 'static,
            {
            }

            #(#field_ty_asserts)*
        };

        #(#doc_attrs)*
        /// Stored data for interned `#name` values.
        #[derive(facet::Facet)]
        #vis struct #value_struct {
            #(#value_fields,)*
        }

        #(#doc_attrs)*
        /// Ingredient type for interned `#name` values.
        #vis type #ingredient_ty = picante::InternedIngredient<#value_struct>;

        #(#doc_attrs)*
        /// Trait for databases that store the `#name` ingredient.
        #vis trait #has_trait: picante::HasRuntime {
            fn #accessor(&self) -> &#ingredient_ty;
        }

        /// Construct a new `#name` interner.
        #vis fn #make_fn() -> ::std::sync::Arc<#ingredient_ty> {
            ::std::sync::Arc::new(picante::InternedIngredient::new(#kind_const, stringify!(#name)))
        }

        #(#doc_attrs)*
        #[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, facet::Facet)]
        #[repr(transparent)]
        #vis struct #name(pub picante::InternId);

        impl #name {
            #vis fn new<DB: #has_trait>(db: &DB, #(#ctor_params,)*) -> picante::PicanteResult<Self> {
                let data = #value_struct { #(#ctor_inits),* };
                Ok(Self(db.#accessor().intern(data)?))
            }

            #vis fn value<DB: #has_trait>(self, db: &DB) -> picante::PicanteResult<::std::sync::Arc<#value_struct>> {
                db.#accessor().get(db, self.0)
            }

            #(#getters)*
        }
    };

    expanded.into()
}
