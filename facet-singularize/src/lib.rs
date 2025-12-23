//! Fast, no-regex English singularization.
//!
//! This crate provides functions to convert plural English words to their singular form,
//! without using regex. It's designed for use in deserialization where performance matters.
//!
//! # Example
//!
//! ```
//! use facet_singularize::singularize;
//!
//! assert_eq!(singularize("dependencies"), "dependency");
//! assert_eq!(singularize("items"), "item");
//! assert_eq!(singularize("children"), "child");
//! assert_eq!(singularize("boxes"), "box");
//! ```
//!
//! # Performance
//!
//! This crate uses simple string operations (suffix matching, table lookups) instead of
//! regex, making it suitable for hot paths like deserialization.

#![no_std]
#![warn(missing_docs)]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::string::String;

mod ie_exceptions;

/// Irregular plural → singular mappings.
///
/// These are common English words where the plural form doesn't follow standard rules.
/// The list is sorted alphabetically by plural for binary search.
static IRREGULARS: &[(&str, &str)] = &[
    ("analyses", "analysis"),
    ("axes", "axis"),
    ("bases", "basis"),
    ("cacti", "cactus"),
    ("children", "child"),
    ("crises", "crisis"),
    ("criteria", "criterion"),
    ("curricula", "curriculum"),
    ("data", "datum"),
    ("diagnoses", "diagnosis"),
    ("dice", "die"),
    ("ellipses", "ellipsis"),
    ("feet", "foot"),
    ("foci", "focus"),
    ("formulae", "formula"),
    ("fungi", "fungus"),
    ("geese", "goose"),
    ("genera", "genus"),
    ("hypotheses", "hypothesis"),
    ("indices", "index"),
    ("larvae", "larva"),
    ("lice", "louse"),
    ("matrices", "matrix"),
    ("media", "medium"),
    ("memoranda", "memorandum"),
    ("men", "man"),
    ("mice", "mouse"),
    ("nebulae", "nebula"),
    ("nuclei", "nucleus"),
    ("oases", "oasis"),
    ("octopi", "octopus"),
    ("oxen", "ox"),
    ("parentheses", "parenthesis"),
    ("people", "person"),
    ("phenomena", "phenomenon"),
    ("radii", "radius"),
    ("stimuli", "stimulus"),
    ("strata", "stratum"),
    ("syllabi", "syllabus"),
    ("synopses", "synopsis"),
    ("teeth", "tooth"),
    ("theses", "thesis"),
    ("vertebrae", "vertebra"),
    ("vertices", "vertex"),
    ("women", "woman"),
];

/// Words that are the same in singular and plural form.
static UNCOUNTABLE: &[&str] = &[
    "aircraft",
    "bison",
    "buffalo",
    "deer",
    "equipment",
    "fish",
    "furniture",
    "information",
    "machinery",
    "moose",
    "news",
    "rice",
    "salmon",
    "series",
    "sheep",
    "shrimp",
    "software",
    "species",
    "swine",
    "trout",
    "tuna",
];

/// Convert a plural English word to its singular form.
///
/// This function handles:
/// - Irregular plurals (children → child, people → person, etc.)
/// - Uncountable nouns (sheep, fish, etc.) - returned unchanged
/// - Standard suffix rules:
///   - `-ies` → `-y` (dependencies → dependency)
///   - `-ves` → `-f` or `-fe` (wolves → wolf, knives → knife)
///   - `-es` → remove `-es` for words ending in s, x, z, ch, sh (boxes → box)
///   - `-s` → remove `-s` (items → item)
///
/// # Examples
///
/// ```
/// use facet_singularize::singularize;
///
/// // Irregular
/// assert_eq!(singularize("children"), "child");
/// assert_eq!(singularize("people"), "person");
/// assert_eq!(singularize("mice"), "mouse");
///
/// // Standard rules
/// assert_eq!(singularize("dependencies"), "dependency");
/// assert_eq!(singularize("boxes"), "box");
/// assert_eq!(singularize("items"), "item");
/// assert_eq!(singularize("wolves"), "wolf");
///
/// // Uncountable (unchanged)
/// assert_eq!(singularize("sheep"), "sheep");
/// assert_eq!(singularize("fish"), "fish");
/// ```
#[cfg(feature = "alloc")]
pub fn singularize(word: &str) -> String {
    // Check irregulars first (binary search since list is sorted)
    if let Ok(idx) = IRREGULARS.binary_search_by_key(&word, |&(plural, _)| plural) {
        return String::from(IRREGULARS[idx].1);
    }

    // Check uncountable
    if UNCOUNTABLE.binary_search(&word).is_ok() {
        return String::from(word);
    }

    // Apply suffix rules
    if let Some(singular) = try_singularize_suffix(word) {
        return singular;
    }

    // No rule matched, return as-is
    String::from(word)
}

/// Check if a singular word could be the singular form of a plural word.
///
/// This is useful for matching node names to field names in deserialization:
/// - `is_singular_of("dependency", "dependencies")` → `true`
/// - `is_singular_of("child", "children")` → `true`
/// - `is_singular_of("item", "items")` → `true`
///
/// This function is allocation-free when possible.
pub fn is_singular_of(singular: &str, plural: &str) -> bool {
    // Exact match (for uncountable or same word)
    if singular == plural {
        return true;
    }

    // Check irregulars - search by plural, compare singular
    if let Ok(idx) = IRREGULARS.binary_search_by_key(&plural, |&(p, _)| p) {
        return IRREGULARS[idx].1 == singular;
    }

    // Check uncountable
    if UNCOUNTABLE.binary_search(&plural).is_ok() {
        return singular == plural;
    }

    // Check suffix rules without allocation
    is_singular_of_by_suffix(singular, plural)
}

/// Try to singularize using suffix rules, returning None if no rule matches.
#[cfg(feature = "alloc")]
fn try_singularize_suffix(word: &str) -> Option<String> {
    let len = word.len();

    // Need at least 2 characters
    if len < 2 {
        return None;
    }

    // -ies → -y (but not -eies, -aies which become -ey, -ay)
    if len > 3 && word.ends_with("ies") {
        if ie_exceptions::contains(word) {
            let prefix = &word[..len - 3];
            return Some(alloc::format!("{prefix}ie"));
        }
        let prefix = &word[..len - 3];
        // Common -ie base words are handled via the exception list.
        let last_char = prefix.chars().last()?;
        if !matches!(last_char, 'a' | 'e' | 'o' | 'u') {
            return Some(alloc::format!("{prefix}y"));
        }
    }

    // -ves → -f or -fe
    if len > 3 && word.ends_with("ves") {
        let prefix = &word[..len - 3];
        // Common -ves → -fe patterns: knives→knife, wives→wife, lives→life
        if matches!(prefix, "kni" | "wi" | "li") {
            return Some(alloc::format!("{prefix}fe"));
        }
        // -eaves → -eaf (leaves→leaf, sheaves→sheaf)
        if prefix.ends_with("ea") {
            return Some(alloc::format!("{prefix}f"));
        }
        // -oaves → -oaf (loaves→loaf)
        if prefix.ends_with("oa") {
            return Some(alloc::format!("{prefix}f"));
        }
        // -alves → -alf (halves→half, calves→calf)
        if prefix.ends_with("al") {
            return Some(alloc::format!("{prefix}f"));
        }
        // -elves → -elf (shelves→shelf, selves→self, elves→elf)
        if prefix.ends_with("el") || prefix == "el" {
            return Some(alloc::format!("{prefix}f"));
        }
        // -olves → -olf (wolves→wolf)
        if prefix.ends_with("ol") {
            return Some(alloc::format!("{prefix}f"));
        }
        // Default: -ves → -f (might not be correct for all words)
        return Some(alloc::format!("{prefix}f"));
    }

    // -es → remove for sibilants (s, x, z, ch, sh)
    if len > 2 && word.ends_with("es") {
        let prefix = &word[..len - 2];

        // -zzes → -z (quizzes→quiz, fizzes→fiz)
        if prefix.ends_with("zz") {
            return Some(String::from(&prefix[..prefix.len() - 1]));
        }
        // -sses → -ss (classes→class, but also masses→mass)
        // However "classes" should become "class", so we keep the double s
        if prefix.ends_with("ss") {
            return Some(String::from(prefix));
        }

        if prefix.ends_with('s')
            || prefix.ends_with('x')
            || prefix.ends_with('z')
            || prefix.ends_with("ch")
            || prefix.ends_with("sh")
        {
            return Some(String::from(prefix));
        }
        // -oes → -o for some words (heroes→hero, potatoes→potato)
        if prefix.ends_with('o') {
            return Some(String::from(prefix));
        }
    }

    // -s → remove (most common case, check last)
    if word.ends_with('s') && !word.ends_with("ss") {
        let prefix = &word[..len - 1];
        if !prefix.is_empty() {
            return Some(String::from(prefix));
        }
    }

    None
}

/// Check if singular matches plural by suffix rules, without allocation.
fn is_singular_of_by_suffix(singular: &str, plural: &str) -> bool {
    let s_len = singular.len();
    let p_len = plural.len();

    // -ies → -ie (exception list)
    if p_len == s_len + 1 && plural.ends_with("ies") && singular.ends_with("ie") {
        if ie_exceptions::contains(plural) {
            return plural[..p_len - 3] == singular[..s_len - 2];
        }
    }

    // -ies → -y
    if p_len == s_len + 2 && plural.ends_with("ies") && singular.ends_with('y') {
        return plural[..p_len - 3] == singular[..s_len - 1];
    }

    // -ves → -f
    if p_len == s_len + 2 && plural.ends_with("ves") && singular.ends_with('f') {
        return plural[..p_len - 3] == singular[..s_len - 1];
    }

    // -ves → -fe
    if p_len == s_len + 1 && plural.ends_with("ves") && singular.ends_with("fe") {
        return plural[..p_len - 3] == singular[..s_len - 2];
    }

    // -es → remove (for sibilants)
    if p_len == s_len + 2 && plural.ends_with("es") && &plural[..p_len - 2] == singular {
        // Check singular ends with sibilant
        return singular.ends_with('s')
            || singular.ends_with('x')
            || singular.ends_with('z')
            || singular.ends_with("ch")
            || singular.ends_with("sh")
            || singular.ends_with('o');
    }

    // -s → remove
    if p_len == s_len + 1 && plural.ends_with('s') && !plural.ends_with("ss") {
        return &plural[..p_len - 1] == singular;
    }

    // Exact match (uncountable that wasn't in our list)
    singular == plural
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irregulars() {
        assert_eq!(singularize("children"), "child");
        assert_eq!(singularize("people"), "person");
        assert_eq!(singularize("mice"), "mouse");
        assert_eq!(singularize("feet"), "foot");
        assert_eq!(singularize("teeth"), "tooth");
        assert_eq!(singularize("geese"), "goose");
        assert_eq!(singularize("men"), "man");
        assert_eq!(singularize("women"), "woman");
        assert_eq!(singularize("oxen"), "ox");
        assert_eq!(singularize("dice"), "die");
        assert_eq!(singularize("indices"), "index");
        assert_eq!(singularize("vertices"), "vertex");
        assert_eq!(singularize("matrices"), "matrix");
        assert_eq!(singularize("criteria"), "criterion");
        assert_eq!(singularize("phenomena"), "phenomenon");
        assert_eq!(singularize("data"), "datum");
        assert_eq!(singularize("media"), "medium");
    }

    #[test]
    fn test_ie_plurals() {
        assert_eq!(singularize("movies"), "movie");
        assert_eq!(singularize("cookies"), "cookie");
        assert_eq!(singularize("pies"), "pie");
        assert_eq!(singularize("ties"), "tie");
        assert_eq!(singularize("brownies"), "brownie");
        assert_eq!(singularize("rookies"), "rookie");
        assert_eq!(singularize("selfies"), "selfie");
    }

    #[test]
    fn test_uncountable() {
        assert_eq!(singularize("sheep"), "sheep");
        assert_eq!(singularize("fish"), "fish");
        assert_eq!(singularize("deer"), "deer");
        assert_eq!(singularize("moose"), "moose");
        assert_eq!(singularize("series"), "series");
        assert_eq!(singularize("species"), "species");
        assert_eq!(singularize("news"), "news");
        assert_eq!(singularize("software"), "software");
    }

    #[test]
    fn test_ies_to_y() {
        assert_eq!(singularize("dependencies"), "dependency");
        assert_eq!(singularize("categories"), "category");
        assert_eq!(singularize("stories"), "story");
        assert_eq!(singularize("cities"), "city");
        assert_eq!(singularize("parties"), "party");
        assert_eq!(singularize("queries"), "query");
        assert_eq!(singularize("policies"), "policy");
        assert_eq!(singularize("ponies"), "pony");
        assert_eq!(singularize("babies"), "baby");
    }

    #[test]
    fn test_ves_to_f() {
        assert_eq!(singularize("wolves"), "wolf");
        assert_eq!(singularize("halves"), "half");
        assert_eq!(singularize("shelves"), "shelf");
        assert_eq!(singularize("leaves"), "leaf");
        assert_eq!(singularize("calves"), "calf");
    }

    #[test]
    fn test_ves_to_fe() {
        assert_eq!(singularize("knives"), "knife");
        assert_eq!(singularize("wives"), "wife");
        assert_eq!(singularize("lives"), "life");
    }

    #[test]
    fn test_es_sibilants() {
        assert_eq!(singularize("boxes"), "box");
        assert_eq!(singularize("matches"), "match");
        assert_eq!(singularize("watches"), "watch");
        assert_eq!(singularize("dishes"), "dish");
        assert_eq!(singularize("bushes"), "bush");
        assert_eq!(singularize("classes"), "class");
        assert_eq!(singularize("buses"), "bus");
        assert_eq!(singularize("quizzes"), "quiz");
    }

    #[test]
    fn test_oes_to_o() {
        assert_eq!(singularize("heroes"), "hero");
        assert_eq!(singularize("potatoes"), "potato");
        assert_eq!(singularize("tomatoes"), "tomato");
        assert_eq!(singularize("echoes"), "echo");
    }

    #[test]
    fn test_simple_s() {
        assert_eq!(singularize("items"), "item");
        assert_eq!(singularize("samples"), "sample");
        assert_eq!(singularize("users"), "user");
        assert_eq!(singularize("configs"), "config");
        assert_eq!(singularize("servers"), "server");
        assert_eq!(singularize("handlers"), "handler");
    }

    #[test]
    fn test_is_singular_of() {
        // Irregulars
        assert!(is_singular_of("child", "children"));
        assert!(is_singular_of("person", "people"));
        assert!(is_singular_of("mouse", "mice"));

        // Standard rules
        assert!(is_singular_of("dependency", "dependencies"));
        assert!(is_singular_of("box", "boxes"));
        assert!(is_singular_of("item", "items"));
        assert!(is_singular_of("wolf", "wolves"));
        assert!(is_singular_of("knife", "knives"));
        assert!(is_singular_of("movie", "movies"));
        assert!(is_singular_of("cookie", "cookies"));
        assert!(is_singular_of("pie", "pies"));
        assert!(is_singular_of("tie", "ties"));

        // Uncountable
        assert!(is_singular_of("sheep", "sheep"));
        assert!(is_singular_of("fish", "fish"));

        // Non-matches
        assert!(!is_singular_of("cat", "dogs"));
        assert!(!is_singular_of("dependency", "items"));
    }

    #[test]
    fn test_already_singular() {
        // Words that don't end in common plural suffixes should be returned as-is
        assert_eq!(singularize("config"), "config");
        assert_eq!(singularize("item"), "item");
    }
}
