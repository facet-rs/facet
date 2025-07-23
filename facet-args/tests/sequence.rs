use facet::Facet;
use facet_testhelpers::test;

#[test]
#[ignore]
fn test_value_singleton_list() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "n")]
        numbers: Vec<u64>,

        #[facet(named, short = "s")]
        strings: Vec<String>,
    }

    // Test with a single value (no delimiter used)
    let args_single: Args = facet_args::from_slice(&["-n", "42"]).unwrap();

    assert_eq!(args_single.numbers, vec![42]);
}

#[test]
#[ignore]
fn test_value_singleton_lists_x2() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "n")]
        numbers: Vec<u64>,

        #[facet(named, short = "s")]
        strings: Vec<String>,
    }

    // Test with a single value (no delimiter used)
    let args_single: Args = facet_args::from_slice(&["-n", "42", "-s", "single"]).unwrap();

    assert_eq!(args_single.numbers, vec![42]);
    assert_eq!(args_single.strings, vec!["single".to_string()]);
}

#[test]
#[ignore]
fn test_value_delimiter_approach() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "n", delimiter = ",")]
        numbers: Vec<u64>,

        #[facet(named, short = "s", delimiter = ":")]
        strings: Vec<String>,

        #[facet(named, short = "v")]
        verbose: bool,
    }

    // Test with delimited values in a single argument
    let args: Args = facet_args::from_slice(&["-n", "1,2,3", "-s", "foo:bar:baz", "-v"]).unwrap();

    assert_eq!(args.numbers, vec![1, 2, 3]);
    assert_eq!(
        args.strings,
        vec!["foo".to_string(), "bar".to_string(), "baz".to_string()]
    );
    assert!(args.verbose);

    // Test with empty parts in delimited values
    let args_empty_parts: Args =
        facet_args::from_slice(&["-n", "1,,3", "-s", "start::end", "-v"]).unwrap();

    assert_eq!(args_empty_parts.numbers, vec![1, 0, 3]); // Empty part becomes 0 for numbers
    assert_eq!(
        args_empty_parts.strings,
        vec!["start".to_string(), "".to_string(), "end".to_string()]
    ); // Empty string for strings
    assert!(args_empty_parts.verbose);
}

#[test]
#[ignore]
fn test_repeated_flag_approach() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "n")]
        numbers: Vec<i64>,

        #[facet(named, short = "s")]
        strings: Vec<String>,

        #[facet(named, short = "v")]
        verbose: bool,
    }

    // Test with multiple flags providing multiple values
    let args: Args = facet_args::from_slice(&[
        "-n", "1", "-n", "2", "-n", "3", "-s", "foo", "-s", "bar", "-v",
    ])
    .unwrap();

    assert_eq!(args.numbers, vec![1, 2, 3]);
    assert_eq!(args.strings, vec!["foo".to_string(), "bar".to_string()]);
    assert!(args.verbose);

    // Test with an empty vector (no flags provided)
    let args_empty: Args = facet_args::from_slice(&["-v"]).unwrap();
    assert_eq!(args_empty.numbers, Vec::<i64>::new());
    assert_eq!(args_empty.strings, Vec::<String>::new());
    assert!(args_empty.verbose);
}

#[test]
#[ignore]
fn test_fixed_length_sequence() {
    #[derive(Facet, Debug, PartialEq)]
    struct Point {
        #[facet(named, sequence_size = 2)]
        coords: Vec<i32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "p", sequence_size = 2)]
        point: Vec<i32>,

        #[facet(named, short = "c", sequence_size = 3)]
        color: Vec<u8>,
    }

    // Test with exact number of values
    let args: Args = facet_args::from_slice(&["-p", "10", "20", "-c", "255", "0", "128"]).unwrap();

    assert_eq!(args.point, vec![10, 20]);
    assert_eq!(args.color, vec![255, 0, 128]);

    // Test with too few values (should fail)
    let args_too_few_point: Result<Args, _> = facet_args::from_slice(&[
        "-p", "10", // Missing one value
        "-c", "255", "0", "128",
    ]);
    assert!(args_too_few_point.is_err());

    // Test with too many values (should fail)
    let args_too_many_color: Result<Args, _> = facet_args::from_slice(&[
        "-p", "10", "20", "-c", "255", "0", "128", "255", // One value too many
    ]);
    assert!(args_too_many_color.is_err());

    // Test with nested struct
    #[derive(Facet, Debug, PartialEq)]
    struct ComplexArgs {
        #[facet(named)]
        point: Point,
    }

    let complex_args: ComplexArgs =
        facet_args::from_slice(&["--point", "--coords", "5", "10"]).unwrap();

    assert_eq!(complex_args.point.coords, vec![5, 10]);
}

#[test]
#[ignore]
#[allow(clippy::approx_constant)]
fn test_mixed_approaches() {
    #[derive(Facet, Debug, PartialEq)]
    struct Args {
        #[facet(named, short = "n", delimiter = ",")]
        delimiter_numbers: Vec<i64>,

        #[facet(named, short = "r")]
        repeated_numbers: Vec<i64>,

        #[facet(named, short = "f", sequence_size = 2)]
        fixed_point: Vec<f64>,
    }

    // Test combining all approaches
    let args: Args = facet_args::from_slice(&[
        "-n", "1,2,3", "-r", "10", "-r", "20", "-r", "30", "-f", "3.14", "2.71",
    ])
    .unwrap();

    assert_eq!(args.delimiter_numbers, vec![1, 2, 3]);
    assert_eq!(args.repeated_numbers, vec![10, 20, 30]);
    assert_eq!(args.fixed_point, vec![3.14, 2.71]);
}
