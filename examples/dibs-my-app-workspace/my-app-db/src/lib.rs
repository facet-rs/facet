//! Schema definitions for my-app (ecommerce example).
//!
//! This crate defines a minimal ecommerce schema using facet reflection.
//! Demonstrates:
//! - Products with required variants (MedusaJS pattern)
//! - Multi-currency pricing with NUMERIC/Decimal
//! - External vendor sync tracking
//! - Localized translations
//!
//! ## Naming Convention
//!
//! **Table names use singular form** (e.g., `product`, `variant`, `order`).
//!
//! This convention treats each table as a definition of what a single record
//! represents, rather than a container of multiple records.
//!
//! ## Money Storage: NUMERIC/Decimal vs Cents
//!
//! We store prices as `rust_decimal::Decimal` (Postgres `NUMERIC`) in their natural
//! form (e.g., `44.99` for $44.99), NOT as cents (4499).
//!
//! **Why this works:**
//! - `NUMERIC` is arbitrary-precision decimal, not floating point
//! - No rounding errors: `44.99` is stored exactly as `44.99`
//! - `rust_decimal::Decimal` maps cleanly to Postgres `NUMERIC`
//!
//! **When to use cents instead:**
//! - Payment APIs (Stripe, etc.) often expect cents - convert at the boundary
//! - If using regular integers (to avoid float issues) - but we have NUMERIC
//!
//! **Best practices:**
//! - Store in NUMERIC with the currency's natural precision (2 decimals for USD/EUR)
//! - Always store the currency code alongside the amount
//! - Convert to cents only when interfacing with payment processors

mod migrations;

use facet::Facet;
use rust_decimal::Decimal;

/// A product in the catalog.
///
/// Products are abstract containers - they don't have prices directly.
/// All pricing is on variants. A product must have at least one variant.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "product")]
#[facet(dibs::icon = "package")]
pub struct Product {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// URL-friendly handle (unique)
    #[facet(dibs::unique, dibs::subtype = "slug")]
    pub handle: String,

    /// Product status
    #[facet(dibs::default = "'draft'")]
    pub status: String, // 'draft', 'published', 'archived'

    /// Whether this product is active for sale
    #[facet(dibs::default = "true", dibs::icon = "toggle-right")]
    pub active: bool,

    /// Flexible metadata (JSON)
    pub metadata: Option<String>, // JSONB when we add support

    /// When the product was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the product was last updated
    #[facet(dibs::default = "now()")]
    pub updated_at: jiff::Timestamp,

    /// Soft delete timestamp
    pub deleted_at: Option<jiff::Timestamp>,
}

/// A variant of a product (e.g., size/color combination).
///
/// Every product must have at least one variant. Variants hold the actual
/// purchasable SKU, inventory tracking, and link to prices.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "product_variant")]
#[facet(dibs::icon = "layers")]
pub struct ProductVariant {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// Parent product
    #[facet(dibs::fk = "product.id")]
    pub product_id: i64,

    /// Stock Keeping Unit (unique identifier)
    #[facet(dibs::unique, dibs::subtype = "sku")]
    pub sku: String,

    /// Variant title (e.g., "Small / Blue")
    #[facet(dibs::label)]
    pub title: String,

    /// Variant attributes as JSON (e.g., {"size": "M", "color": "Blue"})
    pub attributes: Option<String>, // JSONB when we add support

    /// Whether to track inventory for this variant
    #[facet(dibs::default = "true")]
    pub manage_inventory: bool,

    /// Allow purchases when out of stock
    #[facet(dibs::default = "false")]
    pub allow_backorder: bool,

    /// Display order within product
    #[facet(dibs::default = "0")]
    pub sort_order: i32,

    /// When the variant was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When the variant was last updated
    #[facet(dibs::default = "now()")]
    pub updated_at: jiff::Timestamp,

    /// Soft delete timestamp
    pub deleted_at: Option<jiff::Timestamp>,
}

/// A price for a variant in a specific currency/region.
///
/// Variants can have multiple prices for different currencies or regions.
/// This enables multi-currency storefronts and regional pricing.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "variant_price")]
#[facet(dibs::icon = "coins")]
pub struct VariantPrice {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// The variant this price belongs to
    #[facet(dibs::fk = "product_variant.id")]
    pub variant_id: i64,

    /// Currency code (ISO 4217, e.g., "EUR", "USD")
    pub currency_code: String,

    /// Price amount (NUMERIC for precision)
    #[facet(dibs::subtype = "money")]
    pub amount: Decimal,

    /// Optional region for regional pricing (e.g., "EU", "US")
    pub region: Option<String>,

    /// When this price was created
    #[facet(dibs::default = "now()")]
    pub created_at: jiff::Timestamp,

    /// When this price was last updated
    #[facet(dibs::default = "now()")]
    pub updated_at: jiff::Timestamp,
}

/// Tracks where a product originated (external vendor sync).
///
/// When syncing products from vendors like Printify or Gelato, this table
/// links our internal product ID to the vendor's external ID.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "product_source")]
#[facet(dibs::icon = "cloud-download")]
pub struct ProductSource {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// The product this source links to
    #[facet(dibs::fk = "product.id")]
    pub product_id: i64,

    /// Vendor identifier (e.g., "printify", "gelato")
    pub vendor: String,

    /// External ID in the vendor's system
    pub external_id: String,

    /// When we last synced from this vendor
    pub last_synced_at: Option<jiff::Timestamp>,

    /// Raw data from vendor (for debugging/audit)
    #[facet(dibs::long)]
    pub raw_data: Option<String>, // JSONB when we add support
}

/// Localized product content (translations).
///
/// Each product can have translations for different locales.
/// The default/fallback content lives on the product itself or first translation.
#[derive(Debug, Clone, Facet)]
#[facet(derive(dibs::Table))]
#[facet(dibs::table = "product_translation")]
#[facet(dibs::icon = "languages")]
pub struct ProductTranslation {
    /// Primary key
    #[facet(dibs::pk)]
    pub id: i64,

    /// The product this translation belongs to
    #[facet(dibs::fk = "product.id")]
    pub product_id: i64,

    /// Locale code (e.g., "en", "fr", "de")
    pub locale: String,

    /// Translated product title
    #[facet(dibs::label)]
    pub title: String,

    /// Translated product description
    #[facet(dibs::lang = "markdown")]
    pub description: Option<String>,
}

/// Call this in build.rs to ensure inventory table submissions are linked.
///
/// Build scripts that use `dibs::build_queries` need to force the linker to
/// include this crate's inventory submissions.
pub fn ensure_linked() {}
