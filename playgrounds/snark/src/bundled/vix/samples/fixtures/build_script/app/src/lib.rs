include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub fn message() -> &'static str {
    #[cfg(vix_slice3_build_script)]
    {
        GENERATED
    }
    #[cfg(not(vix_slice3_build_script))]
    {
        "missing-build-script-cfg"
    }
}
