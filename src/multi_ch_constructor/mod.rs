mod configs;
pub use configs::Config;
use log::info;

pub(crate) mod defaults;

use crate::helpers::err;
use std::{fs, path::Path};

const REPO_DIR: &str = "externals/multi-ch-constructor";

pub fn build(mchc_cfg: &configs::Config) -> err::Feedback {
    let mut cmd_args = Vec::new();
    cmd_args.push("-Bbuild");

    // graph-dim

    cmd_args.push("-D");
    let cmd_arg = &format!("GRAPH_DIM={}", mchc_cfg.dim);
    cmd_args.push(cmd_arg);

    // cost-accuracy

    cmd_args.push("-D");
    let cmd_arg = &format!("COST_ACCURACY={}", mchc_cfg.cost_accuracy);
    cmd_args.push(cmd_arg);

    let repo_dir = fs::canonicalize(&REPO_DIR)?;
    let is_successful = std::process::Command::new("cmake")
        .current_dir(&repo_dir)
        .args(&cmd_args)
        .status()?
        .success();
    if !is_successful {
        return Err(format!("Failed: cmake {}", cmd_args.join(" ")).into());
    }

    let cmd_args = &["--build", "build"];
    let is_successful = std::process::Command::new("cmake")
        .current_dir(repo_dir)
        .args(cmd_args)
        .status()?
        .success();
    if !is_successful {
        return Err(format!("Failed: cmake {}", cmd_args.join(" ")).into());
    }

    Ok(())
}

/// Expects `graph.fmi` in `graph_dir` and creates `graph.ch.fmi` in `graph_dir`.
pub fn construct_ch_graph(mchc_cfg: &configs::Config) -> err::Feedback {
    let cmd_args = &[
        "--threads",
        &format!("{}", mchc_cfg.num_threads),
        if mchc_cfg.is_printing_osm_ids {
            "--using-osm-ids"
        } else {
            ""
        },
        if mchc_cfg.is_using_external_edge_ids {
            "--external-edge-ids"
        } else {
            ""
        },
        "--text",
        &format!("{}", mchc_cfg.fmi_graph.to_string_lossy()),
        "--percent",
        &format!("{}", &mchc_cfg.contraction_ratio),
        "--write",
        &format!("{}", mchc_cfg.ch_fmi_graph.to_string_lossy()),
    ];
    let exec_path = Path::new(&REPO_DIR)
        .join("build")
        .join(format!("multi-ch{}", mchc_cfg.dim));

    if !exec_path.exists() {
        info!(
            "multi-ch is not yet compiled for dimension {}. building",
            mchc_cfg.dim
        );
        build(mchc_cfg)?;
    }
    let is_successful = std::process::Command::new(exec_path)
        .args(cmd_args)
        .status()?
        .success();
    if !is_successful {
        return Err(err::Msg::from(format!(
            "{}{}{}{}{}",
            "Failed: ./externals/multi-ch-constructor/build/multi-ch ",
            cmd_args.join(" "),
            "\n",
            "Maybe you have a graph-file with edges defined by the nodes' ids,",
            " but the multi-ch-constructor needs the nodes' indices."
        )));
    }

    Ok(())
}
