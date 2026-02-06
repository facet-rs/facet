use std::env::var_os;
use supports_color::Stream;

/// Determine if output should be colored.
///
/// This respects the [`NO_COLOR`](https://no-color.org) and [`FORCE_COLOR`] environment variables.
pub fn should_use_color() -> bool {
    // Don't use color when creating snapshots for `insta`. Color isn't disabled for testing in general
    // to allow coloring to be tested.
    var_os("INSTA_UPDATE").is_none()
        && var_os("INSTA_WORKSPACE").is_none()
        && var_os("INSTA_SNAPSHOT_UPDATE").is_none()
        && supports_color::on(Stream::Stdout).is_some()
}
