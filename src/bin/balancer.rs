use log::{error, info};
use osmgraphing::{
    configs, defaults,
    helpers::{err, init_logging},
    io,
    network::Graph,
    routing,
};
use rand::SeedableRng;
use std::{path::Path, time::Instant};

fn main() {
    let result = run();
    if let Err(msg) = result {
        error!("{}\n", msg);
        std::process::exit(1);
    }
}

fn run() -> err::Feedback {
    // process user-input

    let args = parse_cmdline();
    match init_logging(&args.max_log_level, &["balancer"]) {
        Ok(_) => (),
        Err(msg) => return Err(format!("{}", msg).into()),
    };

    // check writing-cfg
    let _ = configs::writing::network::Config::try_from_yaml(&args.cfg)?;
    let mut balancing_cfg = configs::balancing::Config::try_from_yaml(&args.cfg)?;

    info!("EXECUTE balancer");

    let mut dijkstra = routing::Dijkstra::new();
    let mut explorator = routing::ConvexHullExplorator::new();
    let mut rng = rand_pcg::Pcg32::seed_from_u64(defaults::SEED);

    // prepare simulation
    // e.g. creating the results-folder and converting the graph into the right format

    let custom_graph = simulation_pipeline::read_in_custom_graph(&args.cfg)?;
    // check routing-cfg
    let _ = configs::routing::Config::try_from_yaml(&args.cfg, custom_graph.cfg())?;

    // start balancing

    simulation_pipeline::prepare_results(&args.cfg, &mut balancing_cfg)?;

    let mut graph = custom_graph;
    for iter in 0..balancing_cfg.num_iter {
        simulation_pipeline::prepare_iteration(iter, &balancing_cfg)?;
        simulation_pipeline::write_multi_ch_graph(&balancing_cfg, graph, iter)?;
        simulation_pipeline::construct_ch_graph(&balancing_cfg, iter)?;
        let mut ch_graph = simulation_pipeline::read_in_ch_graph(&balancing_cfg, iter)?;
        let routing_cfg =
            simulation_pipeline::read_in_routing_cfg(&balancing_cfg, iter, &args.cfg, &ch_graph)?;
        simulation_pipeline::balance(
            iter,
            &balancing_cfg,
            &mut ch_graph,
            &routing_cfg,
            &mut dijkstra,
            &mut explorator,
            &mut rng,
        )?;
        graph = ch_graph;
    }

    // store balanced graph

    let mut writing_cfg = configs::writing::network::Config::try_from_yaml(&args.cfg)?;
    writing_cfg.map_file = balancing_cfg
        .results_dir
        .join(writing_cfg.map_file.file_name().ok_or(err::Msg::from(
            "The provided route-pairs-file in the (routing-)config is not a file.",
        ))?);
    write_graph(&graph, &writing_cfg)?;

    info!(
        "Execute py ./scripts/balancing/visualizer --results-dir {} to visualize.",
        balancing_cfg.results_dir.display()
    );

    Ok(())
}

mod simulation_pipeline {
    use chrono;
    use log::{debug, info};
    use osmgraphing::{
        configs, defaults,
        helpers::err,
        io,
        network::{Graph, RoutePair},
        routing,
    };
    use progressing::{Bar, MappingBar};
    use rand::distributions::{Distribution, Uniform};
    use std::{fs, path::Path, time::Instant};

    pub fn read_in_custom_graph(raw_parsing_cfg: &str) -> err::Result<Graph> {
        let parsing_cfg = configs::parsing::Config::try_from_yaml(&raw_parsing_cfg)?;
        super::parse_graph(parsing_cfg)
    }

    pub fn prepare_results<P: AsRef<Path>>(
        raw_cfg: P,
        balancing_cfg: &mut configs::balancing::Config,
    ) -> err::Feedback {
        let raw_cfg = raw_cfg.as_ref();

        // set results-directory dependent of the current date in utc
        balancing_cfg.results_dir = balancing_cfg.results_dir.join(format!(
            "utc_{}",
            chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S")
        ));
        fs::create_dir_all(&balancing_cfg.results_dir)?;
        info!("Storing results in {}", balancing_cfg.results_dir.display());

        fs::copy(
            raw_cfg,
            balancing_cfg.results_dir.join(
                raw_cfg
                    .file_name()
                    .ok_or(err::Msg::from("The provided cfg is not a file."))?,
            ),
        )?;

        Ok(())
    }

    pub fn prepare_iteration(
        iter: usize,
        balancing_cfg: &configs::balancing::Config,
    ) -> err::Feedback {
        // create directory for results

        let iter_dir = balancing_cfg.results_dir.join(format!("{}", iter));
        fs::create_dir_all(&iter_dir.join(defaults::balancing::stats::DIR))?;

        // copy all necessary configs in there
        fs::copy(
            if iter == 0 {
                &balancing_cfg.iter_0_cfg
            } else {
                &balancing_cfg.iter_i_cfg
            },
            iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;

        Ok(())
    }

    pub fn write_multi_ch_graph(
        balancing_cfg: &configs::balancing::Config,
        graph: Graph,
        iter: usize,
    ) -> err::Feedback {
        let iter_dir = balancing_cfg.results_dir.join(format!("{}", iter));
        let mut writing_cfg = configs::writing::network::Config::try_from_yaml(
            &iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;
        // path is relative to results-dir
        writing_cfg.map_file = iter_dir.join(writing_cfg.map_file);

        super::write_graph(&graph, &writing_cfg)
    }

    pub fn construct_ch_graph(
        balancing_cfg: &configs::balancing::Config,
        iter: usize,
    ) -> err::Feedback {
        let iter_dir = balancing_cfg.results_dir.join(format!("{}", iter));

        let graph_dim = {
            let is_using_new_metric = iter > 0;
            if is_using_new_metric {
                balancing_cfg.new_graph_dim
            } else {
                balancing_cfg.new_graph_dim - 1 // without new metric
            }
        };

        let cmd_args = &["-Bbuild", "-D", &format!("GRAPH_DIM={}", graph_dim)];
        let is_successful = std::process::Command::new("cmake")
            .current_dir(fs::canonicalize(&balancing_cfg.multi_ch_constructor.dir)?)
            .args(cmd_args)
            .status()?
            .success();
        if !is_successful {
            return Err(format!("Failed: cmake {}", cmd_args.join(" ")).into());
        }

        let cmd_args = &["--build", "build"];
        let is_successful = std::process::Command::new("cmake")
            .current_dir(fs::canonicalize(&balancing_cfg.multi_ch_constructor.dir)?)
            .args(cmd_args)
            .status()?
            .success();
        if !is_successful {
            return Err(format!("Failed: cmake {}", cmd_args.join(" ")).into());
        }

        let cmd_args = &[
            "--using-osm-ids",
            "--text",
            &format!("{}", iter_dir.join("graph.fmi").to_string_lossy()),
            "--percent",
            &format!("{}", &balancing_cfg.multi_ch_constructor.contraction_ratio),
            "--write",
            &format!("{}", iter_dir.join("graph.ch.fmi").to_string_lossy()),
        ];
        let is_successful = std::process::Command::new(
            Path::new(&balancing_cfg.multi_ch_constructor.dir)
                .join("build")
                .join("multi-ch"),
        )
        .args(cmd_args)
        .status()?
        .success();
        if !is_successful {
            return Err(format!(
                "Failed: ./externals/multi-ch-constructor/build/multi-ch {}",
                cmd_args.join(" ")
            )
            .into());
        }

        Ok(())
    }

    pub fn read_in_ch_graph(
        balancing_cfg: &configs::balancing::Config,
        iter: usize,
    ) -> err::Result<Graph> {
        let iter_dir = balancing_cfg.results_dir.join(format!("{}", iter));
        let mut parsing_cfg = configs::parsing::Config::try_from_yaml(
            &iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;
        parsing_cfg.map_file = iter_dir.join(parsing_cfg.map_file);
        super::parse_graph(parsing_cfg)
    }

    pub fn read_in_routing_cfg(
        balancing_cfg: &configs::balancing::Config,
        iter: usize,
        raw_routing_cfg: &str,
        ch_graph: &Graph,
    ) -> err::Result<configs::routing::Config> {
        // read in routing-cfg and

        let mut routing_cfg =
            configs::routing::Config::try_from_yaml(&raw_routing_cfg, ch_graph.cfg())?;
        let old_route_pairs_file = routing_cfg.route_pairs_file.ok_or(err::Msg::from(
            "Please provide a route-pairs-file in your (routing-)config.",
        ))?;
        let new_route_pairs_file =
            balancing_cfg
                .results_dir
                .join(old_route_pairs_file.file_name().ok_or(err::Msg::from(
                    "The provided route-pairs-file in the (routing-)config is not a file.",
                ))?);

        // if first iteration
        if iter == 0 {
            // -> deactivate workload-metric

            let workload_idx = ch_graph
                .cfg()
                .edges
                .metrics
                .try_idx_of(&balancing_cfg.workload_id)?;
            routing_cfg.alphas[*workload_idx] = 0.0;

            // -> and copy route-pairs-file into the results-directory
            fs::copy(old_route_pairs_file, &new_route_pairs_file)?;
        }

        routing_cfg.route_pairs_file = Some(new_route_pairs_file);
        Ok(routing_cfg)
    }

    pub fn balance(
        iter: usize,
        balancing_cfg: &configs::balancing::Config,
        ch_graph: &mut Graph,
        routing_cfg: &configs::routing::Config,
        dijkstra: &mut routing::Dijkstra,
        explorator: &mut routing::ConvexHullExplorator,
        rng: &mut rand_pcg::Lcg64Xsh32,
    ) -> err::Feedback {
        info!(
            "Explorate several routes for metrics {:?} of dimension {}",
            ch_graph.cfg().edges.metrics.units,
            ch_graph.metrics().dim()
        );

        let route_pairs = io::routing::Parser::parse(&routing_cfg)?;

        // simple init-logging

        info!("START Executing routes and analyzing workload",);
        let mut progress_bar = MappingBar::new(0..=route_pairs.len());
        info!("{}", progress_bar);

        // find all routes and count density on graph

        let mut workloads: Vec<usize> = vec![0; ch_graph.fwd_edges().count()];

        for &(route_pair, num_routes) in &route_pairs {
            let RoutePair { src, dst } = route_pair.into_node(&ch_graph);

            // find explorated routes

            let now = Instant::now();
            let found_paths =
                explorator.fully_explorate(src.idx(), dst.idx(), dijkstra, &ch_graph, &routing_cfg);
            debug!(
                "Ran Explorator-query from src-id {} to dst-id {} in {} ms. Found {} path(s).",
                src.id(),
                dst.id(),
                now.elapsed().as_micros() as f64 / 1_000.0,
                found_paths.len()
            );

            // Update next workload by looping over all found routes
            // -> Routes have to be flattened,
            // -> or shortcuts will lead to wrong best-paths, because counts won't be cumulated.

            if found_paths.len() > 0 {
                let die = Uniform::from(0..found_paths.len());
                for _ in 0..num_routes {
                    let p = found_paths[die.sample(rng)].clone().flatten(&ch_graph);

                    debug!("    {}", p);

                    for edge_idx in p {
                        workloads[*edge_idx] += 1;
                    }
                }
            }

            progress_bar.add(true);
            if progress_bar.progress() % (1 + (progress_bar.end() / 10)) == 0 {
                info!("{}", progress_bar);
            }
        }

        // update graph with new values
        defaults::balancing::update_new_metric(&workloads, ch_graph, &balancing_cfg);

        // export density and iteration-results

        // measure writing-time
        let now = Instant::now();

        io::balancing::Writer::write(iter, &workloads, &ch_graph, &balancing_cfg)?;
        info!(
            "FINISHED Written in {} seconds ({} µs).",
            now.elapsed().as_secs(),
            now.elapsed().as_micros(),
        );
        info!("");

        Ok(())
    }
}

// utils

/// If the map-file starts with "graph", it is assumed to have a generic name and this method returns directory of the map-file.
/// Otherwise, it returns the filename of the map-file without all extension.
fn _extract_map_name<P: AsRef<Path>>(map_file: P) -> err::Result<String> {
    let map_file = map_file.as_ref();
    let map_name = {
        if let Some(map_name) = map_file.file_stem() {
            let map_name = map_name.to_string_lossy();
            // check if name is too generic
            if map_name.starts_with("graph") {
                // because of generic name -> take name of parent-directory
                map_file
                    // get path without filename
                    .parent()
                    .expect(&format!(
                        "The provided map-file {} isn't as expected.",
                        map_file.to_string_lossy()
                    ))
                    // and extract parent-directory from path
                    .file_name()
                    .expect(&format!(
                        "The provided map-file {} isn't as expected.",
                        map_file.to_string_lossy()
                    ))
                    .to_string_lossy()
                    .into_owned()
            } else {
                // take filename
                let i = map_name
                    .chars()
                    .position(|c| c == '.')
                    .expect("Expected some graph-extension");
                String::from(&map_name[..i])
            }
        } else {
            return Err(format!("No map-file specified.").into());
        }
    };

    return Ok(map_name);
}

fn parse_graph(parsing_cfg: configs::parsing::Config) -> err::Result<Graph> {
    let now = Instant::now();

    let graph = io::network::Parser::parse_and_finalize(parsing_cfg)?;

    info!(
        "FINISHED Parsed graph in {} seconds ({} µs).",
        now.elapsed().as_secs(),
        now.elapsed().as_micros(),
    );
    info!("");
    info!("{}", graph);
    info!("");

    Ok(graph)
}

fn write_graph(graph: &Graph, writing_cfg: &configs::writing::network::Config) -> err::Feedback {
    // check if new file does already exist

    if writing_cfg.map_file.exists() {
        return Err(err::Msg::from(format!(
            "New map-file {} does already exist. Please remove it.",
            writing_cfg.map_file.display()
        )));
    }

    // writing to file

    let now = Instant::now();

    io::network::Writer::write(&graph, &writing_cfg)?;
    info!(
        "Finished writing in {} seconds ({} µs).",
        now.elapsed().as_secs(),
        now.elapsed().as_micros(),
    );
    info!("");

    Ok(())
}

fn parse_cmdline<'a>() -> CmdlineArgs {
    // arg: quiet
    let tmp = &[
        "Sets the logging-level by setting environment-variable 'RUST_LOG'.",
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
        .alias("parsing")
        .value_name("PATH")
        .help(
            "Sets the parser and other configurations according to this config. \
            See resources/blueprint.yaml for more info.",
        )
        .takes_value(true)
        .required(true);

    // all
    clap::App::new(env!("CARGO_PKG_NAME"))
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .long_about(
            (&[
                "",
                "This balancer takes a config-file, parses the chosen graph with specified \
                settings, and optimizes found routes with the provided balancing- and routing- \
                config before writing the balanced graph into a fmi-file. Optimizing means \
                generating a new metric.",
                "",
                "Hence a correct config-file contains following:",
                "- A parsing-config reading graph being balanced.",
                "- A balancing-config defining the settings for the balancer.",
                "- A routing-config specifying the routing-settings, which are used for \
                calculating the new metric.",
                "- A writing-config for exporting the balanced graph.",
                "",
                "You can visualize the results with the python-module",
                "py ./scripts/balancing/visualizer --results-dir <RESULTS_DIR/DATE>",
            ]
            .join("\n"))
                .as_ref(),
        )
        .arg(arg_log_level)
        .arg(arg_cfg)
        .get_matches()
        .into()
}

mod constants {
    pub mod ids {
        pub const MAX_LOG_LEVEL: &str = "max-log-level";
        pub const CFG: &str = "cfg";
    }
}

struct CmdlineArgs {
    max_log_level: String,
    cfg: String,
}

impl<'a> From<clap::ArgMatches<'a>> for CmdlineArgs {
    fn from(matches: clap::ArgMatches<'a>) -> CmdlineArgs {
        let max_log_level = matches
            .value_of(constants::ids::MAX_LOG_LEVEL)
            .expect(&format!("cmdline-arg: {}", constants::ids::MAX_LOG_LEVEL));
        let cfg = matches
            .value_of(constants::ids::CFG)
            .expect(&format!("cmdline-arg: {}", constants::ids::CFG));

        CmdlineArgs {
            max_log_level: String::from(max_log_level),
            cfg: String::from(cfg),
        }
    }
}