#![cfg_attr(windows, allow(dead_code))]

use yansi::Style;

/// A menu item
pub struct MenuItem {
    /// may contain `[]`, like `[Y]es` — that's the shortcut key. pressing it immediately executes the action
    pub label: String,

    /// style for the action
    pub style: Style,

    /// returned by show_menu
    pub action: String,
}

#[cfg(windows)]
pub fn show_menu(question: &str, items: &[MenuItem]) -> Option<String> {
    _ = (question, items);
    // Always automatically apply changes
    println!("Automatically applying changes");
    Some("apply".to_string())
}

#[cfg(not(windows))]
pub fn show_menu(_question: &str, _items: &[MenuItem]) -> Option<String> {
    // Always automatically apply changes
    println!("Automatically applying changes");
    Some("apply".to_string())
}
