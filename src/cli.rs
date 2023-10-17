// RCL -- A sane configuration language.
// Copyright 2023 Ruud van Asseldonk

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// A copy of the License has been included in the root of the repository.

//! Types that represent a parsed command line, and functions to parse it.

use std::collections::HashMap;
use std::str::FromStr;

use crate::cli_utils::{match_option, parse_option, Arg, ArgIter};
use crate::error::{Error, Result};
use crate::markup::{Markup, MarkupMode};
use crate::pprint::{concat, Doc};

const USAGE_MAIN: &str = r#"
RCL -- A sane configuration language.

Usage:
  rcl [<options>] <command> <arguments>

Commands:
  evaluate     Evaluate a document to an output format.
  format       Auto-format an RCL document.
  highlight    Print a document with syntax highlighting.
  query        Evaluate an expression against an input document.

Command shorthands:
  e, eval      Alias for 'evaluate'.
  f, fmt       Alias for 'format'.
  h            Alias for 'highlight'.
  jq           Alias for 'query --output=json'.
  q            Alias for 'query'.

Global options:
  -h --help       Show this screen, or command-specific help.
  --version       Show version.
  --color <mode>  Set how output is colored, see modes below.

Color modes:
  ansi    Always color output using ANSI escape codes.
  auto    Use ANSI if the output file is a TTY and the NO_COLOR environment
          variable is not set to a non-empty string. This is the default.
  none    Do not color output at all.
"#;

const USAGE_EVAL_QUERY: &str = r#"
RCL -- A sane configuration language.

Usage:
  rcl [<options>] evaluate [<options>] <file>
  rcl [<options>] query    [<options>] <file> <query>

The 'evaluate' command evaluates the expression in the input file and prints it
to stdout. The 'query' command additionally evaluates an expression against the
result.

Arguments:
  <file>     The input file to process, or '-' for stdin.
  <query>    An RCL expression to evaluate. The result of evaluating the input
             file is bound to the variable 'input'.

Options:
  -o --output <format>  Output format, can be one of 'json', 'rcl'.
                        Defaults to 'rcl'.
  -w --width <width>    Target width for pretty-printing, must be an integer.
                        Defaults to 80.
  --sandbox <mode>      Sandboxing mode, can be one of 'pure', 'workdir', and
                        'unrestricted'. Defaults to 'workdir'.
  -I --include <a>:<f>  Enable importing the file <f> when --sandbox=pure.
                        To import the file, the import expression must be of the
                        form 'import "<a>"'. In the argument, the alias <a> and
                        file <f> are separated by a colon.

Sandboxing modes:
  pure          Only allow importing files specified with --include. Do not
                allow filesystem access aside from these includes.
  workdir       Only allow importing files inside the working directory and
                subdirectories.
  unrestricted  Grant unrestricted filesystem access, allow importing any file.

See also --help for global options.
"#;

const USAGE_FORMAT: &str = r#"
RCL -- A sane configuration language.

Usage:
  rcl [<options>] format [<options>] <file>...

The 'format' command formats one or more input documents in standard style.

Arguments:
  <file>...        The input files to process, or '-' for stdin. When --in-place
                   is used, there can be multiple input files.

Options:
  -i --in-place       Rewrite files in-place instead of writing to stdout.
                      By default the formatted result is written to stdout.
  -w --width <width>  Target width in number of columns, must be an integer.
                      Defaults to 80.

See also --help for global options.
"#;

/// Options that apply to all subcommands.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct GlobalOptions {
    /// Whether and how to output color and other markup.
    ///
    /// We call it “markup” internally because it's more than just color, but
    /// we call it `--color` on the command line because that is what most tools
    /// call it.
    pub markup: Option<MarkupMode>,
}

/// The available output formats (JSON, RCL).
#[derive(Debug, Default, Eq, PartialEq)]
pub enum OutputFormat {
    Json,
    #[default]
    Rcl,
}

/// The available sandboxing modes.
#[derive(Debug, Default, Eq, PartialEq)]
pub enum SandboxMode {
    Pure,
    #[default]
    Workdir,
    Unrestricted,
}

/// Options for commands that output values.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct OutputOptions {
    /// The format to output in.
    pub format: OutputFormat,

    /// Sandboxing mode for imports.
    pub sandbox: SandboxMode,

    /// Files to include, when the sandboxing mode is pure.
    pub includes: HashMap<String, String>,
}

/// Options for commands that pretty-print their output.
#[derive(Debug, Eq, PartialEq)]
pub struct FormatOptions {
    /// Target width (number of columns) to try to not exceed.
    pub width: u32,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { width: 80 }
    }
}

/// Input to act on.
#[derive(Debug, Eq, PartialEq)]
pub enum Target {
    File(String),
    Stdin,
}

/// For the `fmt` command, which documents to format, and in what mode.
#[derive(Debug, Eq, PartialEq)]
pub enum FormatTarget {
    Stdout { fname: Target },
    InPlace { fnames: Vec<Target> },
}

/// The different subcommands supported by the main program.
#[derive(Debug, Eq, PartialEq)]
pub enum Cmd {
    Evaluate {
        output_opts: OutputOptions,
        format_opts: FormatOptions,
        fname: Target,
    },
    Query {
        output_opts: OutputOptions,
        format_opts: FormatOptions,
        fname: Target,
        query: String,
    },
    Format {
        format_opts: FormatOptions,
        target: FormatTarget,
    },
    Highlight {
        fname: Target,
    },
    Help {
        usage: &'static str,
    },
    Version,
}

/// Parse the command line.
pub fn parse(args: Vec<String>) -> Result<(GlobalOptions, Cmd)> {
    let mut args = ArgIter::new(args);

    // Skip over the program name.
    args.next();

    let mut cmd: Option<&'static str> = None;
    let mut cmd_help: Option<&'static str> = None;
    let mut format_opts = FormatOptions::default();
    let mut global_opts = GlobalOptions::default();
    let mut output_opts = OutputOptions::default();
    let mut in_place = false;
    let mut is_version = false;
    let mut targets: Vec<Target> = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_ref() {
            Arg::Long("color") => {
                global_opts.markup = match_option! {
                    args: arg,
                    "auto" => None,
                    "ansi" => Some(MarkupMode::Ansi),
                    "none" => Some(MarkupMode::None),
                }
            }
            Arg::Long("output") | Arg::Short("o") => {
                output_opts.format = match_option! {
                    args: arg,
                    "json" => OutputFormat::Json,
                    "rcl" => OutputFormat::Rcl,
                }
            }
            Arg::Long("sandbox") => {
                output_opts.sandbox = match_option! {
                    args: arg,
                    "pure" => SandboxMode::Pure,
                    "workdir" => SandboxMode::Workdir,
                    "unrestricted" => SandboxMode::Unrestricted,
                }
            }
            Arg::Long("include") | Arg::Short("I") => {
                let (alias, fname) = parse_option! {
                    args: arg,
                    |v: &str| -> std::result::Result<(String, String), &'static str> {
                        let mut parts = v.splitn(2, ":");
                        let alias = parts.next().ok_or("unused")?;
                        let fname = parts.next().ok_or("unused")?;
                        Ok((alias.to_owned(), fname.to_owned()))
                    }
                };
                output_opts.includes.insert(alias, fname);
            }
            Arg::Long("width") | Arg::Short("w") => {
                format_opts.width = parse_option! { args: arg, u32::from_str };
            }
            Arg::Long("in-place") | Arg::Short("i") => {
                in_place = true;
            }
            Arg::Long("help") | Arg::Short("h") => {
                is_version = false;
                cmd_help = match cmd {
                    Some(c) => Some(c),
                    None => Some("main"),
                };
            }
            Arg::Long("version") => {
                is_version = true;
                cmd_help = None;
            }
            Arg::Plain("evaluate") | Arg::Plain("eval") | Arg::Plain("e") if cmd.is_none() => {
                cmd = Some("evaluate");
            }
            Arg::Plain("query") | Arg::Plain("q") if cmd.is_none() => {
                cmd = Some("query");
            }
            Arg::Plain("jq") if cmd.is_none() => {
                cmd = Some("query");
                output_opts.format = OutputFormat::Json;
            }
            Arg::Plain("format") | Arg::Plain("fmt") | Arg::Plain("f") if cmd.is_none() => {
                cmd = Some("format");
            }
            Arg::Plain("highlight") | Arg::Plain("h") if cmd.is_none() => {
                cmd = Some("highlight");
            }
            Arg::Plain(fname) if cmd.is_some() => {
                targets.push(Target::File(fname.to_string()));
            }
            Arg::StdInOut if cmd.is_some() => {
                targets.push(Target::Stdin);
            }
            Arg::Plain(..) => {
                let err = concat! {
                    "Unknown command '"
                    Doc::lines(&arg.to_string()).into_owned().with_markup(Markup::Highlight)
                    "'. See --help for usage."
                };
                return Error::new(err).err();
            }
            _ => {
                let err = concat! {
                    "Unknown option '"
                    Doc::lines(&arg.to_string()).into_owned().with_markup(Markup::Highlight)
                    "'. See --help for usage."
                };
                return Error::new(err).err();
            }
        }
    }

    if is_version {
        return Ok((global_opts, Cmd::Version));
    }

    let help_opt = match cmd_help {
        Some("evaluate") => Some(Cmd::Help {
            usage: USAGE_EVAL_QUERY,
        }),
        Some("format") => Some(Cmd::Help {
            usage: USAGE_FORMAT,
        }),
        // TODO: Add usage for highlight.
        Some("highlight") => Some(Cmd::Help { usage: USAGE_MAIN }),
        Some("main") => Some(Cmd::Help { usage: USAGE_MAIN }),
        Some("query") => Some(Cmd::Help {
            usage: USAGE_EVAL_QUERY,
        }),
        _ => None,
    };
    if let Some(help) = help_opt {
        return Ok((global_opts, help));
    }

    let result = match cmd {
        Some("evaluate") => Cmd::Evaluate {
            output_opts,
            format_opts,
            fname: get_unique_target(targets)?,
        },
        Some("query") => {
            let (fname, query) = match targets.len() {
                2 => (
                    targets.remove(0),
                    // Not a file, but a consequence of the CLI parsing that
                    // handles - being stdin/stdout.
                    match targets.remove(0) {
                        Target::File(query) => query,
                        Target::Stdin => "-".to_string(),
                    },
                ),
                _ => {
                    return Error::new(
                        "Expected an input file and a query. \
                        See --help for usage.",
                    )
                    .err()
                }
            };
            Cmd::Query {
                output_opts,
                format_opts,
                query,
                fname,
            }
        }
        Some("format") => Cmd::Format {
            format_opts,
            target: if in_place {
                FormatTarget::InPlace { fnames: targets }
            } else {
                FormatTarget::Stdout {
                    fname: get_unique_target(targets)?,
                }
            },
        },
        Some("highlight") => Cmd::Highlight {
            fname: get_unique_target(targets)?,
        },
        None => Cmd::Help { usage: USAGE_MAIN },
        _ => panic!("Should have returned an error before getting here."),
    };
    Ok((global_opts, result))
}

fn get_unique_target(mut targets: Vec<Target>) -> Result<Target> {
    match targets.pop() {
        None => Error::new("Expected an input file. See --help for usage.").err(),
        Some(_) if !targets.is_empty() => {
            Error::new("Too many input files. See --help for usage.").err()
        }
        Some(t) => Ok(t),
    }
}

#[cfg(test)]
mod test {
    use crate::cli::{
        Cmd, FormatOptions, FormatTarget, GlobalOptions, OutputFormat, OutputOptions, Target,
    };
    use crate::markup::MarkupMode;
    use crate::pprint::Config;

    fn fail_parse(args: &[&'static str]) -> String {
        let args_vec: Vec<_> = args.iter().map(|a| a.to_string()).collect();
        let err = super::parse(args_vec).err().unwrap();
        let cfg = Config {
            width: 80,
            markup: MarkupMode::None,
        };
        err.report(&[]).println(&cfg)
    }

    fn parse(args: &[&'static str]) -> (GlobalOptions, Cmd) {
        let args_vec: Vec<_> = args.iter().map(|a| a.to_string()).collect();
        super::parse(args_vec).unwrap()
    }

    #[test]
    fn parse_cmd_eval() {
        let expected_opt = GlobalOptions { markup: None };
        let expected_cmd = Cmd::Evaluate {
            output_opts: OutputOptions::default(),
            format_opts: FormatOptions::default(),
            fname: Target::File("infile".into()),
        };
        let mut expected = (expected_opt, expected_cmd);

        // All of the aliases should behave the same.
        assert_eq!(parse(&["rcl", "evaluate", "infile"]), expected);
        assert_eq!(parse(&["rcl", "eval", "infile"]), expected);
        assert_eq!(parse(&["rcl", "e", "infile"]), expected);

        // Test that --color works.
        assert_eq!(parse(&["rcl", "--color=auto", "e", "infile"]), expected);
        expected.0.markup = Some(MarkupMode::None);
        assert_eq!(parse(&["rcl", "--color=none", "e", "infile"]), expected);
        expected.0.markup = Some(MarkupMode::Ansi);
        assert_eq!(parse(&["rcl", "--color=ansi", "e", "infile"]), expected);

        // We should be able to pass --color in any place.
        assert_eq!(parse(&["rcl", "--color=ansi", "e", "infile"]), expected);
        assert_eq!(parse(&["rcl", "--color", "ansi", "e", "infile"]), expected);
        assert_eq!(
            parse(&["rcl", "eval", "--color", "ansi", "infile"]),
            expected
        );
        assert_eq!(
            parse(&["rcl", "eval", "infile", "--color", "ansi"]),
            expected
        );

        // If we specify an option twice, the last one takes precedence.
        assert_eq!(
            parse(&["rcl", "e", "infile", "--color=none", "--color=ansi"]),
            expected
        );

        // Test that --width works, in any location, last option wins.
        expected.0.markup = None;
        if let Cmd::Evaluate { format_opts, .. } = &mut expected.1 {
            format_opts.width = 42;
        }
        assert_eq!(parse(&["rcl", "e", "--width=42", "infile"]), expected);
        assert_eq!(parse(&["rcl", "e", "--width", "42", "infile"]), expected);
        assert_eq!(parse(&["rcl", "e", "-w42", "infile"]), expected);
        assert_eq!(parse(&["rcl", "e", "-w", "42", "infile"]), expected);
        assert_eq!(parse(&["rcl", "e", "infile", "-w42"]), expected);
        assert_eq!(parse(&["rcl", "-w42", "e", "infile"]), expected);
        assert_eq!(
            parse(&["rcl", "-w100", "e", "--width=42", "infile"]),
            expected
        );

        // Test that --output works. We don't have to be as thorough, it's using
        // the same parser, if it works for the other options it should work here.
        if let Cmd::Evaluate {
            format_opts,
            output_opts,
            ..
        } = &mut expected.1
        {
            format_opts.width = 80;
            output_opts.format = OutputFormat::Json;
        }
        assert_eq!(parse(&["rcl", "e", "infile", "-ojson"]), expected);
        assert_eq!(parse(&["rcl", "e", "infile", "--output", "json"]), expected);
        assert_eq!(parse(&["rcl", "e", "infile", "--output=json"]), expected);
        assert_eq!(parse(&["rcl", "-ojson", "e", "infile"]), expected);
        assert_eq!(parse(&["rcl", "-orcl", "-ojson", "e", "infile"]), expected);
    }

    #[test]
    fn parse_cmd_eval_fails_on_invalid_usage() {
        assert_eq!(
            fail_parse(&["rcl", "eval"]),
            "Error: Expected an input file. See --help for usage.\n"
        );
        assert_eq!(
            fail_parse(&["rcl", "eval", "infile", "--width=bobcat"]),
            "Error: 'bobcat' is not valid for --width. See --help for usage.\n"
        );
        assert_eq!(
            fail_parse(&["rcl", "eval", "infile", "-wbobcat"]),
            "Error: 'bobcat' is not valid for -w. See --help for usage.\n"
        );
        assert_eq!(
            fail_parse(&["rcl", "eval", "infile", "--output=yamr"]),
            "Error: Expected --output to be followed by one of json, rcl. See --help for usage.\n"
        );
        assert_eq!(
            fail_parse(&["rcl", "frobnicate", "infile"]),
            "Error: Unknown command 'frobnicate'. See --help for usage.\n"
        );
        assert_eq!(
            fail_parse(&["rcl", "eval", "--frobnicate", "infile"]),
            "Error: Unknown option '--frobnicate'. See --help for usage.\n"
        );
    }

    #[test]
    fn parse_cmd_fmt() {
        let expected_opt = GlobalOptions { markup: None };
        let expected_cmd = Cmd::Format {
            format_opts: FormatOptions::default(),
            target: FormatTarget::Stdout {
                fname: Target::File("infile".into()),
            },
        };
        let mut expected = (expected_opt, expected_cmd);

        // All of the aliases should behave the same.
        assert_eq!(parse(&["rcl", "format", "infile"]), expected);
        assert_eq!(parse(&["rcl", "fmt", "infile"]), expected);
        assert_eq!(parse(&["rcl", "f", "infile"]), expected);

        // Without --in-place, we can do only one arg.
        assert_eq!(
            fail_parse(&["rcl", "f", "f1", "f2"]),
            "Error: Too many input files. See --help for usage.\n",
        );

        if let Cmd::Format { ref mut target, .. } = &mut expected.1 {
            *target = FormatTarget::InPlace {
                fnames: vec![Target::File("f1".into()), Target::File("f2".into())],
            };
        }
        assert_eq!(parse(&["rcl", "f", "--in-place", "f1", "f2"]), expected);
        assert_eq!(parse(&["rcl", "-i", "f", "f1", "f2"]), expected);
    }

    #[test]
    fn parse_cmd_help_version() {
        assert!(matches!(parse(&["rcl", "--help"]).1, Cmd::Help { .. }));
        assert!(matches!(parse(&["rcl", "--version"]).1, Cmd::Version));
        assert!(matches!(parse(&["rcl", "eval", "-h"]).1, Cmd::Help { .. }));
        assert!(matches!(
            parse(&["rcl", "format", "-h"]).1,
            Cmd::Help { .. }
        ));
        assert!(matches!(
            parse(&["rcl", "highlight", "-h"]).1,
            Cmd::Help { .. }
        ));
        assert!(matches!(parse(&["rcl", "query", "-h"]).1, Cmd::Help { .. }));
        // Missing subcommand also triggers help.
        assert!(matches!(parse(&["rcl"]).1, Cmd::Help { .. }));
    }

    #[test]
    fn parse_cmd_highlight() {
        let expected_opt = GlobalOptions { markup: None };
        let expected_cmd = Cmd::Highlight {
            fname: Target::File("infile".into()),
        };
        let expected = (expected_opt, expected_cmd);
        assert_eq!(parse(&["rcl", "highlight", "infile"]), expected);
    }

    #[test]
    fn parse_cmd_query() {
        let expected_opt = GlobalOptions { markup: None };
        let expected_cmd = Cmd::Query {
            output_opts: OutputOptions::default(),
            format_opts: FormatOptions::default(),
            fname: Target::File("infile".into()),
            query: "input.name".to_string(),
        };
        let mut expected = (expected_opt, expected_cmd);
        assert_eq!(parse(&["rcl", "query", "infile", "input.name"]), expected);
        assert_eq!(parse(&["rcl", "q", "infile", "input.name"]), expected);

        if let Cmd::Query { output_opts, .. } = &mut expected.1 {
            output_opts.format = OutputFormat::Json
        };
        assert_eq!(parse(&["rcl", "jq", "infile", "input.name"]), expected);

        assert_eq!(
            fail_parse(&["rcl", "q", "infile"]),
            "Error: Expected an input file and a query. See --help for usage.\n",
        );
    }

    #[test]
    fn parse_cmd_handles_stdin_and_double_dash() {
        assert_eq!(
            parse(&["rcl", "highlight", "infile"]).1,
            Cmd::Highlight {
                fname: Target::File("infile".into()),
            }
        );
        assert_eq!(
            parse(&["rcl", "highlight", "--", "infile"]).1,
            Cmd::Highlight {
                fname: Target::File("infile".into()),
            }
        );
        assert_eq!(
            parse(&["rcl", "highlight", "-"]).1,
            Cmd::Highlight {
                fname: Target::Stdin,
            }
        );
        assert_eq!(
            parse(&["rcl", "highlight", "--", "-"]).1,
            Cmd::Highlight {
                fname: Target::File("-".into()),
            }
        );
    }
}
