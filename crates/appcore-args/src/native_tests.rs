// =============================================================================
//        #######
//     ###       ###     F: native_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/05 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/05 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2
// =============================================================================

//! Regression coverage for native argv and declarative source resolution.
// appcore-norm: test
use crate::*;
use std::ffi::OsString;

#[test]
fn argv_env_default_precedence_and_required_resolution() {
    let spec = CliSpec::new("node")
        .option(
            OptionSpec::value("bind")
                .env("NODE_BIND")
                .default_value("127.0.0.1:9000")
                .value_kind(ValueKind::SocketAddr)
                .required(true),
        )
        .option(
            OptionSpec::flag("once")
                .env("NODE_ONCE")
                .default_value("false"),
        );
    let parser = CliParser::new(&spec);
    let args = RawOsArgs::parse(["--bind", "[::1]:8000"].map(OsString::from)).unwrap();
    let parsed = parser
        .parse_os_with_env(&args, |name| {
            Some(OsString::from(if name == "NODE_ONCE" {
                "true"
            } else {
                "bad-address"
            }))
        })
        .unwrap();
    assert_eq!(
        parsed.option_socket_addr("bind").unwrap(),
        Some("[::1]:8000".parse().unwrap())
    );
    assert!(parsed.has_flag("once"));
    let empty = RawOsArgs::parse([]).unwrap();
    let env = parser
        .parse_os_with_env(&empty, |name| {
            (name == "NODE_BIND").then(|| "127.0.0.1:7000".into())
        })
        .unwrap();
    assert_eq!(env.option_value("bind").unwrap(), "127.0.0.1:7000");
    assert!(!env.has_flag("once"));
    let defaults = parser.parse_os_with_env(&empty, |_| None).unwrap();
    assert_eq!(defaults.option_value("bind").unwrap(), "127.0.0.1:9000");
}

#[test]
fn group_requirement_and_exclusion_include_environment_values() {
    let spec = CliSpec::new("node")
        .option(OptionSpec::flag("serve").requires("source"))
        .option(
            OptionSpec::value("descriptor")
                .exclusive_group("source")
                .env("DESCRIPTOR"),
        )
        .option(
            OptionSpec::value("catalog")
                .exclusive_group("source")
                .env("CATALOG"),
        );
    let parser = CliParser::new(&spec);
    let args = RawArgs::parse(["--serve"]).unwrap();
    assert_eq!(
        parser.parse_with_env(&args, |_| None).unwrap_err().kind(),
        CliErrorKind::MissingRequirement
    );
    assert!(parser
        .parse_with_env(&args, |name| (name == "CATALOG").then(|| "index".into()))
        .is_ok());
    assert_eq!(
        parser
            .parse_with_env(&args, |_| Some("index".into()))
            .unwrap_err()
            .kind(),
        CliErrorKind::OptionConflict
    );
}

#[test]
fn invalid_env_is_redacted_and_terminal_help_skips_sources() {
    let spec = CliSpec::new("node")
        .option(
            OptionSpec::value("count")
                .env("COUNT")
                .value_type(ValueType::U64),
        )
        .option(OptionSpec::flag("help").terminal(true));
    let parser = CliParser::new(&spec);
    let error = parser
        .parse_with_env(&RawArgs::parse([] as [&str; 0]).unwrap(), |_| {
            Some("private-input".into())
        })
        .unwrap_err();
    assert!(!error.to_string().contains("private-input"));
    assert!(!format!("{error:?}").contains("private-input"));
    assert!(parser
        .parse_with_env(&RawArgs::parse(["--help"]).unwrap(), |_| panic!(
            "must not read env"
        ))
        .is_ok());
}

#[cfg(unix)]
#[test]
fn native_paths_preserve_non_utf8_in_every_argument_form() {
    use std::os::unix::ffi::OsStringExt;
    let path = OsString::from_vec(vec![b'f', 0xff]);
    let spec = CliSpec::new("node")
        .option(
            OptionSpec::value("path")
                .short('p')
                .value_kind(ValueKind::Path),
        )
        .argument(ArgumentSpec::new("input").value_kind(ValueKind::Path));
    let parser = CliParser::new(&spec);
    for prefix in ["--path=", "-p", "-p="] {
        let mut word = OsString::from(prefix);
        word.push(&path);
        let raw = RawOsArgs::parse([word, path.clone(), "--".into(), path.clone()]).unwrap();
        let parsed = parser.parse_os_with_env(&raw, |_| None).unwrap();
        assert_eq!(
            parsed.option_path("path").unwrap(),
            std::path::PathBuf::from(&path)
        );
        assert_eq!(parsed.positionals(), std::slice::from_ref(&path));
        assert_eq!(parsed.passthrough(), std::slice::from_ref(&path));
    }
    let raw = RawOsArgs::parse(["--path".into(), path.clone()]).unwrap();
    assert_eq!(
        parser
            .parse_os_with_env(&raw, |_| None)
            .unwrap()
            .option_value("path"),
        Some(path.as_os_str())
    );
    let text = CliSpec::new("node").option(OptionSpec::value("path"));
    assert!(CliParser::new(&text)
        .parse_os_with_env(&raw, |_| None)
        .is_err());
}

#[test]
fn env_resolution_obeys_native_limits_and_stops_on_first_failure() {
    let spec = CliSpec::new("node").option(
        OptionSpec::value("path")
            .env("PATH_VALUE")
            .value_kind(ValueKind::Path),
    );
    let limits = ArgLimits {
        max_words: 1,
        max_word_bytes: 4,
        max_total_bytes: 4,
    };
    let raw = RawOsArgs::parse_with_limits([], limits).unwrap();
    assert!(CliParser::new(&spec)
        .parse_os_with_env(&raw, |_| Some("oversized".into()))
        .is_err());
    let mut read = 0;
    let words = ["first", "second", "third"].into_iter().map(|v| {
        read += 1;
        OsString::from(v)
    });
    assert!(RawOsArgs::parse_with_limits(words, limits).is_err());
    assert_eq!(read, 1);
}

#[test]
fn invalid_native_specs_fail_before_env_lookup() {
    for option in [
        OptionSpec::value("path").env("BAD=NAME"),
        OptionSpec::flag("path").value_kind(ValueKind::Path),
        OptionSpec::value("bind")
            .value_kind(ValueKind::SocketAddr)
            .default_value("localhost:8"),
        OptionSpec::value("path").exclusive_group("path"),
    ] {
        assert!(CliSpec::new("node").option(option).validate().is_err());
    }
}
