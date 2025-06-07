pub use chrono::{self, NaiveDate, NaiveDateTime};
pub use uuid::{self, Uuid};

#[cfg(feature = "facet")]
use facet::Facet;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The root struct representing the catalog of everything.
#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Catalog {
    pub id: Uuid,
    pub businesses: Vec<Business>,
    pub created_at: NaiveDateTime,
    pub metadata: CatalogMetadata,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct CatalogMetadata {
    pub version: String,
    pub region: String,
}

/// A business represented in the catalog.
#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Business {
    pub id: Uuid,
    pub name: String,
    pub address: Address,
    pub owner: BusinessOwner,
    pub users: Vec<BusinessUser>,
    pub branches: Vec<Branch>,
    pub products: Vec<Product>,
    pub created_at: NaiveDateTime,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct BusinessOwner {
    pub user: User,
    pub ownership_percent: f32,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Branch {
    pub id: Uuid,
    pub name: String,
    pub address: Address,
    pub employees: Vec<BusinessUser>,
    pub inventory: Vec<BranchInventory>,
    pub open: bool,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct BranchInventory {
    pub product: Product,
    pub stock: u32,
    pub location_code: Option<String>,
}

/// A user of the business
#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct BusinessUser {
    pub user: User,
    pub roles: Vec<Role>,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub profile: UserProfile,
    pub settings: Settings,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct UserProfile {
    pub first_name: String,
    pub last_name: String,
    pub date_of_birth: NaiveDate,
    pub gender: Gender,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub home_address: Address,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Address {
    pub street: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
    pub geo: Option<GeoLocation>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum Gender {
    Male,
    Female,
    Other,
    PreferNotToSay,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Product {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub price_cents: u64,
    pub currency: String,
    pub available: bool,
    pub metadata: Option<ProductMetadata>,
    pub reviews: Vec<ProductReview>,
    pub categories: Vec<Category>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct ProductMetadata {
    pub sku: Option<String>,
    pub categories: Vec<String>,
    pub weight_grams: Option<u32>,
    pub dimensions: Option<ProductDimensions>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct ProductDimensions {
    pub length_mm: Option<f32>,
    pub width_mm: Option<f32>,
    pub height_mm: Option<f32>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct ProductReview {
    pub id: Uuid,
    pub reviewer: UserSummary,
    pub rating: u8,
    pub text: Option<String>,
    pub created_at: NaiveDateTime,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent: Option<Box<Category>>,
}

/// Brief user reference (for lists)
#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct UserSummary {
    pub id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Permission {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub struct Settings {
    pub user_id: Uuid,
    pub email_notifications: bool,
    pub push_notifications: bool,
    pub theme: Theme,
    pub language: String,
}

#[cfg_attr(feature = "facet", derive(Facet))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
#[repr(u8)]
pub enum Theme {
    Light,
    Dark,
    System,
}
