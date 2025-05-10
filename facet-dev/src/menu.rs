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
    None
}

#[cfg(not(windows))]
pub fn show_menu(question: &str, items: &[MenuItem]) -> Option<String> {
    // Check for environment variable to automatically accept
    if std::env::var("FACET_PRECOMMIT_ACCEPT_ALL").is_ok() {
        // If FACET_PRECOMMIT_ACCEPT_ALL is set, automatically return "apply"
        println!("FACET_PRECOMMIT_ACCEPT_ALL is set, automatically accepting changes");
        return Some("apply".to_string());
    }

    // Requires the 'termion' crate for raw input handling
    use std::io::{self, Write};
    use termion::event::{Event, Key};
    use termion::input::TermRead;
    use termion::raw::IntoRawMode;
    use yansi::Paint as _;

    use termion::color;
    println!("{}", question);
    for item in items.iter() {
        let label = &item.label;
        let mut chars = label.chars().peekable();
        let mut after_colon = false;
        while let Some(ch) = chars.next() {
            // Check for shortcut
            if ch == '[' {
                let mut shortcut = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ']' {
                        chars.next(); // consume ']'
                        print!("[{}]", shortcut.paint(item.style));
                        break;
                    } else {
                        shortcut.push(next_ch);
                        chars.next();
                    }
                }
                continue;
            }
            // Check for ':'
            if !after_colon && ch == ':' {
                after_colon = true;
                print!("{}", color::Fg(color::LightBlack));
                print!("{}", ch);
                continue;
            }
            // After colon? Dim color, otherwise normal
            if after_colon {
                print!("{}", ch.to_string().dim());
            } else {
                print!("{}", ch);
            }
        }
        if after_colon {
            print!("{}", color::Fg(color::Reset));
        }
        println!();
    }
    print!("Enter your choice: ");
    io::stdout().flush().unwrap();

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .unwrap();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let mut stdout = tty.try_clone().unwrap().into_raw_mode().unwrap();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let stdin = tty;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let mut stdout = stdout().into_raw_mode().unwrap();
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let stdin = stdin();
    let mut result = String::new();
    let mut selected_action = String::new();

    for evt in stdin.events() {
        let evt = evt.unwrap();
        match evt {
            Event::Key(Key::Char(c)) => {
                if c.is_ascii_digit() {
                    result.push(c);
                    write!(stdout, "{}", c).unwrap();
                    stdout.flush().unwrap();

                    if let Ok(idx) = result.parse::<usize>() {
                        if idx >= 1 && idx <= items.len() {
                            selected_action = items[idx - 1].action.clone();
                            break;
                        }
                    }
                } else {
                    // Check for shortcut key in label. Look for e.g. [Y]
                    let mut found = false;
                    for item in items {
                        if let Some(start) = item.label.find('[') {
                            if let Some(end) = item.label[start + 1..].find(']') {
                                let shortcut = &item.label[start + 1..start + 1 + end];
                                if shortcut.eq_ignore_ascii_case(&c.to_string()) {
                                    selected_action = item.action.clone();
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                    if found {
                        break;
                    }
                }
            }
            Event::Key(Key::Backspace) => {
                if !result.is_empty() {
                    result.pop();
                    write!(stdout, "\x08 \x08").unwrap(); // Erase last character
                    stdout.flush().unwrap();
                }
            }
            Event::Key(Key::Esc) => {
                // Allow cancelling with ESC
                break;
            }
            Event::Key(Key::Ctrl('c')) | Event::Key(Key::Ctrl('d')) => {
                // Allow cancelling with Ctrl-C or Ctrl-D
                break;
            }
            _ => {}
        }
    }

    if !selected_action.is_empty() {
        Some(selected_action)
    } else {
        None
    }
}
