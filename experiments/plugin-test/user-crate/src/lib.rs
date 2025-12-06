// First invoke plugin-a (which registers its codegen via ctor)
plugin_a::invoke_a!();

// Define a struct
#[derive(Debug)]
pub struct MyError {
    pub message: String,
}

// plugin-b generates Display and Error impls using plugin-a's codegen!
plugin_b::invoke_b!(MyError);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error() {
        let err = MyError {
            message: "oops".into(),
        };
        // This works because plugin-b generated Display impl
        assert_eq!(format!("{}", err), "Error: MyError");
        // This works because plugin-b generated Error impl
        let _: &dyn std::error::Error = &err;
    }
}
