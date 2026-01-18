//! Example table definitions for the dibs CLI.
//!
//! These tables are used to demonstrate and test dibs functionality.

use facet::Facet;

/// Multi-tenant organization or workspace.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "tenants")]
pub struct Tenant {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::unique)]
    pub slug: String,

    #[facet(dibs::index)]
    pub name: String,

    #[facet(dibs::default = "now()")]
    pub created_at: i64,
}

/// User accounts in the system.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "users")]
#[facet(dibs::composite_index(columns = "tenant_id,email"))]
pub struct User {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::unique)]
    pub email: String,

    #[facet(dibs::index)]
    pub name: String,

    pub bio: Option<String>,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    pub tenant_id: i64,

    #[facet(dibs::default = "now()", dibs::index = "idx_users_created")]
    pub created_at: i64,
}

/// Blog posts.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "posts")]
#[facet(dibs::composite_index(
    name = "idx_posts_tenant_published",
    columns = "tenant_id,published"
))]
pub struct Post {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::index)]
    pub title: String,

    pub body: String,

    pub published: bool,

    #[facet(dibs::fk = "users.id", dibs::index)]
    pub author_id: i64,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    pub tenant_id: i64,

    #[facet(dibs::default = "now()")]
    pub created_at: i64,

    pub updated_at: Option<i64>,
}

/// Comments on posts.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "comments")]
pub struct Comment {
    #[facet(dibs::pk)]
    pub id: i64,

    pub body: String,

    #[facet(dibs::fk = "posts.id", dibs::index)]
    pub post_id: i64,

    #[facet(dibs::fk = "users.id", dibs::index)]
    pub author_id: i64,

    #[facet(dibs::default = "now()")]
    pub created_at: i64,
}

/// Tags for categorizing posts.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "tags")]
pub struct Tag {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::unique)]
    pub name: String,

    #[facet(dibs::fk = "tenants.id", dibs::index)]
    pub tenant_id: i64,
}

/// Many-to-many relationship between posts and tags.
#[derive(Facet)]
#[facet(derive(dibs::Table), dibs::table = "post_tags")]
#[facet(dibs::composite_index(name = "idx_post_tags_unique", columns = "post_id,tag_id"))]
pub struct PostTag {
    #[facet(dibs::pk)]
    pub id: i64,

    #[facet(dibs::fk = "posts.id", dibs::index)]
    pub post_id: i64,

    #[facet(dibs::fk = "tags.id", dibs::index)]
    pub tag_id: i64,
}
