use facet::Facet;
use facet_args as args;

#[test]
fn test_eq_long_solo() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        hello: String,
    }
    let args: Args = facet_args::from_slice(&["--hello=world"]).unwrap();
    assert_eq!(args.hello, "world".to_string());
}

#[test]
fn test_eq_short_solo() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::short)]
        k: i64,
    }
    let args: Args = facet_args::from_slice(&["-k=3"]).unwrap();
    assert_eq!(args.k, 3);
}

#[test]
fn test_eq_long_rename_solo() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named, rename = "cores")]
        concurrency: i64,
    }
    let args: Args = facet_args::from_slice(&["--cores=4"]).unwrap();
    assert_eq!(args.concurrency, 4);
}

#[test]
fn test_eq_short_rename_solo() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::short, rename = "j")]
        concurrency: i64,
    }
    let args: Args = facet_args::from_slice(&["-j=4"]).unwrap();
    assert_eq!(args.concurrency, 4);
}

#[test]
fn test_eq_long_followed() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        hello: String,
        #[facet(args::named)]
        two: i64,
    }
    let args: Args = facet_args::from_slice(&["--hello=world", "--two", "2"]).unwrap();
    assert_eq!(args.hello, "world".to_string());
    assert_eq!(args.two, 2);
}

#[test]
fn test_eq_short_followed() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::short)]
        y: String,
        #[facet(args::named)]
        two: i64,
    }
    let args: Args = facet_args::from_slice(&["-y=yes", "--two", "2"]).unwrap();
    assert_eq!(args.y, "yes".to_string());
    assert_eq!(args.two, 2);
}

#[test]
fn test_eq_long_preceded() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        one: i64,
        #[facet(args::named)]
        hello: String,
    }
    let args: Args = facet_args::from_slice(&["--one", "1", "--hello=world"]).unwrap();
    assert_eq!(args.one, 1);
    assert_eq!(args.hello, "world".to_string());
}

#[test]
fn test_eq_short_preceded() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        one: i64,
        #[facet(args::short)]
        y: String,
    }
    let args: Args = facet_args::from_slice(&["--one", "1", "-y=yes"]).unwrap();
    assert_eq!(args.one, 1);
    assert_eq!(args.y, "yes".to_string());
}

#[test]
fn test_eq_long_in_the_middle() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        one: i64,
        #[facet(args::named)]
        hello: String,
        #[facet(args::named)]
        two: i64,
    }
    let args: Args =
        facet_args::from_slice(&["--one", "1", "--hello=world", "--two", "2"]).unwrap();
    assert_eq!(args.one, 1);
    assert_eq!(args.hello, "world".to_string());
    assert_eq!(args.two, 2);
}

#[test]
fn test_eq_short_in_the_middle() {
    #[derive(Facet, Debug)]
    struct Args {
        #[facet(args::named)]
        one: i64,
        #[facet(args::short)]
        y: String,
        #[facet(args::named)]
        two: i64,
    }
    let args: Args = facet_args::from_slice(&["--one", "1", "-y=yes", "--two", "2"]).unwrap();
    assert_eq!(args.one, 1);
    assert_eq!(args.y, "yes".to_string());
    assert_eq!(args.two, 2);
}
