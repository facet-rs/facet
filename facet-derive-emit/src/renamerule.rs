/// Represents different case conversion strategies for renaming.
/// All strategies assume an initial input of `snake_case` (e.g., `foo_bar`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenameRule {
    /// Rename to PascalCase: `foo_bar` -> `FooBar`
    PascalCase,
    /// Rename to camelCase: `foo_bar` -> `fooBar`
    CamelCase,
    /// Rename to snake_case: `foo_bar` -> `foo_bar`
    SnakeCase,
    /// Rename to SCREAMING_SNAKE_CASE: `foo_bar` -> `FOO_BAR`
    ScreamingSnakeCase,
    /// Rename to kebab-case: `foo_bar` -> `foo-bar`
    KebabCase,
    /// Rename to SCREAMING-KEBAB-CASE: `foo_bar` -> `FOO-BAR`
    ScreamingKebabCase,
}

impl RenameRule {
    /// Parse a string into a `RenameRule`
    pub(crate) fn from_str(rule: &str) -> Option<Self> {
        match rule {
            "PascalCase" => Some(RenameRule::PascalCase),
            "camelCase" => Some(RenameRule::CamelCase),
            "snake_case" => Some(RenameRule::SnakeCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameRule::ScreamingSnakeCase),
            "kebab-case" => Some(RenameRule::KebabCase),
            "SCREAMING-KEBAB-CASE" => Some(RenameRule::ScreamingKebabCase),
            _ => None,
        }
    }

    /// Apply this renaming rule to a string
    pub(crate) fn apply(self, input: &str) -> String {
        match self {
            RenameRule::PascalCase => to_pascal_case(input),
            RenameRule::CamelCase => to_camel_case(input),
            RenameRule::SnakeCase => to_snake_case(input),
            RenameRule::ScreamingSnakeCase => to_screaming_snake_case(input),
            RenameRule::KebabCase => to_kebab_case(input),
            RenameRule::ScreamingKebabCase => to_screaming_kebab_case(input),
        }
    }
}

/// Converts a string to PascalCase: `foo_bar` -> `FooBar`
fn to_pascal_case(input: &str) -> String {
    split_into_words(input)
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    c.to_uppercase().collect::<String>() + &chars.collect::<String>().to_lowercase()
                }
            }
        })
        .collect()
}

/// Converts a string to camelCase: `foo_bar` -> `fooBar`
fn to_camel_case(input: &str) -> String {
    let pascal = to_pascal_case(input);
    if pascal.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    let mut chars = pascal.chars();
    if let Some(first_char) = chars.next() {
        result.push(first_char.to_lowercase().next().unwrap());
    }
    result.extend(chars);
    result
}

/// Converts a string to snake_case: `FooBar` -> `foo_bar`
fn to_snake_case(input: &str) -> String {
    let words = split_into_words(input);
    words
        .iter()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Converts a string to SCREAMING_SNAKE_CASE: `FooBar` -> `FOO_BAR`
fn to_screaming_snake_case(input: &str) -> String {
    let words = split_into_words(input);
    words
        .iter()
        .map(|word| word.to_uppercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// Converts a string to kebab-case: `FooBar` -> `foo-bar`
fn to_kebab_case(input: &str) -> String {
    let words = split_into_words(input);
    words
        .iter()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Converts a string to SCREAMING-KEBAB-CASE: `FooBar` -> `FOO-BAR`
fn to_screaming_kebab_case(input: &str) -> String {
    let words = split_into_words(input);
    words
        .iter()
        .map(|word| word.to_uppercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Splits a string into words based on case and separators
///
/// Logic:
/// - Iterates through characters in the input string.
/// - Splits at underscores, hyphens, or whitespace.
/// - Starts a new word on case boundaries, e.g. between lowercase and uppercase (as in "fooBar").
/// - Handles consecutive uppercase letters correctly (e.g. "HTTPServer").
/// - Aggregates non-separator characters into words.
/// - Returns a vector of non-empty words as Strings.
fn split_into_words(input: &str) -> Vec<String> {
    if input.is_empty() {
        return vec![];
    }

    let mut words = Vec::new();
    let mut current_word = String::new();
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        // If separator, start new word
        if c == '_' || c == '-' || c.is_whitespace() {
            if !current_word.is_empty() {
                words.push(std::mem::take(&mut current_word));
            }
            continue;
        }

        // Peek at next character for deciding about word boundaries
        let next = chars.peek().copied();

        if c.is_uppercase() {
            if !current_word.is_empty() {
                let prev = current_word.chars().last().unwrap();
                // Both cases should take the same action, so fold them together.
                // Case 1: previous is lowercase or digit, now uppercase (e.g. fooBar, foo1Bar)
                // Case 2: end of consecutive uppercase group, e.g. "BARBaz"
                // (prev is uppercase and next char is lowercase)
                if prev.is_lowercase()
                    || prev.is_ascii_digit()
                    || (prev.is_uppercase() && next.map(|n| n.is_lowercase()).unwrap_or(false))
                {
                    words.push(std::mem::take(&mut current_word));
                }
            }
            current_word.push(c);
        } else {
            // Lowercase or digit, just append
            // If previous is uppercase and next is lowercase, need to split, but handled above
            current_word.push(c);
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words.into_iter().filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::split_into_words;

    #[test]
    fn test_split_into_words_simple_snake_case() {
        assert_eq!(split_into_words("foo_bar_baz"), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_split_into_words_single_word() {
        assert_eq!(split_into_words("foo"), vec!["foo"]);
        assert_eq!(split_into_words("Foo"), vec!["Foo"]);
    }

    #[test]
    fn test_split_into_words_empty_string() {
        assert_eq!(split_into_words(""), Vec::<String>::new());
    }

    #[test]
    fn test_split_into_words_multiple_underscores() {
        assert_eq!(split_into_words("foo__bar"), vec!["foo", "bar"]);
        assert_eq!(split_into_words("_foo_bar_"), vec!["foo", "bar"]);
    }

    #[test]
    fn test_split_into_words_kebab_case() {
        assert_eq!(split_into_words("foo-bar-baz"), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_split_into_words_mixed_separators_and_space() {
        assert_eq!(split_into_words("foo_ bar-baz"), vec!["foo", "bar", "baz"]);
        assert_eq!(split_into_words("a_b-c d"), vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn test_split_into_words_camel_case() {
        assert_eq!(split_into_words("fooBarBaz"), vec!["foo", "Bar", "Baz"]);
        assert_eq!(split_into_words("fooBar"), vec!["foo", "Bar"]);
        assert_eq!(
            split_into_words("fooBar_BazQuux"),
            vec!["foo", "Bar", "Baz", "Quux"]
        );
    }

    #[test]
    fn test_split_into_words_pascal_case() {
        assert_eq!(split_into_words("FooBarBaz"), vec!["Foo", "Bar", "Baz"]);
        assert_eq!(split_into_words("FooBar"), vec!["Foo", "Bar"]);
    }

    #[test]
    fn test_split_into_words_http_server() {
        assert_eq!(split_into_words("HTTPServer"), vec!["HTTP", "Server"]);
        assert_eq!(
            split_into_words("theHTTPServer"),
            vec!["the", "HTTP", "Server"]
        );
    }

    #[test]
    fn test_split_into_words_consecutive_uppercase_at_end() {
        assert_eq!(split_into_words("FooBAR"), vec!["Foo", "BAR"]);
        assert_eq!(split_into_words("FooBARBaz"), vec!["Foo", "BAR", "Baz"]);
    }

    #[test]
    fn test_split_into_words_separators_and_case_boundaries() {
        assert_eq!(split_into_words("foo_barBaz"), vec!["foo", "bar", "Baz"]);
        assert_eq!(
            split_into_words("fooBar_bazQux"),
            vec!["foo", "Bar", "baz", "Qux"]
        );
    }

    #[test]
    fn test_rename_rule_snake_case() {
        use super::RenameRule;
        // Snake case input should remain unchanged
        assert_eq!(RenameRule::SnakeCase.apply("foo_bar_baz"), "foo_bar_baz");
        // CamelCase input becomes snake_case
        assert_eq!(RenameRule::SnakeCase.apply("fooBarBaz"), "foo_bar_baz");
        // PascalCase input becomes snake_case
        assert_eq!(RenameRule::SnakeCase.apply("FooBarBaz"), "foo_bar_baz");
        // SCREAMING_SNAKE_CASE input becomes snake_case
        assert_eq!(RenameRule::SnakeCase.apply("FOO_BAR_BAZ"), "foo_bar_baz");
        // kebab-case input becomes snake_case
        assert_eq!(RenameRule::SnakeCase.apply("foo-bar-baz"), "foo_bar_baz");
        assert_eq!(
            RenameRule::SnakeCase.apply("Foo_Bar-Baz quux"),
            "foo_bar_baz_quux"
        );
        // Mixed case and separator input
        assert_eq!(
            RenameRule::SnakeCase.apply("theHTTPServer"),
            "the_http_server"
        );
        assert_eq!(RenameRule::SnakeCase.apply("FooBARBaz"), "foo_bar_baz");
        // Empty input keeps empty
        assert_eq!(RenameRule::SnakeCase.apply(""), "");
    }
}
