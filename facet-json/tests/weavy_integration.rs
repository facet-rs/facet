//! Weavy-backed replay of main integration tests that are currently supported
//! by the owned Weavy deserializer.

#[path = "integration/backend_weavy.rs"]
mod json_backend;

#[path = "integration/format_specific_proxy.rs"]
mod format_specific_proxy;
#[path = "integration/int_map_keys.rs"]
mod int_map_keys;
#[path = "integration/issue_1236.rs"]
mod issue_1236;
#[path = "integration/issue_1791.rs"]
mod issue_1791;
#[path = "integration/issue_1852.rs"]
mod issue_1852;
#[path = "integration/issue_1982.rs"]
mod issue_1982;
#[path = "integration/issue_1989.rs"]
mod issue_1989;
#[path = "integration/issue_2004.rs"]
mod issue_2004;
#[path = "integration/option_enum_test.rs"]
mod option_enum_test;
#[path = "integration/rename.rs"]
mod rename;
#[path = "integration/tendril.rs"]
mod tendril;
