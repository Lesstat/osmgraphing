use criterion::{black_box, criterion_group, criterion_main, Criterion};
use log::error;
use osmgraphing::{
    configs::{self, Config},
    helpers,
    io::Parser,
    network::{Graph, NodeIdx},
    routing,
};

fn criterion_benchmark(c: &mut Criterion) {
    helpers::init_logging("WARN", vec![]).expect("No user-input, so this should be fine.");

    // parsing
    let cfg = Config::from_yaml("resources/configs/isle-of-man.pbf.yaml").unwrap();

    // create graph
    let graph = match Parser::parse_and_finalize(cfg.parser) {
        Ok(graph) => graph,
        Err(msg) => {
            error!("{}", msg);
            return;
        }
    };
    let nodes = graph.nodes();

    // routing
    let labelled_routes = vec![
        // short route (~3 km)
        (
            "",
            " with short routes (~3 km)",
            vec![(
                nodes.idx_from(283_500_532).expect("A"),
                nodes.idx_from(283_501_263).expect("B"),
            )],
        ),
        // medium route (~30 km)
        (
            "",
            " with medium routes (~30 km)",
            vec![(
                nodes.idx_from(283_483_998).expect("C"),
                nodes.idx_from(1_746_745_421).expect("D"),
            )],
        ),
        // long route (~56 km)
        (
            "",
            " with long routes (~56 km)",
            vec![(
                nodes.idx_from(1_151_603_193).expect("E"),
                nodes.idx_from(456_478_793).expect("F"),
            )],
        ),
    ];

    // benchmarking shortest routing
    let routing_strs = vec![
        "routing: [{ id: 'Meters' }]",
        "routing: [{ id: 'Meters' }, { id: 'Seconds' }]",
    ];
    for routing_str in routing_strs {
        let routing_cfg = configs::routing::Config::from_str(routing_str, graph.cfg())
            .expect("MetricIds should be provided.");

        for (prefix, suffix, routes) in labelled_routes.iter() {
            c.bench_function(
                &format!("{}Shortest Dijkstra (bidir){}", prefix, suffix),
                |b| {
                    b.iter(|| {
                        bidir_shortest_dijkstra(
                            black_box(&graph),
                            black_box(&routes),
                            black_box(&routing_cfg),
                        )
                    })
                },
            );
        }

        // benchmarking fastest routing
        for (prefix, suffix, routes) in labelled_routes.iter() {
            c.bench_function(
                &format!("{}Fastest Dijkstra (bidir){}", prefix, suffix),
                |b| {
                    b.iter(|| {
                        bidir_fastest_dijkstra(
                            black_box(&graph),
                            black_box(&routes),
                            black_box(&routing_cfg),
                        )
                    })
                },
            );
        }
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

//------------------------------------------------------------------------------------------------//

fn bidir_shortest_dijkstra(
    graph: &Graph,
    routes: &Vec<(NodeIdx, NodeIdx)>,
    cfg: &configs::routing::Config,
) {
    let mut dijkstra = routing::Dijkstra::new();

    let nodes = graph.nodes();
    for &(src_idx, dst_idx) in routes.iter() {
        let src = nodes.create(src_idx);
        let dst = nodes.create(dst_idx);
        let _option_path = dijkstra.compute_best_path(&src, &dst, graph, cfg);
    }
}

fn bidir_fastest_dijkstra(
    graph: &Graph,
    routes: &Vec<(NodeIdx, NodeIdx)>,
    cfg: &configs::routing::Config,
) {
    let mut dijkstra = routing::Dijkstra::new();

    let nodes = graph.nodes();
    for &(src_idx, dst_idx) in routes.iter() {
        let src = nodes.create(src_idx);
        let dst = nodes.create(dst_idx);
        let _option_path = dijkstra.compute_best_path(&src, &dst, graph, cfg);
    }
}
