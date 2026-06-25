//! Weavy-backed replay of main integration tests that are currently supported
//! by the owned Weavy deserializer.

#[path = "integration/backend_weavy.rs"]
mod json_backend;

#[path = "integration/flatten_in_externally_tagged_enum.rs"]
mod flatten_in_externally_tagged_enum;
#[path = "integration/format_specific_proxy.rs"]
mod format_specific_proxy;
#[path = "integration/int_map_keys.rs"]
mod int_map_keys;
#[path = "integration/issue_1236.rs"]
mod issue_1236;
#[path = "integration/issue_1721_1724.rs"]
mod issue_1721_1724;
#[path = "integration/issue_1775.rs"]
mod issue_1775;
#[path = "integration/issue_1791.rs"]
mod issue_1791;
#[path = "integration/issue_1852.rs"]
mod issue_1852;
#[path = "integration/issue_1896.rs"]
mod issue_1896;
#[path = "integration/issue_1900.rs"]
mod issue_1900;
#[path = "integration/issue_1904.rs"]
mod issue_1904;
#[path = "integration/issue_1982.rs"]
mod issue_1982;
#[path = "integration/issue_1987.rs"]
mod issue_1987;
#[path = "integration/issue_1989.rs"]
mod issue_1989;
#[path = "integration/issue_1990.rs"]
mod issue_1990;
#[path = "integration/issue_2004.rs"]
mod issue_2004;
#[path = "integration/issue_2007.rs"]
mod issue_2007;
#[path = "integration/issue_2010.rs"]
mod issue_2010;
#[path = "integration/issue_2059.rs"]
mod issue_2059;
#[path = "integration/list_deferred_processing.rs"]
mod list_deferred_processing;
#[path = "integration/metadata_container_flatten_map.rs"]
mod metadata_container_flatten_map;
#[path = "integration/mixed_tagged_untagged.rs"]
mod mixed_tagged_untagged;
#[path = "integration/nested_internal_tagging.rs"]
mod nested_internal_tagging;
#[path = "integration/option_enum_test.rs"]
mod option_enum_test;
#[path = "integration/rename.rs"]
mod rename;
#[path = "integration/tendril.rs"]
mod tendril;
