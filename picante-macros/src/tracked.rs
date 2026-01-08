use crate::util::{
    FnItem, Param, compile_error, fn_kind_id_ident, parse_db_generic, pascal_ident, snake_ident,
    strip_ref_inner_ident, take_return_ok_type,
};
use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote};

// r[macro.tracked.purpose]
pub(crate) fn expand(item: TokenStream) -> TokenStream {
    let item: TokenStream2 = item.into();
    let parsed = match FnItem::parse(item) {
        Ok(v) => v,
        Err(e) => return compile_error(&e),
    };

    if parsed.params.is_empty() {
        return compile_error("picante: #[picante::tracked] requires a `db` parameter");
    }

    let db_param = &parsed.params[0];
    if db_param.name != "db" {
        return compile_error("picante: first parameter must be named `db`");
    }

    let (db_ident, db_bounds) = match parse_db_generic(&parsed.generics) {
        Ok(v) => v,
        Err(e) => return compile_error(&e),
    };

    let Some(db_ty_ident) = strip_ref_inner_ident(&db_param.param_type) else {
        return compile_error("picante: `db` must be `&DB` (with `DB` as a generic type param)");
    };

    if db_ty_ident != db_ident {
        return compile_error("picante: `db` parameter type must be `&DB`");
    }

    let name = parsed.name;
    let vis = parsed.vis;
    let doc_attrs = parsed.doc_attrs;
    let impl_attrs = parsed.other_attrs;

    let kind_const = fn_kind_id_ident(&name);
    let pascal = pascal_ident(&name);
    let query_ty = format_ident!("{}Query", pascal);
    let has_trait = format_ident!("Has{}Query", pascal);
    let getter_name = format_ident!("{}_query", snake_ident(&name));
    let make_name = format_ident!("make_{}_query", snake_ident(&name));
    let impl_name = format_ident!("__picante_impl_{}", name);

    let (output_ty, returns_picante_result) = match take_return_ok_type(&parsed.return_type) {
        Ok(v) => v,
        Err(e) => return compile_error(&e),
    };

    // r[macro.tracked.key-tuple]
    let (key_ty, key_expr, unpack_key) = build_key(&parsed.params[1..]);
    let call_impl = build_call_impl(&impl_name, parsed.is_async, &parsed.params[1..]);

    // r[macro.tracked.return-wrap]
    let compute = if returns_picante_result {
        quote! { #call_impl }
    } else {
        quote! { Ok(#call_impl) }
    };

    let wrapper_params = parsed.params[1..].iter().map(|p| {
        let name = &p.name;
        let ty = &p.param_type;
        quote! { #name: #ty }
    });

    let impl_params = parsed.params.iter().map(|p| {
        let name = &p.name;
        let ty = &p.param_type;
        quote! { #name: #ty }
    });

    let async_kw = parsed.is_async.then(|| quote! { async });
    let return_ty = parsed.return_type;
    let body = parsed.body;

    // Has*Query traits use minimal supertraits (no user bounds).
    // This allows DbTrait to include Has*Query without cycle risk.
    // User bounds only apply to make_*_query() constructor.
    let supertraits = quote! { picante::IngredientLookup + Send + Sync + 'static };

    let make_bounds = if db_bounds.is_empty() {
        quote! { picante::IngredientLookup + Send + Sync + 'static }
    } else {
        quote! { #db_bounds + picante::IngredientLookup + Send + Sync + 'static }
    };

    let impl_where_clause = if db_bounds.is_empty() {
        TokenStream2::new()
    } else {
        quote! { where #db_ident: #db_bounds }
    };

    // r[macro.tracked.output]
    let expanded = quote! {
        /// Stable kind id for this query.
        #vis const #kind_const: picante::QueryKindId =
            picante::QueryKindId::from_str(concat!(module_path!(), "::", stringify!(#name)));

        #(#doc_attrs)*
        /// Derived query ingredient type for `#name`.
        #vis type #query_ty<DB> = picante::DerivedIngredient<DB, #key_ty, #output_ty>;

        const _: () = {
            fn __picante_assert_key_traits<K>()
            where
                K: Clone
                    + Eq
                    + ::std::hash::Hash
                    + facet::Facet<'static>
                    + Send
                    + Sync
                    + 'static,
            {
            }

            fn __picante_assert_value_traits<V>()
            where
                V: Clone + facet::Facet<'static> + Send + Sync + 'static,
            {
            }

            let _ = __picante_assert_key_traits::<#key_ty>;
            let _ = __picante_assert_value_traits::<#output_ty>;
        };

        #(#doc_attrs)*
        /// Trait for databases that store the `#name` query ingredient.
        #vis trait #has_trait: #supertraits {
            /// Access the `#name` query ingredient.
            fn #getter_name(&self) -> &#query_ty<Self>;
        }

        #(#doc_attrs)*
        /// Construct a new `#name` query ingredient.
        #vis fn #make_name<DB>() -> ::std::sync::Arc<#query_ty<DB>>
        where
            DB: #make_bounds,
        {
            ::std::sync::Arc::new(picante::DerivedIngredient::new(
                #kind_const,
                stringify!(#name),
                |db, key| {
                    Box::pin(async move {
                        #unpack_key
                        #compute
                    })
                },
            ))
        }

        #(#doc_attrs)*
        /// Memoized wrapper for `#name`.
        #vis async fn #name<DB>(db: &DB, #(#wrapper_params),*) -> picante::PicanteResult<#output_ty>
        where
            DB: #has_trait,
        {
            db.#getter_name().get(db, #key_expr).await
        }

        #(#impl_attrs)*
        #vis #async_kw fn #impl_name<#db_ident>(#(#impl_params),*) -> #return_ty
        #impl_where_clause
        #body
    };

    expanded.into()
}

fn build_key(params: &[Param]) -> (TokenStream2, TokenStream2, TokenStream2) {
    if params.is_empty() {
        return (quote! { () }, quote! { () }, quote! {});
    }

    if params.len() == 1 {
        let name = &params[0].name;
        let ty = &params[0].param_type;
        return (
            ty.clone(),
            quote! { #name },
            quote! {
                let #name = key;
            },
        );
    }

    let tys: Vec<_> = params.iter().map(|p| &p.param_type).collect();
    let names: Vec<_> = params.iter().map(|p| &p.name).collect();

    (
        quote! { ( #(#tys),* ) },
        quote! { ( #(#names),* ) },
        quote! { let ( #(#names),* ) = key; },
    )
}

fn build_call_impl(impl_name: &Ident, is_async: bool, params: &[Param]) -> TokenStream2 {
    let args = params.iter().map(|p| &p.name);
    if is_async {
        quote! { #impl_name(db, #(#args),*).await }
    } else {
        quote! { #impl_name(db, #(#args),*) }
    }
}
