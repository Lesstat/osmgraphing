use log::{debug, info};
use osmgraphing::{
    configs::{self, routing::RoutingAlgo},
    helpers::err,
    io,
    network::Graph,
};
use rand::SeedableRng;
use std::{path::Path, sync::Arc, time::Instant};

pub mod multithreading;

pub fn run(args: CmdlineArgs) -> err::Feedback {
    // check writing-cfg
    let _ = configs::writing::network::graph::Config::try_from_yaml(&args.cfg)?;
    let mut balancing_cfg = configs::balancing::Config::try_from_yaml(&args.cfg)?;

    info!("EXECUTE balancer");
    info!("Using balancer-seed={}", balancing_cfg.seed);

    let mut rng = rand_pcg::Pcg32::seed_from_u64(balancing_cfg.seed);

    // prepare simulation
    // e.g. creating the results-folder and converting the graph into the right format

    let custom_graph = simulation_pipeline::read_in_custom_graph(&args.cfg)?;
    // check routing-cfg
    let _ = configs::routing::Config::try_from_yaml(&args.cfg, custom_graph.cfg())?;

    // start balancing

    simulation_pipeline::prepare_results(&args.cfg, &mut balancing_cfg)?;

    let mut graph = custom_graph;
    for iter in 0..balancing_cfg.num_iter {
        // Iterate +1 to get analysis of new graph as well.
        // -> store graph before creating a new one

        if iter == balancing_cfg.num_iter - 1 {
            // store balanced graph

            let mut writing_cfg =
                configs::writing::network::graph::Config::try_from_yaml(&args.cfg)?;
            writing_cfg.map_file =
                balancing_cfg
                    .results_dir
                    .join(writing_cfg.map_file.file_name().ok_or(err::Msg::from(
                        "The provided route-pairs-file in the (routing-)config is not a file.",
                    ))?);
            write_graph(&graph, &writing_cfg)?;
        }

        // simulate and create new balanced graph

        simulation_pipeline::prepare_iteration(iter, &balancing_cfg)?;
        simulation_pipeline::write_multi_ch_graph(&balancing_cfg, graph, iter)?;
        simulation_pipeline::construct_ch_graph(&balancing_cfg, iter)?;
        let ch_graph = simulation_pipeline::read_in_ch_graph(&balancing_cfg, iter)?;
        let routing_cfg =
            simulation_pipeline::read_in_routing_cfg(&balancing_cfg, iter, &args.cfg, &ch_graph)?;

        let mut arc_ch_graph = Arc::new(ch_graph);
        simulation_pipeline::balance(
            iter,
            &balancing_cfg,
            &mut arc_ch_graph,
            &Arc::new(routing_cfg),
            &mut rng,
        )?;
        graph = Arc::try_unwrap(arc_ch_graph)
            .map_err(|_e| "The ch-graph should be owned by only one Arc.")?;
    }

    info!(
        "Execute py ./scripts/balancing/visualizer --results-dir {} to visualize.",
        balancing_cfg.results_dir.display()
    );

    Ok(())
}

mod simulation_pipeline {
    use super::multithreading;
    use chrono;
    use log::info;
    use osmgraphing::{configs, defaults, helpers::err, io, multi_ch_constructor, network::Graph};
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::Instant,
    };

    fn iter_dir(iter: usize, balancing_cfg: &configs::balancing::Config) -> PathBuf {
        balancing_cfg.results_dir.join(format!("{}", iter))
    }

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

        let iter_dir = iter_dir(iter, balancing_cfg);
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
        let iter_dir = iter_dir(iter, balancing_cfg);

        // writing graph

        let mut writing_cfg = configs::writing::network::graph::Config::try_from_yaml(
            &iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;
        // path is relative to results-dir
        writing_cfg.map_file = iter_dir.join(writing_cfg.map_file);

        super::write_graph(&graph, &writing_cfg)?;

        // writing edges

        let mut writing_cfg = configs::writing::network::edges::Config::try_from_yaml(
            &iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;
        // path is relative to results-dir
        writing_cfg.file = iter_dir.join(writing_cfg.file);

        super::write_edges(&graph, &writing_cfg)
    }

    pub fn construct_ch_graph(
        balancing_cfg: &configs::balancing::Config,
        iter: usize,
    ) -> err::Feedback {
        let mut mchc_cfg = balancing_cfg.multi_ch_constructor.clone();

        let is_using_new_metric = iter > 0;
        if !is_using_new_metric {
            mchc_cfg.dim -= 1;
        }

        let iter_dir = iter_dir(iter, balancing_cfg);
        mchc_cfg.fmi_graph = iter_dir.join(mchc_cfg.fmi_graph);
        mchc_cfg.ch_fmi_graph = iter_dir.join(mchc_cfg.ch_fmi_graph);

        mchc_cfg.cost_accuracy = defaults::accuracy::F64_ABS;

        // multi_ch_constructor::build(&mchc_cfg)?;
        multi_ch_constructor::construct_ch_graph(&mchc_cfg)
    }

    pub fn read_in_ch_graph(
        balancing_cfg: &configs::balancing::Config,
        iter: usize,
    ) -> err::Result<Graph> {
        let iter_dir = iter_dir(iter, balancing_cfg);
        let mut parsing_cfg = configs::parsing::Config::try_from_yaml(
            &iter_dir.join(defaults::balancing::files::ITERATION_CFG),
        )?;

        // map-file is stored relative to results-dir
        parsing_cfg.map_file = iter_dir.join(parsing_cfg.map_file);

        // same holds for edges-info.csv
        // -> update all paths to important map- or data-files

        let gen_cfg = parsing_cfg
            .generating
            .as_mut()
            .expect("Generating-section in parsing-cfg is expected.");
        for i in 0..gen_cfg.edges.categories.len() {
            let category = &mut gen_cfg.edges.categories[i];
            match category {
                configs::parsing::generating::edges::Category::Merge {
                    from,
                    is_file_with_header: _,
                    edge_id: _,
                    edges_info: _,
                } => *from = iter_dir.join(&from),
                configs::parsing::generating::edges::Category::Meta { info: _, id: _ }
                | configs::parsing::generating::edges::Category::Custom {
                    unit: _,
                    id: _,
                    default: _,
                }
                | configs::parsing::generating::edges::Category::Haversine { unit: _, id: _ }
                | configs::parsing::generating::edges::Category::Copy { from: _, to: _ }
                | configs::parsing::generating::edges::Category::Convert { from: _, to: _ }
                | configs::parsing::generating::edges::Category::Calc {
                    result: _,
                    a: _,
                    b: _,
                } => {
                    // no file to update
                }
            }
        }

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

            // The 'new_metric' is probably workload or something related to it.
            let new_metric_id = ch_graph
                .cfg()
                .edges
                .metrics
                .try_idx_of(&balancing_cfg.optimization.metric_id)?;
            routing_cfg.alphas[*new_metric_id] = 0.0;

            // -> and copy route-pairs-file into the results-directory
            match fs::copy(&old_route_pairs_file, &new_route_pairs_file) {
                Ok(_) => (),
                Err(e) => {
                    return Err(format!(
                        "Couldn't copy {} due to error: {}",
                        old_route_pairs_file.display(),
                        e
                    )
                    .into())
                }
            };
        }

        routing_cfg.route_pairs_file = Some(new_route_pairs_file);
        Ok(routing_cfg)
    }

    pub fn balance(
        iter: usize,
        balancing_cfg: &configs::balancing::Config,
        arc_ch_graph: &mut Arc<Graph>,
        arc_routing_cfg: &Arc<configs::routing::Config>,
        rng: &mut rand_pcg::Lcg64Xsh32,
    ) -> err::Feedback {
        info!(
            "Balance via explorating several routes for metrics {:?}x{:?}",
            arc_ch_graph.cfg().edges.metrics.units,
            arc_routing_cfg.alphas,
        );

        // reverse this vector to make splice efficient
        let route_pairs = io::routing::Parser::parse(&arc_routing_cfg)?;

        let mut master = multithreading::Master::spawn_some(
            balancing_cfg.num_threads,
            &arc_ch_graph,
            &arc_routing_cfg,
        )?;
        let (abs_workloads, chosen_paths) = master.work_off(
            route_pairs,
            &arc_ch_graph,
            rng,
            balancing_cfg.monitoring.is_writing_for_smarts,
        )?;

        // update graph with new values
        defaults::balancing::update_new_metric(
            iter,
            &abs_workloads,
            Arc::get_mut(arc_ch_graph).expect(
                "Mutable access to graph should be possible, since Arc should be the only owner.",
            ),
            &balancing_cfg,
        )?;

        // export density and iteration-results

        // measure writing-time
        let now = Instant::now();

        // write results from this iteration

        let stats_dir = iter_dir(iter, balancing_cfg).join(defaults::balancing::stats::DIR);

        let writing_cfg = configs::evaluating_balance::Config {
            seed: balancing_cfg.seed,
            results_dir: stats_dir.clone(),
            monitoring: balancing_cfg.monitoring.clone(),
            num_threads: balancing_cfg.num_threads,
        };
        io::evaluating_balance::Writer::write(&abs_workloads, &arc_ch_graph, &writing_cfg)?;
        // write SMARTS-paths
        if let Some(chosen_paths) = chosen_paths {
            let tmp_cfg = configs::writing::smarts::Config {
                file: writing_cfg
                    .results_dir
                    .join(defaults::smarts::XML_FILE_NAME),
            };
            io::smarts::Writer::write(&chosen_paths, &arc_ch_graph, &tmp_cfg)?;
        }

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

    let graph = io::network::graph::Parser::parse_and_finalize(parsing_cfg)?;

    info!(
        "FINISHED Parsed graph in {} seconds ({} µs).",
        now.elapsed().as_secs(),
        now.elapsed().as_micros(),
    );
    info!("");
    debug!("{}", graph);
    debug!("");

    Ok(graph)
}

fn write_graph(
    graph: &Graph,
    writing_cfg: &configs::writing::network::graph::Config,
) -> err::Feedback {
    // check if new file does already exist

    if writing_cfg.map_file.exists() {
        return Err(err::Msg::from(format!(
            "New map-file {} does already exist. Please remove it.",
            writing_cfg.map_file.display()
        )));
    }

    // writing to file

    let now = Instant::now();

    io::network::graph::Writer::write(&graph, &writing_cfg)?;
    info!(
        "Finished writing in {} seconds ({} µs).",
        now.elapsed().as_secs(),
        now.elapsed().as_micros(),
    );
    info!("");

    Ok(())
}

fn write_edges(
    graph: &Graph,
    writing_cfg: &configs::writing::network::edges::Config,
) -> err::Feedback {
    // check if new file does already exist

    if writing_cfg.file.exists() {
        return Err(err::Msg::from(format!(
            "New map-file {} does already exist. Please remove it.",
            writing_cfg.file.display()
        )));
    }

    // writing to file

    let now = Instant::now();

    io::network::edges::Writer::write(&graph, &writing_cfg)?;
    info!(
        "Finished writing in {} seconds ({} µs).",
        now.elapsed().as_secs(),
        now.elapsed().as_micros(),
    );
    info!("");

    Ok(())
}

pub struct CmdlineArgs {
    pub max_log_level: String,
    pub cfg: String,
}
