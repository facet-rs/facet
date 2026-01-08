use crate::struct_item::{StructDecl, StructField};
use crate::util::{compile_error, pascal_ident, snake_ident};
use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Ident, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};
use unsynn::{IParse, ToTokenIter, ToTokens};

// r[macro.db.purpose]

#[derive(Default)]
struct DbArgs {
    inputs: Vec<ItemPath>,
    interned: Vec<ItemPath>,
    tracked: Vec<ItemPath>,
    /// Custom name for the generated combined trait. If None, defaults to `{DbName}Trait`.
    db_trait: Option<Ident>,
}

#[derive(Clone)]
struct ItemPath {
    prefix: TokenStream2,
    name: Ident,
}

impl ItemPath {
    fn parse(ts: TokenStream2) -> Result<Self, String> {
        let tokens: Vec<TokenTree> = ts.into_iter().collect();
        if tokens.is_empty() {
            return Err("picante: expected an item path".to_string());
        }

        // Only accept `::`-separated identifiers (no generics).
        for tt in &tokens {
            match tt {
                TokenTree::Ident(_) => {}
                TokenTree::Punct(p) if p.as_char() == ':' => {}
                _ => {
                    return Err(
                        "picante: expected a path like `foo::Bar` (no generics)".to_string()
                    );
                }
            }
        }

        let Some(TokenTree::Ident(name)) = tokens.last() else {
            return Err("picante: expected a path ending in an identifier".to_string());
        };

        let mut prefix = TokenStream2::new();
        for tt in tokens[..tokens.len() - 1].iter().cloned() {
            prefix.extend(::core::iter::once(tt));
        }

        Ok(Self {
            prefix,
            name: name.clone(),
        })
    }

    fn qualify(&self, ident: &Ident) -> TokenStream2 {
        let prefix = &self.prefix;
        quote! { #prefix #ident }
    }
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr: TokenStream2 = attr.into();
    let item: TokenStream2 = item.into();

    let args = match parse_args(attr) {
        Ok(v) => v,
        Err(e) => return compile_error(&e),
    };

    let mut it = item.to_token_iter();
    let decl: StructDecl = match it.parse() {
        Ok(v) => v,
        Err(e) => return compile_error(&format!("picante: failed to parse db struct: {e:?}")),
    };

    if decl.generics.is_some() {
        return compile_error("picante: #[picante::db] does not support generics");
    }

    let struct_attrs: Vec<_> = decl
        .attributes
        .as_ref()
        .map(|m| m.as_slice().iter().map(|a| a.to_token_stream()).collect())
        .unwrap_or_default();

    let vis = decl
        .vis
        .as_ref()
        .map(|v| v.to_token_stream())
        .unwrap_or_default();

    let db_name = decl.name;

    let mut custom_fields = Vec::new();
    let mut custom_field_names = std::collections::HashSet::<String>::new();
    for delim in decl.fields.content.into_iter() {
        let f = delim.value;

        let field_vis = f
            .vis
            .as_ref()
            .map(|v| v.to_token_stream())
            .unwrap_or_default();

        let field_attrs: Vec<_> = f
            .attributes
            .as_ref()
            .map(|m| m.as_slice().iter().map(|a| a.to_token_stream()).collect())
            .unwrap_or_default();

        let name = f.name;
        let ty = f.typ.to_token_stream();

        if !custom_field_names.insert(name.to_string()) {
            return compile_error(&format!("picante: duplicate field `{}` in db struct", name));
        }

        custom_fields.push(StructField {
            name,
            vis: field_vis,
            ty,
            attrs: field_attrs,
            is_key: false,
        });
    }

    for reserved in ["runtime", "ingredients"] {
        if custom_field_names.contains(reserved) {
            return compile_error(&format!(
                "picante: db field name conflict: `{}` is reserved",
                reserved
            ));
        }
    }

    let mut generated_field_names = std::collections::HashSet::<String>::new();
    generated_field_names.insert("runtime".to_string());
    generated_field_names.insert("ingredients".to_string());

    let mut generated_fields = Vec::<TokenStream2>::new();
    let mut ctor_lets = Vec::<TokenStream2>::new();
    let mut ctor_registers = Vec::<TokenStream2>::new();
    let mut ctor_inits = Vec::<TokenStream2>::new();

    let mut trait_impls = Vec::<TokenStream2>::new();

    // Collect Has* trait bounds for the combined db trait
    let mut has_trait_bounds = Vec::<TokenStream2>::new();

    // Inputs: two ingredients per input "entity" (keys + data)
    for input in &args.inputs {
        let entity = &input.name;
        let entity_snake = snake_ident(entity);

        let keys_field = format_ident!("{entity_snake}_keys_ingredient");
        let data_field = format_ident!("{entity_snake}_data_ingredient");

        for name in [&keys_field, &data_field] {
            let name_s = name.to_string();
            if custom_field_names.contains(&name_s) {
                return compile_error(&format!(
                    "picante: db field name conflict: `{}` is a custom db field and also generated by input `{}`",
                    name_s, entity,
                ));
            }
            if !generated_field_names.insert(name_s.clone()) {
                return compile_error(&format!(
                    "picante: db field name conflict: `{}` is generated multiple times (check for duplicate/overlapping inputs)",
                    name_s,
                ));
            }
        }

        let keys_ty_ident = format_ident!("{entity}KeysIngredient");
        let data_ty_ident = format_ident!("{entity}DataIngredient");
        let has_trait_ident = format_ident!("Has{entity}Ingredient");

        let keys_ty = input.qualify(&keys_ty_ident);
        let data_ty = input.qualify(&data_ty_ident);
        let has_trait = input.qualify(&has_trait_ident);

        has_trait_bounds.push(quote! { #has_trait });

        let make_keys_ident = format_ident!("make_{entity_snake}_keys");
        let make_data_ident = format_ident!("make_{entity_snake}_data");
        let make_keys = input.qualify(&make_keys_ident);
        let make_data = input.qualify(&make_data_ident);

        generated_fields.push(quote! {
            #keys_field: ::std::sync::Arc<#keys_ty>,
            #data_field: ::std::sync::Arc<#data_ty>,
        });

        ctor_lets.push(quote! {
            let #keys_field = #make_keys();
            let #data_field = #make_data();
        });
        ctor_registers.push(quote! {
            ingredients.register(#keys_field.clone());
            ingredients.register(#data_field.clone());
        });
        ctor_inits.push(quote! { #keys_field, #data_field });

        let keys_method = format_ident!("{entity_snake}_keys");
        let data_method = format_ident!("{entity_snake}_data");

        trait_impls.push(quote! {
            impl #has_trait for #db_name {
                fn #keys_method(&self) -> &#keys_ty {
                    &self.#keys_field
                }

                fn #data_method(&self) -> &#data_ty {
                    &self.#data_field
                }
            }
        });
    }

    // Interned: one ingredient per interned "entity"
    for interned in &args.interned {
        let entity = &interned.name;
        let entity_snake = snake_ident(entity);

        let field = format_ident!("{entity_snake}_ingredient");
        let field_s = field.to_string();
        if custom_field_names.contains(&field_s) {
            return compile_error(&format!(
                "picante: db field name conflict: `{}` is a custom db field and also generated by interned `{}`",
                field_s, entity,
            ));
        }
        if !generated_field_names.insert(field_s.clone()) {
            return compile_error(&format!(
                "picante: db field name conflict: `{}` is generated multiple times (check for duplicate/overlapping interned items)",
                field_s,
            ));
        }

        let ingredient_ty_ident = format_ident!("{entity}Ingredient");
        let has_trait_ident = format_ident!("Has{entity}Ingredient");

        let ingredient_ty = interned.qualify(&ingredient_ty_ident);
        let has_trait = interned.qualify(&has_trait_ident);

        has_trait_bounds.push(quote! { #has_trait });

        let accessor = format_ident!("{entity_snake}_ingredient");
        let make_ident = format_ident!("make_{entity_snake}_ingredient");
        let make_fn = interned.qualify(&make_ident);

        generated_fields.push(quote! {
            #field: ::std::sync::Arc<#ingredient_ty>,
        });

        ctor_lets.push(quote! {
            let #field = #make_fn();
        });
        ctor_registers.push(quote! {
            ingredients.register(#field.clone());
        });
        ctor_inits.push(quote! { #field });

        trait_impls.push(quote! {
            impl #has_trait for #db_name {
                fn #accessor(&self) -> &#ingredient_ty {
                    &self.#field
                }
            }
        });
    }

    // Tracked: one derived ingredient per query
    for tracked in &args.tracked {
        let func = &tracked.name;
        let func_snake = snake_ident(func);
        let field = func_snake.clone();

        let field_s = field.to_string();
        if custom_field_names.contains(&field_s) {
            return compile_error(&format!(
                "picante: db field name conflict: `{}` is a custom db field and also generated by tracked `{}`",
                field_s, func,
            ));
        }
        if !generated_field_names.insert(field_s.clone()) {
            return compile_error(&format!(
                "picante: db field name conflict: `{}` is generated multiple times (check for duplicate/overlapping tracked queries)",
                field_s,
            ));
        }

        let pascal = pascal_ident(func);
        let query_ty_ident = format_ident!("{pascal}Query");
        let has_trait_ident = format_ident!("Has{pascal}Query");

        let query_ty = tracked.qualify(&query_ty_ident);
        let has_trait = tracked.qualify(&has_trait_ident);

        // Include Has*Query traits in the combined db trait.
        // This is safe because Has*Query supertraits no longer include user-specified bounds
        // (they only require IngredientLookup + Send + Sync + 'static).
        has_trait_bounds.push(quote! { #has_trait });

        let getter = format_ident!("{func_snake}_query");
        let make_ident = format_ident!("make_{func_snake}_query");
        let make_fn = tracked.qualify(&make_ident);

        generated_fields.push(quote! {
            #field: ::std::sync::Arc<#query_ty<#db_name>>,
        });

        ctor_lets.push(quote! {
            let #field = #make_fn::<#db_name>();
        });
        ctor_registers.push(quote! {
            ingredients.register(#field.clone());
        });
        ctor_inits.push(quote! { #field });

        trait_impls.push(quote! {
            impl #has_trait for #db_name {
                fn #getter(&self) -> &#query_ty<Self> {
                    &self.#field
                }
            }
        });
    }

    let custom_field_defs = custom_fields.iter().map(|f| {
        let attrs = &f.attrs;
        let vis = &f.vis;
        let name = &f.name;
        let ty = &f.ty;
        quote! { #(#attrs)* #vis #name: #ty, }
    });

    let ctor_params = custom_fields.iter().map(|f| {
        let name = &f.name;
        let ty = &f.ty;
        quote! { #name: #ty }
    });

    let custom_inits = custom_fields.iter().map(|f| {
        let name = &f.name;
        quote! { #name }
    });

    // Generate the combined db trait
    let db_trait_name = args
        .db_trait
        .unwrap_or_else(|| format_ident!("{db_name}Trait"));

    let db_trait_def = quote! {
        /// Combined trait for all ingredients in this database.
        ///
        /// Use this as a bound instead of listing all `Has*Ingredient` traits individually:
        /// ```ignore
        /// async fn my_query<DB: #db_trait_name>(db: &DB, ...) -> ...
        /// ```
        #vis trait #db_trait_name:
            #(#has_trait_bounds +)*
            picante::HasRuntime +
            picante::IngredientLookup +
            ::core::marker::Send +
            ::core::marker::Sync +
            'static
        {}

        impl #db_trait_name for #db_name {}
    };

    // === Generate Snapshot struct and implementations ===

    let snapshot_name = format_ident!("{}Snapshot", db_name);

    // Snapshot fields (same as db, but without custom fields)
    let mut snapshot_fields = Vec::<TokenStream2>::new();
    let mut snapshot_inits = Vec::<TokenStream2>::new();
    let mut snapshot_registers = Vec::<TokenStream2>::new();
    let mut snapshot_trait_impls = Vec::<TokenStream2>::new();

    // Inputs: snapshot the data, share the keys (interned)
    for input in &args.inputs {
        let entity = &input.name;
        let entity_snake = snake_ident(entity);

        let keys_field = format_ident!("{entity_snake}_keys_ingredient");
        let data_field = format_ident!("{entity_snake}_data_ingredient");

        let keys_ty_ident = format_ident!("{entity}KeysIngredient");
        let data_ty_ident = format_ident!("{entity}DataIngredient");
        let has_trait_ident = format_ident!("Has{entity}Ingredient");

        let keys_ty = input.qualify(&keys_ty_ident);
        let data_ty = input.qualify(&data_ty_ident);
        let has_trait = input.qualify(&has_trait_ident);

        let _make_data_ident = format_ident!("make_{entity_snake}_data");

        snapshot_fields.push(quote! {
            #keys_field: ::std::sync::Arc<#keys_ty>,
            #data_field: ::std::sync::Arc<#data_ty>,
        });

        // Keys are shared (interned, append-only), data is snapshotted
        snapshot_inits.push(quote! {
            #keys_field: db.#keys_field.clone(),
            #data_field: {
                let snapshot_data = db.#data_field.snapshot();
                ::std::sync::Arc::new(picante::InputIngredient::new_from_snapshot(
                    db.#data_field.kind(),
                    db.#data_field.kind_name(),
                    snapshot_data,
                ))
            },
        });

        snapshot_registers.push(quote! {
            snapshot.ingredients.register(snapshot.#keys_field.clone());
            snapshot.ingredients.register(snapshot.#data_field.clone());
        });

        let keys_method = format_ident!("{entity_snake}_keys");
        let data_method = format_ident!("{entity_snake}_data");

        snapshot_trait_impls.push(quote! {
            impl #has_trait for #snapshot_name {
                fn #keys_method(&self) -> &#keys_ty {
                    &self.#keys_field
                }

                fn #data_method(&self) -> &#data_ty {
                    &self.#data_field
                }
            }
        });
    }

    // r[snapshot.interned]
    // Interned: share the Arc (append-only, stable)
    for interned in &args.interned {
        let entity = &interned.name;
        let entity_snake = snake_ident(entity);

        let field = format_ident!("{entity_snake}_ingredient");

        let ingredient_ty_ident = format_ident!("{entity}Ingredient");
        let has_trait_ident = format_ident!("Has{entity}Ingredient");

        let ingredient_ty = interned.qualify(&ingredient_ty_ident);
        let has_trait = interned.qualify(&has_trait_ident);

        snapshot_fields.push(quote! {
            #field: ::std::sync::Arc<#ingredient_ty>,
        });

        // Share the Arc directly (interned values are append-only)
        snapshot_inits.push(quote! {
            #field: db.#field.clone(),
        });

        snapshot_registers.push(quote! {
            snapshot.ingredients.register(snapshot.#field.clone());
        });

        let accessor = format_ident!("{entity_snake}_ingredient");

        snapshot_trait_impls.push(quote! {
            impl #has_trait for #snapshot_name {
                fn #accessor(&self) -> &#ingredient_ty {
                    &self.#field
                }
            }
        });
    }

    // Tracked: create new DerivedIngredient with snapshotted cells
    for tracked in &args.tracked {
        let func = &tracked.name;
        let func_snake = snake_ident(func);
        let field = func_snake.clone();

        let pascal = pascal_ident(func);
        let query_ty_ident = format_ident!("{pascal}Query");
        let has_trait_ident = format_ident!("Has{pascal}Query");

        let query_ty = tracked.qualify(&query_ty_ident);
        let has_trait = tracked.qualify(&has_trait_ident);

        let getter = format_ident!("{func_snake}_query");
        let make_ident = format_ident!("make_{func_snake}_query");
        let make_fn = tracked.qualify(&make_ident);

        snapshot_fields.push(quote! {
            #field: ::std::sync::Arc<#query_ty<#snapshot_name>>,
        });

        // Create new DerivedIngredient and load deep-cloned cells
        snapshot_inits.push(quote! {
            #field: {
                let cells = db.#field.snapshot_cells_deep().await;
                let ingredient = #make_fn::<#snapshot_name>();
                ingredient.load_cells(cells);
                ingredient
            },
        });

        snapshot_registers.push(quote! {
            snapshot.ingredients.register(snapshot.#field.clone());
        });

        snapshot_trait_impls.push(quote! {
            impl #has_trait for #snapshot_name {
                fn #getter(&self) -> &#query_ty<Self> {
                    &self.#field
                }
            }
        });
    }

    // r[macro.db.snapshot]
    // r[snapshot.frozen]
    // r[snapshot.independent]
    // r[snapshot.multiple]
    let snapshot_def = quote! {
        /// A point-in-time snapshot of the database.
        ///
        /// Snapshots share structure with the original database via `im::HashMap`,
        /// making them cheap to create (O(1)). Queries can be executed against
        /// a snapshot to get results as of the snapshot time.
        ///
        /// Snapshots are read-only views. While the ingredient methods technically
        /// allow mutations, modifying a snapshot is not supported and may cause
        /// unexpected behavior.
        #vis struct #snapshot_name {
            runtime: picante::Runtime,
            ingredients: picante::IngredientRegistry<#snapshot_name>,
            #(#snapshot_fields)*
        }

        impl #snapshot_name {
            // r[snapshot.creation]
            // r[snapshot.async]
            /// Create a snapshot from a database.
            ///
            /// This captures the current state of all inputs and cached query results.
            /// Input data uses O(1) structural sharing via `im::HashMap`.
            /// Cached query results are deep-cloned to ensure snapshot independence.
            ///
            /// The snapshot shares its `RuntimeId` with the parent database, enabling
            /// in-flight query deduplication across concurrent snapshots.
            #vis async fn from_database(db: &#db_name) -> Self {
                let parent_runtime = picante::HasRuntime::runtime(db);
                // Create a snapshot runtime that shares the parent's RuntimeId.
                // This allows in-flight query deduplication to work across snapshots.
                let runtime = picante::Runtime::new_for_snapshot(parent_runtime.id());
                // Set the snapshot's revision to match the database's current revision.
                // This ensures cached query results (which have verified_at from the db's revision)
                // are considered valid in the snapshot.
                runtime.set_current_revision(parent_runtime.current_revision());
                let mut snapshot = Self {
                    runtime,
                    ingredients: picante::IngredientRegistry::new(),
                    #(#snapshot_inits)*
                };

                // Register ingredients for dynamic dependency revalidation (IngredientLookup).
                //
                // Without this, `db.ingredient(dep.kind)` in revalidation will return None for
                // snapshots and all revalidation attempts will fail (forcing recomputation).
                #(#snapshot_registers)*

                snapshot
            }

            /// Access the ingredient registry (for persistence helpers).
            #vis fn ingredient_registry(&self) -> &picante::IngredientRegistry<Self> {
                &self.ingredients
            }
        }

        impl picante::HasRuntime for #snapshot_name {
            fn runtime(&self) -> &picante::Runtime {
                &self.runtime
            }
        }

        impl picante::IngredientLookup for #snapshot_name {
            fn ingredient(&self, kind: picante::QueryKindId) -> Option<&dyn picante::DynIngredient<Self>> {
                self.ingredients.ingredient(kind)
            }
        }

        #(#snapshot_trait_impls)*

        impl #db_trait_name for #snapshot_name {}
    };

    // Collect all ingredient field names for persistable_ingredients() method
    let mut all_ingredient_fields = Vec::<TokenStream2>::new();

    // Input ingredients: both keys and data
    for input in &args.inputs {
        let entity = &input.name;
        let entity_snake = snake_ident(entity);
        let keys_field = format_ident!("{entity_snake}_keys_ingredient");
        let data_field = format_ident!("{entity_snake}_data_ingredient");
        all_ingredient_fields.push(quote! { &*self.#keys_field });
        all_ingredient_fields.push(quote! { &*self.#data_field });
    }

    // Interned ingredients
    for interned in &args.interned {
        let entity = &interned.name;
        let entity_snake = snake_ident(entity);
        let field = format_ident!("{entity_snake}_ingredient");
        all_ingredient_fields.push(quote! { &*self.#field });
    }

    // Tracked query ingredients
    for tracked in &args.tracked {
        let func = &tracked.name;
        let func_snake = snake_ident(func);
        let field = func_snake.clone();
        all_ingredient_fields.push(quote! { &*self.#field });
    }

    // r[macro.db.output]
    let expanded = quote! {
        #(#struct_attrs)*
        #vis struct #db_name {
            runtime: picante::Runtime,
            ingredients: picante::IngredientRegistry<#db_name>,
            #(#generated_fields)*
            #(#custom_field_defs)*
        }

        impl #db_name {
            #vis fn new(#(#ctor_params),*) -> Self {
                let runtime = picante::Runtime::new();
                let mut ingredients = picante::IngredientRegistry::new();

                #(#ctor_lets)*
                #(#ctor_registers)*

                Self {
                    runtime,
                    ingredients,
                    #(#ctor_inits,)*
                    #(#custom_inits,)*
                }
            }

            /// Access the ingredient registry (for persistence helpers).
            #vis fn ingredient_registry(&self) -> &picante::IngredientRegistry<Self> {
                &self.ingredients
            }

            /// Returns a vector of all persistable ingredients in this database.
            ///
            /// This includes all input ingredients (keys and data), interned ingredients,
            /// and tracked query ingredients. Use this with `picante::persist::save_cache`
            /// and `picante::persist::load_cache`.
            ///
            /// # Example
            /// ```ignore
            /// use picante::persist::{save_cache, load_cache};
            ///
            /// let db = MyDb::new();
            /// let ingredients = db.persistable_ingredients();
            /// save_cache("cache.bin", db.runtime(), &ingredients).await?;
            /// ```
            #vis fn persistable_ingredients(&self) -> ::std::vec::Vec<&dyn picante::persist::PersistableIngredient> {
                ::std::vec![
                    #(#all_ingredient_fields as &dyn picante::persist::PersistableIngredient,)*
                ]
            }

            /// Save this database's state to a cache file.
            ///
            /// This is a convenience wrapper around `picante::persist::save_cache` that
            /// automatically collects all ingredients from this database.
            ///
            /// # Example
            /// ```ignore
            /// db.save_to_cache("cache.bin").await?;
            /// ```
            #vis async fn save_to_cache(&self, path: impl ::core::convert::AsRef<::std::path::Path>) -> picante::PicanteResult<()> {
                let ingredients = self.persistable_ingredients();
                picante::persist::save_cache(path, &self.runtime, &ingredients).await
            }

            /// Save this database's state to a cache file with custom options.
            ///
            /// This is a convenience wrapper around `picante::persist::save_cache_with_options`
            /// that automatically collects all ingredients from this database.
            ///
            /// # Example
            /// ```ignore
            /// use picante::persist::CacheSaveOptions;
            ///
            /// db.save_to_cache_with_options(
            ///     "cache.bin",
            ///     &CacheSaveOptions {
            ///         max_bytes: Some(4096),
            ///         max_records_per_section: None,
            ///         max_record_bytes: None,
            ///     }
            /// ).await?;
            /// ```
            #vis async fn save_to_cache_with_options(
                &self,
                path: impl ::core::convert::AsRef<::std::path::Path>,
                options: &picante::persist::CacheSaveOptions,
            ) -> picante::PicanteResult<()> {
                let ingredients = self.persistable_ingredients();
                picante::persist::save_cache_with_options(path, &self.runtime, &ingredients, options).await
            }

            /// Load this database's state from a cache file.
            ///
            /// This is a convenience wrapper around `picante::persist::load_cache` that
            /// automatically collects all ingredients from this database.
            ///
            /// Returns `Ok(true)` if the cache was loaded successfully, `Ok(false)` if there
            /// was no cache file or it was ignored due to corruption, or `Err(_)` on error.
            ///
            /// # Example
            /// ```ignore
            /// let loaded = db.load_from_cache("cache.bin").await?;
            /// if loaded {
            ///     println!("Cache loaded successfully");
            /// }
            /// ```
            #vis async fn load_from_cache(&self, path: impl ::core::convert::AsRef<::std::path::Path>) -> picante::PicanteResult<bool> {
                let ingredients = self.persistable_ingredients();
                picante::persist::load_cache(path, &self.runtime, &ingredients).await
            }

            /// Load this database's state from a cache file with custom options.
            ///
            /// This is a convenience wrapper around `picante::persist::load_cache_with_options`
            /// that automatically collects all ingredients from this database.
            ///
            /// Returns `Ok(true)` if the cache was loaded successfully, `Ok(false)` if there
            /// was no cache file or it was ignored due to corruption, or `Err(_)` on error.
            ///
            /// # Example
            /// ```ignore
            /// use picante::persist::{CacheLoadOptions, OnCorruptCache};
            ///
            /// let loaded = db.load_from_cache_with_options(
            ///     "cache.bin",
            ///     &CacheLoadOptions {
            ///         max_bytes: None,
            ///         on_corrupt: OnCorruptCache::Delete,
            ///     }
            /// ).await?;
            /// ```
            #vis async fn load_from_cache_with_options(
                &self,
                path: impl ::core::convert::AsRef<::std::path::Path>,
                options: &picante::persist::CacheLoadOptions,
            ) -> picante::PicanteResult<bool> {
                let ingredients = self.persistable_ingredients();
                picante::persist::load_cache_with_options(path, &self.runtime, &ingredients, options).await
            }

            // ===== Write-Ahead Log (WAL) Methods =====

            /// Replay a WAL file, applying incremental changes to the database.
            ///
            /// This is typically called after `load_from_cache` to apply changes
            /// that were recorded after the base snapshot was created.
            ///
            /// # Example
            ///
            /// ```ignore
            /// // Load base snapshot
            /// db.load_from_cache("cache.bin").await?;
            ///
            /// // Apply incremental changes from WAL
            /// let entries_applied = db.replay_wal("cache.wal").await?;
            /// println!("Replayed {} WAL entries", entries_applied);
            /// ```
            ///
            /// Returns the number of WAL entries applied.
            #vis async fn replay_wal(
                &self,
                path: impl ::core::convert::AsRef<::std::path::Path>,
            ) -> picante::PicanteResult<usize> {
                let ingredients = self.persistable_ingredients();
                picante::persist::replay_wal(path, &self.runtime, &ingredients).await
            }

            /// Append changes to a WAL file since the WAL's base revision.
            ///
            /// This collects all changes from ingredients that occurred after the
            /// base revision and appends them to the WAL.
            ///
            /// # Example
            ///
            /// ```ignore
            /// let mut wal = picante::wal::WalWriter::create("cache.wal", base_revision)?;
            /// let entries = db.append_to_wal(&mut wal).await?;
            /// wal.flush()?;
            /// println!("Appended {} entries to WAL", entries);
            /// ```
            ///
            /// Returns the number of entries appended.
            #vis async fn append_to_wal(
                &self,
                wal: &mut picante::wal::WalWriter,
            ) -> picante::PicanteResult<usize> {
                let ingredients = self.persistable_ingredients();
                picante::persist::append_to_wal(wal, &self.runtime, &ingredients).await
            }

            /// Compact a WAL by creating a new snapshot and discarding the old WAL.
            ///
            /// This creates a new snapshot at the current revision, deletes the old WAL,
            /// and optionally creates a new empty WAL file.
            ///
            /// # Example
            ///
            /// ```ignore
            /// // Compact the WAL and create a new empty one
            /// let new_revision = db.compact_wal(
            ///     "cache.bin",
            ///     "cache.wal",
            ///     true  // create new WAL after compaction
            /// ).await?;
            /// println!("Compacted WAL, new snapshot at revision {}", new_revision);
            /// ```
            ///
            /// Returns the revision of the new snapshot.
            #vis async fn compact_wal(
                &self,
                cache_path: impl ::core::convert::AsRef<::std::path::Path>,
                wal_path: impl ::core::convert::AsRef<::std::path::Path>,
                create_new_wal: bool,
            ) -> picante::PicanteResult<u64> {
                let ingredients = self.persistable_ingredients();
                picante::persist::compact_wal(
                    cache_path,
                    wal_path,
                    &self.runtime,
                    &ingredients,
                    &picante::persist::CacheSaveOptions::default(),
                    create_new_wal,
                ).await
            }

            /// Compact a WAL with custom save options.
            ///
            /// Like `compact_wal`, but allows specifying options for the snapshot creation.
            ///
            /// # Example
            ///
            /// ```ignore
            /// let new_revision = db.compact_wal_with_options(
            ///     "cache.bin",
            ///     "cache.wal",
            ///     &picante::persist::CacheSaveOptions {
            ///         max_bytes: Some(10_000_000),
            ///         max_records_per_section: None,
            ///         max_record_bytes: None,
            ///     },
            ///     true,
            /// ).await?;
            /// ```
            #vis async fn compact_wal_with_options(
                &self,
                cache_path: impl ::core::convert::AsRef<::std::path::Path>,
                wal_path: impl ::core::convert::AsRef<::std::path::Path>,
                options: &picante::persist::CacheSaveOptions,
                create_new_wal: bool,
            ) -> picante::PicanteResult<u64> {
                let ingredients = self.persistable_ingredients();
                picante::persist::compact_wal(
                    cache_path,
                    wal_path,
                    &self.runtime,
                    &ingredients,
                    options,
                    create_new_wal,
                ).await
            }
        }

        impl picante::HasRuntime for #db_name {
            fn runtime(&self) -> &picante::Runtime {
                &self.runtime
            }
        }

        impl picante::IngredientLookup for #db_name {
            fn ingredient(&self, kind: picante::QueryKindId) -> Option<&dyn picante::DynIngredient<Self>> {
                self.ingredients.ingredient(kind)
            }
        }

        #(#trait_impls)*

        #db_trait_def

        #snapshot_def
    };

    expanded.into()
}

fn parse_args(attr: TokenStream2) -> Result<DbArgs, String> {
    let mut out = DbArgs::default();

    if attr.is_empty() {
        return Ok(out);
    }

    let mut it = attr.into_iter().peekable();
    while let Some(tt) = it.next() {
        match tt {
            TokenTree::Ident(key) => {
                let Some(TokenTree::Group(group)) = it.next() else {
                    return Err(
                        "picante: expected `inputs(...)`, `interned(...)`, or `tracked(...)`"
                            .to_string(),
                    );
                };
                if group.delimiter() != Delimiter::Parenthesis {
                    return Err("picante: expected parentheses, e.g. `inputs(...)`".to_string());
                }

                let key_s = key.to_string();
                let items = parse_list(group.stream(), &key_s)?;

                match key_s.as_str() {
                    "inputs" | "input" => out.inputs.extend(items),
                    "interned" => out.interned.extend(items),
                    "tracked" => out.tracked.extend(items),
                    "db_trait" => {
                        if items.len() != 1 {
                            return Err(
                                "picante: `db_trait(...)` expects exactly one identifier"
                                    .to_string(),
                            );
                        }
                        let item = &items[0];
                        if !item.prefix.is_empty() {
                            return Err(
                                "picante: `db_trait(...)` expects a simple identifier, not a path"
                                    .to_string(),
                            );
                        }
                        if out.db_trait.is_some() {
                            return Err(
                                "picante: `db_trait(...)` specified multiple times".to_string()
                            );
                        }
                        out.db_trait = Some(item.name.clone());
                    }
                    _ => return Err("picante: unknown #[picante::db] key (expected `inputs`, `interned`, `tracked`, `db_trait`)".to_string()),
                }

                if let Some(TokenTree::Punct(p)) = it.peek()
                    && p.as_char() == ','
                {
                    it.next();
                }
            }
            TokenTree::Punct(p) if p.as_char() == ',' => {}
            _ => {
                return Err(
                    "picante: expected `inputs(...)`, `interned(...)`, or `tracked(...)`"
                        .to_string(),
                );
            }
        }
    }

    Ok(out)
}

fn parse_list(ts: TokenStream2, list_name: &str) -> Result<Vec<ItemPath>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::<String>::new();
    let mut current = TokenStream2::new();

    for tt in ts {
        if matches!(&tt, TokenTree::Punct(p) if p.as_char() == ',') {
            if !current.is_empty() {
                let normalized = normalize_tokens(&current);
                if !seen.insert(normalized.clone()) {
                    return Err(format!(
                        "picante: duplicate item `{}` in `{}` list",
                        normalized, list_name
                    ));
                }
                out.push(ItemPath::parse(current)?);
                current = TokenStream2::new();
            }
            continue;
        }
        current.extend(::core::iter::once(tt));
    }

    if !current.is_empty() {
        let normalized = normalize_tokens(&current);
        if !seen.insert(normalized.clone()) {
            return Err(format!(
                "picante: duplicate item `{}` in `{}` list",
                normalized, list_name
            ));
        }
        out.push(ItemPath::parse(current)?);
    }

    Ok(out)
}

fn normalize_tokens(ts: &TokenStream2) -> String {
    ts.to_string().split_whitespace().collect::<String>()
}
