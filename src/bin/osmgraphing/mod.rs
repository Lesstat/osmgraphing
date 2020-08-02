use log::{error, info};
#[cfg(feature = "gpl-3.0")]
mod balancing;
#[cfg(feature = "gpl-3.0")]
use osmgraphing::routing::exploration::ConvexHullExplorator;
use osmgraphing::{
    configs,
    helpers::{err, init_logging},
    io,
    network::RoutePair,
    routing::dijkstra::{self, Dijkstra},
};
use std::{convert::TryFrom, path::PathBuf, time::Instant};

//------------------------------------------------------------------------------------------------//
// points in Germany

// somewhere in Stuttgart (Schwabstrasse)
// id 20_443_604 osm-id 2_933_335_353 lat 48.77017570000000291 lon 9.15657690000000102

// "near" Esslingen
// id:4_647 osm-id:163_354 lat:48.66743380000000485 lon:9.24459110000000095

// somewhere in Ulm
// id 9_058_109 osm-id 580_012_224 lat 48.39352330000000535 lon 9.9816315000000006

// near Aalen
// id 54_288 osm-id 2_237_652 lat 48.88542720000000230 lon 10.13642900000000147

// somewhere in Berlin
// id 296_679 osm-id 26_765_334 lat 52.50536590000000103 lon 13.38662390000000002

//------------------------------------------------------------------------------------------------//

fn main() {
    let args = match parse_cmdline() {
        Ok(args) => args,
        Err(msg) => {
            println!("ERROR: {}", msg);
            println!();
            panic!()
        }
    };
    let result = init_logging(&args.max_log_level, &[]);
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
    info!("EXECUTE {}", env!("CARGO_PKG_NAME"));

    // parse graph

    let graph = {
        // get config by provided user-input

        let parsing_cfg = {
            let raw_parsing_cfg = PathBuf::from(args.cfg.clone());
            configs::parsing::Config::try_from_yaml(&raw_parsing_cfg)?
        };

        // parse and create graph

        // measure parsing-time
        let now = Instant::now();

        let graph = io::network::graph::Parser::parse_and_finalize(parsing_cfg)?;
        info!(
            "Finished parsing in {} seconds ({} µs).",
            now.elapsed().as_secs(),
            now.elapsed().as_micros(),
        );
        info!("");
        info!("{}", graph);
        info!("");

        graph
    };

    // writing built graph

    if args.is_writing_graph {
        // get config by provided user-input

        let writing_cfg = configs::writing::network::graph::Config::try_from_yaml(&args.cfg)?;

        // check if new file does already exist

        if writing_cfg.map_file.exists() {
            return Err(err::Msg::from(format!(
                "New map-file {} does already exist. Please remove it.",
                writing_cfg.map_file.display()
            )));
        }

        // writing to file

        // measure writing-time
        let now = Instant::now();

        io::network::graph::Writer::write(&graph, &writing_cfg)?;
        info!(
            "Finished writing in {} seconds ({} µs).",
            now.elapsed().as_secs(),
            now.elapsed().as_micros(),
        );
        info!("");
    }

    // writing edges to file

    if args.is_writing_edges {
        // get config by provided user-input

        let writing_cfg = configs::writing::network::edges::Config::try_from_yaml(&args.cfg)?;

        // check if new file does already exist

        if writing_cfg.file.exists() {
            return Err(err::Msg::from(format!(
                "New file {} does already exist. Please remove it.",
                writing_cfg.file.display()
            )));
        }

        // writing to file

        // measure writing-time
        let now = Instant::now();

        io::network::edges::Writer::write(&graph, &writing_cfg)?;
        info!(
            "Finished writing in {} seconds ({} µs).",
            now.elapsed().as_secs(),
            now.elapsed().as_micros(),
        );
        info!("");
    }

    // writing routes to file

    if args.is_writing_routes {
        // get config by provided user-input

        let routing_cfg = configs::routing::Config::try_from_yaml(&args.cfg, graph.cfg())?;
        let writing_cfg = configs::writing::routing::Config::try_from_yaml(&args.cfg)?;

        // check if new file does already exist

        if writing_cfg.file.exists() {
            return Err(err::Msg::from(format!(
                "New routes-file {} does already exist. Please remove it.",
                writing_cfg.file.display()
            )));
        }

        // writing to file

        // measure writing-time
        let now = Instant::now();

        io::routing::Writer::write(&graph, &routing_cfg, &writing_cfg)?;
        info!(
            "Finished writing in {} seconds ({} µs).",
            now.elapsed().as_secs(),
            now.elapsed().as_micros(),
        );
        info!("");
    }

    // routing-example

    if args.is_routing && !args.is_checking_balance {
        // get config by provided user-input

        let routing_cfg = configs::routing::Config::try_from_yaml(&args.cfg, graph.cfg())?;

        info!("EXECUTE Do routing with alphas: {:?}", routing_cfg.alphas);

        let mut dijkstra = Dijkstra::new();

        // calculate best paths

        for RoutePair { src, dst } in io::routing::Parser::parse(&routing_cfg)?
            .iter()
            .map(|(route_pair, _)| route_pair.into_node(&graph))
        {
            info!("");

            let now = Instant::now();
            let best_path = dijkstra.compute_best_path(dijkstra::Query {
                src_idx: src.idx(),
                dst_idx: dst.idx(),
                graph: &graph,
                routing_cfg: &routing_cfg,
            });
            info!(
                "Ran Dijkstra-query in {} ms",
                now.elapsed().as_micros() as f64 / 1_000.0,
            );
            if let Some(best_path) = best_path {
                let best_path = best_path.flatten(&graph);
                info!("Found path {}.", best_path);
            } else {
                info!("No path from ({}) to ({}).", src, dst);
            }
        }
    }

    // explorating-example

    #[cfg(feature = "gpl-3.0")]
    if args.is_explorating && !args.is_checking_balance {
        // get config by provided user-input

        let routing_cfg = configs::routing::Config::try_from_yaml(&args.cfg, graph.cfg())?;

        info!("EXECUTE Do routing with alphas: {:?}", routing_cfg.alphas);

        let mut dijkstra = Dijkstra::new();
        let mut explorator = ConvexHullExplorator::new();

        // calculate best paths

        for (RoutePair { src, dst }, route_count) in io::routing::Parser::parse(&routing_cfg)?
            .iter()
            .map(|(route_pair, route_count)| (route_pair.into_node(&graph), *route_count))
        {
            for _ in 0..route_count {
                info!("");

                let now = Instant::now();
                let found_paths = explorator.fully_explorate(
                    dijkstra::Query {
                        src_idx: src.idx(),
                        dst_idx: dst.idx(),
                        graph: &graph,
                        routing_cfg: &routing_cfg,
                    },
                    &mut dijkstra,
                );

                info!("");
                info!(
                    "Ran Exploration-query in {} ms",
                    now.elapsed().as_micros() as f64 / 1_000.0,
                );
                if found_paths.is_empty() {
                    info!("No path found from ({}) to ({}).", src, dst);
                } else {
                    info!("Found {} path(s):", found_paths.len());
                    found_paths.iter().for_each(|path| info!("  {}", path))
                }
            }
        }
    }

    #[cfg(feature = "gpl-3.0")]
    if args.is_checking_balance {
        unimplemented!("TODO");
    }

    #[cfg(feature = "gpl-3.0")]
    if args.is_balancing {
        balancing::run(balancing::CmdlineArgs {
            max_log_level: args.max_log_level.clone(),
            cfg: args.cfg.clone(),
        })?;
    }

    Ok(())
}

fn parse_cmdline<'a>() -> err::Result<CmdlineArgs> {
    let args = clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .long_about(
            "\n\
            This tool takes a config-file, parses the chosen graph with specified settings, and \
            can execute specified tasks. Such tasks may be exporting the graph as fmi-map-file or \
            doing some routing-queries (if provided in config-file).\n\
            \n\
            NOTE\n\
            Some cmdline-arguments can only be used with the cargo-feature 'gpl-3.0' and hence are \
            hidden without it.",
        );

    let args = {
        let arg_log_level = clap::Arg::with_name(constants::ids::MAX_LOG_LEVEL)
            .long("log")
            .short("l")
            .value_name("FILTER-LEVEL")
            .help(
                "Sets the logging-level according to the env-variable 'RUST_LOG'. The env-variable \
                'RUST_LOG' has precedence. It takes values of modules, e.g. export RUST_LOG='warn,\
                osmgraphing=info' for getting warn's by default, but 'info' about the others",
            )
            .takes_value(true)
            .required(false)
            .case_insensitive(true)
            .default_value("INFO")
            .possible_values(&vec!["TRACE", "DEBUG", "INFO", "WARN", "ERROR"]);
        args.arg(arg_log_level)
    };

    let args = {
        let arg_parser_cfg = clap::Arg::with_name(constants::ids::CFG)
            .long("config")
            .short("c")
            .alias("parsing")
            .value_name("PATH")
            .help("Sets the parser and other configurations according to this config.")
            .takes_value(true)
            .required(true);
        args.arg(arg_parser_cfg)
    };

    let args = {
        let arg_is_writing_graph = clap::Arg::with_name(constants::ids::IS_WRITING_GRAPH)
            .long("writing_graph")
            .help(
                "The generated graph will be exported \
               as described in the provided config.",
            )
            .takes_value(false)
            .requires(constants::ids::CFG);
        args.arg(arg_is_writing_graph)
    };

    let args = {
        let arg_is_writing_edges = clap::Arg::with_name(constants::ids::IS_WRITING_EDGES)
            .long("writing_edges")
            .help(
                "The generated graph's edges will be exported \
               as described in the provided config.",
            )
            .takes_value(false)
            .requires(constants::ids::CFG);
        args.arg(arg_is_writing_edges)
    };

    let args = {
        let arg_is_writing_routes = clap::Arg::with_name(constants::ids::IS_WRITING_ROUTES)
            .long("writing_routes")
            .help(
                "The generated graph will be used to \
               generate and export valid routes \
               as described in the provided config.",
            )
            .takes_value(false)
            .requires(constants::ids::CFG);
        args.arg(arg_is_writing_routes)
    };

    let args = {
        let mut arg_is_routing = clap::Arg::with_name(constants::ids::IS_ROUTING)
            .long("routing")
            .help("Does routing as specified in the provided config.")
            .takes_value(true)
            .case_insensitive(true)
            .requires(constants::ids::CFG);
        if cfg!(feature = "gpl-3.0") {
            arg_is_routing = arg_is_routing.possible_values(&vec!["dijkstra", "explorating"]);
        } else {
            arg_is_routing = arg_is_routing.possible_values(&vec!["dijkstra"]);
        }
        args.arg(arg_is_routing)
    };

    let args = {
        let arg_is_balancing = clap::Arg::with_name(constants::ids::IS_BALANCING)
            .long("balancing")
            .help(
                "This balancer takes a config-file, parses the chosen graph with specified \
                settings, and optimizes found routes with the provided balancing- and routing- \
                config before writing the balanced graph into a fmi-file. Optimizing means \
                generating a new metric.\n\
                \n\
                Hence a correct config-file contains following:\n\
                - A parsing-config reading graph being balanced.\n\
                - A balancing-config defining the settings for the balancer.\n\
                - A routing-config specifying the routing-settings, which are used for calculating \
                the new metric.\n\
                - A writing-config for exporting the balanced graph.\n\
                \n\
                You can visualize the results with the python-module\n\
                py ./scripts/balancing/visualizer --results-dir <RESULTS_DIR/DATE>\n",
            )
            .takes_value(false)
            .hidden(!cfg!(feature = "gpl-3.0"))
            .requires(constants::ids::CFG);
        args.arg(arg_is_balancing)
    };

    let args = {
        let arg_is_checking_balance = clap::Arg::with_name(constants::ids::IS_CHECKING_BALANCE)
            .long("checking_balance")
            .help(
                "With this flag, the provided graph is executed with the defined \
                routing-algorithm. In opposite to simply executing the routing-queries, the \
                workload is counted per edge and being written to a specified file.\n\
                \n\
                You can visualize the results with the python-module\n\
                py ./scripts/balancing/visualizer --results-dir <RESULTS_DIR/DATE>\n",
            )
            .takes_value(false)
            .hidden(!cfg!(feature = "gpl-3.0"))
            .requires_all(&[constants::ids::CFG, constants::ids::IS_ROUTING]);
        args.arg(arg_is_checking_balance)
    };

    CmdlineArgs::try_from(args.get_matches())
}

mod constants {
    pub mod ids {
        pub const MAX_LOG_LEVEL: &str = "max-log-level";
        pub const CFG: &str = "cfg";
        pub const IS_WRITING_GRAPH: &str = "is_writing_graph";
        pub const IS_WRITING_EDGES: &str = "is_writing_edges";
        pub const IS_WRITING_ROUTES: &str = "is_writing_routes";
        pub const IS_ROUTING: &str = "is_routing";
        pub const IS_EXPLORATING: &str = "is_explorating";
        pub const IS_BALANCING: &str = "is_balancing";
        pub const IS_CHECKING_BALANCE: &str = "is_checking_balance";
    }
}

struct CmdlineArgs {
    max_log_level: String,
    cfg: String,
    is_writing_graph: bool,
    is_writing_edges: bool,
    is_writing_routes: bool,
    is_routing: bool,
    #[cfg(feature = "gpl-3.0")]
    is_explorating: bool,
    #[cfg(feature = "gpl-3.0")]
    is_balancing: bool,
    is_checking_balance: bool,
}

impl<'a> TryFrom<clap::ArgMatches<'a>> for CmdlineArgs {
    type Error = err::Msg;

    fn try_from(matches: clap::ArgMatches<'a>) -> err::Result<CmdlineArgs> {
        let max_log_level = matches
            .value_of(constants::ids::MAX_LOG_LEVEL)
            .expect(&format!("cmdline-arg: {}", constants::ids::MAX_LOG_LEVEL));
        let cfg = matches
            .value_of(constants::ids::CFG)
            .expect(&format!("cmdline-arg: {}", constants::ids::CFG));
        let is_writing_graph = matches.is_present(constants::ids::IS_WRITING_GRAPH);
        let is_writing_edges = matches.is_present(constants::ids::IS_WRITING_EDGES);
        let is_writing_routes = matches.is_present(constants::ids::IS_WRITING_ROUTES);
        let is_routing = matches.is_present(constants::ids::IS_ROUTING);
        let is_explorating = matches.is_present(constants::ids::IS_EXPLORATING);
        let is_balancing = matches.is_present(constants::ids::IS_BALANCING);
        let is_checking_balance = matches.is_present(constants::ids::IS_CHECKING_BALANCE);

        if is_explorating || is_balancing || is_checking_balance {
            check_for_activated_feature()?;
        }

        Ok(CmdlineArgs {
            max_log_level: String::from(max_log_level),
            cfg: String::from(cfg),
            is_writing_graph,
            is_writing_edges,
            is_writing_routes,
            is_routing,
            #[cfg(feature = "gpl-3.0")]
            is_explorating,
            #[cfg(feature = "gpl-3.0")]
            is_balancing,
            is_checking_balance,
        })
    }
}

fn check_for_activated_feature() -> err::Feedback {
    if !cfg!(feature = "gpl-3.0") {
        return Err(err::Msg::from("Please activate cargo-feature gpl-3.0."));
    }

    Ok(())
}