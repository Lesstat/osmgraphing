use std::path::PathBuf;

use log::{error, info};
use osmgraphing::{
    helpers::{err, init_logging},
    multi_ch_constructor::{self, Config},
};

fn main() {
    let args = parse_cmdline();
    let result = init_logging(&args.max_log_level(), &[]);
    if let Err(msg) = result {
        error!("{}{}", msg, "\n");
        panic!("{}", msg);
    }
    let result = run(args);
    if let Err(msg) = result {
        error!("{}{}", msg, "\n");
        panic!("{}", msg);
    }
}

fn run(args: CmdlineArgs) -> err::Feedback {
    info!("EXECUTE multi-ch-constructor");
    match args {
        CmdlineArgs::Run { cfg, .. } => {
            let mchc_cfg = Config::try_from_yaml(&cfg)?;
            multi_ch_constructor::construct_ch_graph(&mchc_cfg)?;
        }
        CmdlineArgs::Build { dim, acc, .. } => {
            let mchc_cfg = Config {
                fmi_graph: PathBuf::new(),
                ch_fmi_graph: PathBuf::new(),
                contraction_ratio: "0.0".to_string(),
                dim,
                cost_accuracy: acc,
                num_threads: 1,
                is_printing_osm_ids: false,
                is_using_external_edge_ids: false,
            };
            multi_ch_constructor::build(&mchc_cfg)?;
        }
    }

    Ok(())
}

fn parse_cmdline<'a>() -> CmdlineArgs {
    let tmp = &[
        "Sets the logging-level according to the env-variable 'RUST_LOG'.",
        "The env-variable 'RUST_LOG' has precedence.",
        "It takes values of modules, e.g.",
        "export RUST_LOG='warn,osmgraphing=info'",
        "for getting warn's by default, but 'info' about the others",
    ]
    .join("\n");
    let arg_log_level = clap::Arg::with_name(constants::ids::MAX_LOG_LEVEL)
        .long("log")
        .short("l")
        .value_name("FILTER-LEVEL")
        .help(tmp)
        .takes_value(true)
        .required(false)
        .case_insensitive(true)
        .default_value("INFO")
        .possible_values(&vec!["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]);

    let arg_cfg = clap::Arg::with_name(constants::ids::CFG)
        .long("config")
        .short("c")
        .value_name("PATH")
        .help("Sets the constructor's configuration according to this config.")
        .takes_value(true)
        .required(true);

    let run_command = clap::SubCommand::with_name(constants::ids::RUN)
        .about("Run multi-ch-constructor")
        .arg(arg_log_level.clone())
        .arg(arg_cfg);

    let dim_arg = clap::Arg::with_name(constants::ids::DIM)
        .short("d")
        .long("dim")
        .help("Dimension of graphs")
        .default_value("4");

    let acc_arg = clap::Arg::with_name(constants::ids::ACC)
        .short("a")
        .long("acc")
        .help("Accuracy used in comparisons")
        .default_value("0.000001");

    let build_command = clap::SubCommand::with_name(constants::ids::BUILD)
        .about("compile multi-ch-constructor with specific dimensnion and accuracy")
        .arg(dim_arg)
        .arg(acc_arg)
        .arg(arg_log_level);

    clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .long_about(
            (&[
                "",
                &format!(
                    "{}{}",
                    "This tool takes a config-file and constructs a contracted graph",
                    " from the provided graph-file."
                ),
                "settings, and can execute specified tasks.",
                "Such tasks may be exporting the graph as fmi-map-file or doing some ",
                "routing-queries (if provided in config-file).",
            ]
            .join("\n"))
                .as_ref(),
        )
        .subcommand(run_command)
        .subcommand(build_command)
        .get_matches()
        .into()
}

mod constants {
    pub mod ids {
        pub const MAX_LOG_LEVEL: &str = "max-log-level";
        pub const CFG: &str = "cfg";
        pub const DIM: &str = "dimension";
        pub const ACC: &str = "accuracy";
        pub const RUN: &str = "run";
        pub const BUILD: &str = "build";
    }
}

enum CmdlineArgs {
    Run {
        max_log_level: String,
        cfg: String,
    },
    Build {
        dim: usize,
        acc: f64,
        max_log_level: String,
    },
}

impl CmdlineArgs {
    fn max_log_level(&self) -> &str {
        match self {
            CmdlineArgs::Run { max_log_level, .. } => max_log_level,
            CmdlineArgs::Build { max_log_level, .. } => max_log_level,
        }
    }
}

impl<'a> From<clap::ArgMatches<'a>> for CmdlineArgs {
    fn from(matches: clap::ArgMatches<'a>) -> CmdlineArgs {
        match matches.subcommand() {
            (constants::ids::RUN, Some(matches)) => {
                let max_log_level = matches
                    .value_of(constants::ids::MAX_LOG_LEVEL)
                    .expect(&format!("cmdline-arg: {}", constants::ids::MAX_LOG_LEVEL));
                let cfg = matches
                    .value_of(constants::ids::CFG)
                    .expect(&format!("cmdline-arg: {}", constants::ids::CFG));

                CmdlineArgs::Run {
                    max_log_level: String::from(max_log_level),
                    cfg: String::from(cfg),
                }
            }
            (constants::ids::BUILD, Some(matches)) => {
                let dim = matches
                    .value_of(constants::ids::DIM)
                    .unwrap()
                    .parse()
                    .expect(&format!("cmdline-arg: {}", constants::ids::DIM));
                let acc = matches
                    .value_of(constants::ids::ACC)
                    .unwrap()
                    .parse()
                    .expect(&format!("cmdline-arg: {}", constants::ids::ACC));

                let max_log_level = matches
                    .value_of(constants::ids::MAX_LOG_LEVEL)
                    .expect(&format!("cmdline-arg: {}", constants::ids::MAX_LOG_LEVEL));
                CmdlineArgs::Build {
                    dim,
                    acc,
                    max_log_level: String::from(max_log_level),
                }
            }
            _ => {
                panic!("unknown usage pattern. Try multi-ch-constructor help")
            }
        }
    }
}
