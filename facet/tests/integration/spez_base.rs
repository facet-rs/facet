use core::fmt::{Debug, Display};

#[test]
fn test_spez1() {
    struct Wrap<T>(T);

    trait ViaString {
        fn foo(&self);
    }
    impl ViaString for &&Wrap<String> {
        fn foo(&self) {
            println!("String: {}", self.0);
        }
    }

    trait ViaDisplay {
        fn foo(&self);
    }
    impl<T: Display> ViaDisplay for &Wrap<T> {
        fn foo(&self) {
            println!("Display: {}", self.0);
        }
    }

    trait ViaDebug {
        fn foo(&self);
    }
    impl<T: Debug> ViaDebug for Wrap<T> {
        fn foo(&self) {
            println!("Debug: {:?}", self.0);
        }
    }

    // Test method calls
    (&&&Wrap(String::from("hi"))).foo();
    (&&Wrap(3)).foo();
    Wrap(['a', 'b']).foo();
}

#[test]
fn test_spez2() {
    struct Wrap<T>(T);

    trait DefaultYes {
        fn default_fn(&self, _unrelated: u32);
    }
    impl<T: Default + Debug> DefaultYes for Wrap<T> {
        fn default_fn(&self, _unrelated: u32) {
            println!("DefaultYes: {:?}", T::default());
        }
    }

    trait DefaultNo {
        fn default_fn(&self, unrelated: u32);
    }
    impl<T> DefaultNo for &Wrap<T> {
        fn default_fn(&self, _unrelated: u32) {
            println!("DefaultNo");
        }
    }

    struct NoDefaultHere;

    #[allow(clippy::needless_borrow)]
    (&Wrap(String::from("hi"))).default_fn(238);
    #[allow(clippy::needless_borrow)]
    (&Wrap(NoDefaultHere)).default_fn(238);

    let v: i32 = 1;
    #[allow(clippy::needless_borrow)]
    (&Wrap(v)).default_fn(238);
}
