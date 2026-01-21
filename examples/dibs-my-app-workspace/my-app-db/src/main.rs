//! Database service binary for my-app (ecommerce).
//!
//! Usage:
//!   my-app-db          - Run the dibs service (connects back to CLI via roam)
//!   my-app-db seed     - Seed the database with sample data (~2000 products)

use my_app_db::{Product, ProductSource, ProductTranslation, ProductVariant, VariantPrice};
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio_postgres::NoTls;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Touch the types so they're not dead code eliminated
    let _ = (
        std::any::type_name::<Product>(),
        std::any::type_name::<ProductVariant>(),
        std::any::type_name::<VariantPrice>(),
        std::any::type_name::<ProductSource>(),
        std::any::type_name::<ProductTranslation>(),
    );

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "seed" {
        // Seed needs async runtime
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(seed())?;
    } else {
        // run_service creates its own runtime
        dibs::run_service();
    }

    Ok(())
}

// Product name components for generating realistic names
const ADJECTIVES: &[&str] = &[
    "Abstract",
    "Vintage",
    "Modern",
    "Classic",
    "Rustic",
    "Elegant",
    "Bold",
    "Serene",
    "Vibrant",
    "Minimal",
    "Cozy",
    "Artistic",
    "Natural",
    "Urban",
    "Coastal",
    "Mountain",
    "Forest",
    "Desert",
    "Tropical",
    "Nordic",
    "Japanese",
    "Bohemian",
    "Industrial",
    "Retro",
    "Contemporary",
    "Traditional",
    "Luxurious",
    "Simple",
    "Geometric",
    "Floral",
    "Watercolor",
    "Monochrome",
    "Colorful",
    "Pastel",
    "Earthy",
    "Celestial",
    "Mystical",
    "Whimsical",
    "Dramatic",
    "Peaceful",
    "Dynamic",
    "Harmonious",
    "Textured",
    "Smooth",
    "Raw",
    "Polished",
    "Handcrafted",
    "Artisan",
    "Premium",
    "Essential",
];

const SUBJECTS: &[&str] = &[
    "Sunset",
    "Sunrise",
    "Mountains",
    "Ocean",
    "Forest",
    "City",
    "Garden",
    "Sky",
    "Stars",
    "Moon",
    "Waves",
    "Trees",
    "Flowers",
    "Birds",
    "Landscape",
    "Portrait",
    "Still Life",
    "Architecture",
    "Bridge",
    "Tower",
    "Lighthouse",
    "Cabin",
    "Cottage",
    "Meadow",
    "Valley",
    "River",
    "Lake",
    "Waterfall",
    "Canyon",
    "Desert",
    "Beach",
    "Island",
    "Jungle",
    "Savanna",
    "Tundra",
    "Aurora",
    "Galaxy",
    "Nebula",
    "Planet",
    "Cosmos",
    "Horizon",
    "Silhouette",
    "Reflection",
    "Shadow",
    "Light",
    "Pattern",
    "Texture",
    "Lines",
    "Shapes",
    "Colors",
    "Dreams",
    "Memory",
    "Journey",
    "Adventure",
];

const PRODUCT_TYPES: &[(&str, &[&str], (f64, f64))] = &[
    (
        "Print",
        &[
            "Small (8x10)",
            "Medium (12x16)",
            "Large (18x24)",
            "XL (24x36)",
        ],
        (19.99, 89.99),
    ),
    (
        "Canvas",
        &[
            "Small (16x20)",
            "Medium (24x30)",
            "Large (30x40)",
            "XL (40x60)",
        ],
        (49.99, 199.99),
    ),
    (
        "Poster",
        &["Standard (18x24)", "Large (24x36)"],
        (14.99, 29.99),
    ),
    (
        "Framed Print",
        &["Small (8x10)", "Medium (12x16)", "Large (18x24)"],
        (39.99, 129.99),
    ),
    (
        "Metal Print",
        &["Small (12x12)", "Medium (16x20)", "Large (24x36)"],
        (59.99, 179.99),
    ),
    (
        "Acrylic",
        &["Small (12x12)", "Medium (16x20)", "Large (24x36)"],
        (69.99, 199.99),
    ),
    ("T-Shirt", &["S", "M", "L", "XL", "2XL"], (24.99, 29.99)),
    ("Hoodie", &["S", "M", "L", "XL", "2XL"], (44.99, 49.99)),
    ("Mug", &["11oz", "15oz"], (14.99, 17.99)),
    ("Tote Bag", &["Standard"], (19.99, 19.99)),
    (
        "Phone Case",
        &["iPhone 14", "iPhone 15", "Samsung S23", "Samsung S24"],
        (24.99, 29.99),
    ),
    ("Throw Pillow", &["16x16", "18x18", "20x20"], (29.99, 44.99)),
    ("Blanket", &["50x60", "60x80"], (54.99, 79.99)),
    (
        "Sticker",
        &["Small (2\")", "Medium (3\")", "Large (4\")"],
        (3.99, 6.99),
    ),
    ("Notebook", &["A5", "A4"], (12.99, 16.99)),
];

const VENDORS: &[&str] = &["printify", "gelato", "printful", "gooten"];

fn generate_handle(adj: &str, subject: &str, product_type: &str, index: usize) -> String {
    format!(
        "{}-{}-{}-{}",
        adj.to_lowercase().replace(' ', "-"),
        subject.to_lowercase().replace(' ', "-"),
        product_type.to_lowercase().replace(' ', "-"),
        index
    )
}

fn generate_sku(product_type: &str, product_id: i64, variant_idx: usize) -> String {
    let prefix: String = product_type
        .chars()
        .filter(|c| c.is_uppercase())
        .take(3)
        .collect();
    let prefix = if prefix.is_empty() {
        product_type
            .chars()
            .take(3)
            .collect::<String>()
            .to_uppercase()
    } else {
        prefix
    };
    format!("{}-{:05}-{:02}", prefix, product_id, variant_idx)
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0;
    for b in s.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u64);
    }
    h
}

async fn seed() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/dibs_test".to_string());

    println!("Seeding database: {}", database_url);
    println!();

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    // Clear existing data
    println!("Clearing existing data...");
    client
        .execute("DELETE FROM \"product_translation\"", &[])
        .await
        .ok();
    client
        .execute("DELETE FROM \"product_source\"", &[])
        .await
        .ok();
    client
        .execute("DELETE FROM \"variant_price\"", &[])
        .await
        .ok();
    client
        .execute("DELETE FROM \"product_variant\"", &[])
        .await
        .ok();
    client.execute("DELETE FROM \"product\"", &[]).await.ok();
    println!();

    let mut product_id: i64 = 0;
    let mut variant_id: i64 = 0;
    let mut price_id: i64 = 0;
    let mut source_id: i64 = 0;
    let mut translation_id: i64 = 0;

    let total_products = ADJECTIVES.len() * SUBJECTS.len(); // ~2500 products
    println!("Generating {} products...", total_products);

    let progress_interval = total_products / 20;

    for (adj_idx, adj) in ADJECTIVES.iter().enumerate() {
        for (subj_idx, subject) in SUBJECTS.iter().enumerate() {
            product_id += 1;

            // Pick a product type based on hash
            let type_idx =
                (simple_hash(&format!("{}{}", adj, subject)) as usize) % PRODUCT_TYPES.len();
            let (product_type, variants, (min_price, max_price)) = PRODUCT_TYPES[type_idx];

            let handle = generate_handle(adj, subject, product_type, product_id as usize);
            let status = if product_id % 10 == 0 {
                "draft"
            } else {
                "published"
            };
            let active = status == "published";

            // Insert product
            client
                .execute(
                    r#"INSERT INTO "product" (id, handle, status, active) VALUES ($1, $2, $3, $4)"#,
                    &[&product_id, &handle, &status, &active],
                )
                .await?;

            // Insert variants
            let price_step = if variants.len() > 1 {
                (max_price - min_price) / (variants.len() - 1) as f64
            } else {
                0.0
            };

            for (v_idx, variant_name) in variants.iter().enumerate() {
                variant_id += 1;
                let sku = generate_sku(product_type, product_id, v_idx);
                let title = format!("{} {} {} - {}", adj, subject, product_type, variant_name);

                client
                    .execute(
                        r#"INSERT INTO "product_variant" (id, product_id, sku, title, sort_order)
                           VALUES ($1, $2, $3, $4, $5)"#,
                        &[&variant_id, &product_id, &sku, &title, &(v_idx as i32)],
                    )
                    .await?;

                // Insert prices (EUR and USD)
                let base_price = min_price + (price_step * v_idx as f64);
                let eur_price = Decimal::from_str(&format!("{:.2}", base_price))?;
                let usd_price = Decimal::from_str(&format!("{:.2}", base_price * 1.1))?; // USD ~10% more

                for (currency, amount, region) in
                    [("EUR", eur_price, "EU"), ("USD", usd_price, "US")]
                {
                    price_id += 1;
                    client
                        .execute(
                            r#"INSERT INTO "variant_price" (id, variant_id, currency_code, amount, region)
                               VALUES ($1, $2, $3, $4, $5)"#,
                            &[&price_id, &variant_id, &currency, &amount, &Some(region)],
                        )
                        .await?;
                }
            }

            // Insert product source
            source_id += 1;
            let vendor = VENDORS[(simple_hash(&handle) as usize) % VENDORS.len()];
            let external_id = format!("{}_prod_{:08x}", &vendor[..3], simple_hash(&handle));
            client
                .execute(
                    r#"INSERT INTO "product_source" (id, product_id, vendor, external_id, last_synced_at)
                       VALUES ($1, $2, $3, $4, now() - interval '1 hour' * $5)"#,
                    &[&source_id, &product_id, &vendor, &external_id, &((product_id % 48) as f64)],
                )
                .await?;

            // Insert translations (English always, French/German for some)
            let title_en = format!("{} {} {}", adj, subject, product_type);
            let desc_en = format!(
                "Beautiful {} artwork featuring {} themes. Perfect for {} lovers. High-quality {} ready to ship.",
                adj.to_lowercase(),
                subject.to_lowercase(),
                subject.to_lowercase(),
                product_type.to_lowercase()
            );

            translation_id += 1;
            client
                .execute(
                    r#"INSERT INTO "product_translation" (id, product_id, locale, title, description)
                       VALUES ($1, $2, $3, $4, $5)"#,
                    &[&translation_id, &product_id, &"en", &title_en, &Some(&desc_en)],
                )
                .await?;

            // French for ~30% of products
            if product_id % 3 == 0 {
                translation_id += 1;
                let title_fr = format!("{} {} {}", adj, subject, product_type); // Simplified
                client
                    .execute(
                        r#"INSERT INTO "product_translation" (id, product_id, locale, title, description)
                           VALUES ($1, $2, $3, $4, NULL)"#,
                        &[&translation_id, &product_id, &"fr", &title_fr],
                    )
                    .await?;
            }

            // German for ~20% of products
            if product_id % 5 == 0 {
                translation_id += 1;
                let title_de = format!("{} {} {}", adj, subject, product_type);
                client
                    .execute(
                        r#"INSERT INTO "product_translation" (id, product_id, locale, title, description)
                           VALUES ($1, $2, $3, $4, NULL)"#,
                        &[&translation_id, &product_id, &"de", &title_de],
                    )
                    .await?;
            }

            // Progress indicator
            let current = adj_idx * SUBJECTS.len() + subj_idx + 1;
            if current.is_multiple_of(progress_interval) || current == total_products {
                let pct = (current * 100) / total_products;
                print!("\r  Progress: {}% ({}/{})", pct, current, total_products);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        }
    }
    println!();
    println!();

    // Summary
    println!("═══════════════════════════════════════════════════════");
    println!("Seeding complete!");
    println!("═══════════════════════════════════════════════════════");
    println!(
        "  {} products ({} published, {} draft)",
        product_id,
        product_id - product_id / 10,
        product_id / 10
    );
    println!("  {} variants", variant_id);
    println!("  {} prices", price_id);
    println!("  {} vendor sources", source_id);
    println!("  {} translations", translation_id);
    println!("═══════════════════════════════════════════════════════");

    Ok(())
}
