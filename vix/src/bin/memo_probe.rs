use vix::exec::Tree;
use vix::machine::{Machine, MachineArg};

const SOURCE: &str =
    include_str!("../../../playgrounds/snark/src/bundled/vix/samples/cargo_manifest.vix");
const RODIN_SOURCE: &str = include_str!("../../../rodin/rodin.vix");

fn try_render(machine: &Machine, label: &str, value: i64, schema: &str) {
    match machine.render_value(schema, value) {
        Ok(rendered) => println!("{label} as {schema}: {rendered:?}"),
        Err(err) => println!("{label} as {schema}: ERR {err}"),
    }
}

fn main() {
    let source = format!("{RODIN_SOURCE}\n\n{SOURCE}");
    let mut machine = Machine::load(&source).expect("load");

    let names = [
        "workspace_member_only_solve_selected_versions_text",
        "workspace_member_only_index_state",
        "workspace_member_only_solve_selected_member_count",
        "workspace_member_only_solve_selected_versions_text_limit",
        "solve_selected_versions_text_from_workspace_state",
        "solve_selected_versions_text",
        "workspace_index_from_state",
        "selected_versions_text_count",
    ];

    for name in names {
        match machine.entry_param_schemas(name) {
            Some(params) => println!("{name} params={params:?}"),
            None => println!("missing {name}"),
        }
    }

    let workspace = machine
        .intern_arg(
            "Tree",
            MachineArg::Tree(Tree::of(&[
                (
                    "Cargo.toml",
                    r#"[workspace]
members = ["facet"]"#,
                ),
                (
                    "facet/Cargo.toml",
                    r#"[package]
name = "facet"
version = "0.50.0-rc.5"
edition = "2024""#,
                ),
            ])),
        )
        .expect("intern tree")
        .0;
    let root = machine
        .intern_arg("Path", MachineArg::Path(String::new()))
        .expect("path")
        .0;
    let target = machine
        .intern_arg(
            "String",
            MachineArg::String("x86_64-apple-darwin".to_string()),
        )
        .expect("target")
        .0;

    println!("arg handles: workspace={workspace} root={root} target={target}");
    try_render(&machine, "workspace", workspace, "Tree");
    try_render(&machine, "workspace", workspace, "WorkspaceIndexState");
    try_render(&machine, "workspace", workspace, "Index");
    try_render(&machine, "workspace", workspace, "Path");
    try_render(&machine, "workspace", workspace, "String");

    try_render(&machine, "root", root, "Path");
    try_render(&machine, "root", root, "String");
    try_render(&machine, "root", root, "Tree");
    try_render(&machine, "root", root, "WorkspaceIndexState");

    try_render(&machine, "target", target, "String");
    try_render(&machine, "target", target, "Path");
    try_render(&machine, "target", target, "Tree");

    println!(
        "workspace_index_state -> {}",
        machine
            .demand_i64("workspace_index_state", vec![workspace, root])
            .expect("workspace_index_state")
    );
    let state = machine
        .demand_i64("workspace_index_state", vec![workspace, root])
        .expect("workspace_index_state");
    try_render(&machine, "state", state, "WorkspaceIndexState");
    try_render(&machine, "state", state, "Index");
    println!(
        "workspace_member_only_index_state -> {}",
        machine
            .demand_i64("workspace_member_only_index_state", vec![workspace, root])
            .expect("workspace_member_only_index_state")
    );
    let _member_state = machine
        .demand_i64("workspace_member_only_index_state", vec![workspace, root])
        .expect("workspace_member_only_index_state");

    println!(
        "workspace_index_from_state -> {}",
        machine
            .demand_i64("workspace_index_from_state", vec![state])
            .expect("workspace_index_from_state")
    );
    let index = machine
        .demand_i64("workspace_index_from_state", vec![state])
        .expect("workspace_index_from_state");
    try_render(&machine, "index", index, "Index");
    try_render(&machine, "index", index, "WorkspaceIndexState");

    let problem = match machine.demand_i64("workspace_problem", vec![]) {
        Ok(problem) => {
            println!("workspace_problem -> {problem}");
            problem
        }
        Err(err) => {
            println!("workspace_problem ERROR: {err}");
            panic!("workspace_problem failed");
        }
    };
    try_render(&machine, "problem", problem, "Problem");
    try_render(&machine, "problem", problem, "Index");
    try_render(&machine, "problem", problem, "WorkspaceIndexState");
    try_render(&machine, "problem", problem, "String");
    println!(
        "solve_selected_versions_text_from_workspace_state return: {:?}",
        machine.entry_return_schema("solve_selected_versions_text_from_workspace_state")
    );
    println!(
        "solve_selected_versions_text return: {:?}",
        machine.entry_return_schema("solve_selected_versions_text")
    );
    println!(
        "workspace_member_only_solve_selected_versions_text return: {:?}",
        machine.entry_return_schema("workspace_member_only_solve_selected_versions_text")
    );
    println!(
        "workspace_member_solve_selected_versions_text return: {:?}",
        machine.entry_return_schema("workspace_member_solve_selected_versions_text")
    );
    for name in [
        "solve",
        "solve_selected_versions_text",
        "solve_selected_versions_text_from_workspace_state",
        "selected_versions_text_from",
        "selected_versions_text_tuple",
        "selected_versions_text_one",
        "selected_version_text",
        "selected_version_text_from",
        "selected_version_text_tuple",
        "selected_version_text_one_id",
        "workspace_index_from_state",
    ] {
        println!("{name} params={:?}", machine.entry_param_schemas(name));
    }

    let output = machine
        .demand_i64(
            "solve_selected_versions_text_from_workspace_state",
            vec![state, problem, target],
        )
        .expect("solve_selected_versions_text_from_workspace_state");
    println!("solve_selected_versions_text_from_workspace_state = {output}");

    let output = machine
        .demand_i64("solve_selected_versions_text", vec![index, problem, target])
        .expect("solve_selected_versions_text");
    println!("solve_selected_versions_text direct = {output}");

    let output = machine
        .demand_i64(
            "workspace_member_only_solve_selected_versions_text",
            vec![workspace, root, target],
        )
        .expect("workspace_member_only_solve_selected_versions_text");
    println!("workspace_member_only_solve_selected_versions_text = {output}");

    let output = machine
        .demand_i64(
            "workspace_member_solve_selected_versions_text",
            vec![workspace, root, target],
        )
        .expect("workspace_member_solve_selected_versions_text");
    println!("workspace_member_solve_selected_versions_text = {output}");

    let count = machine
        .demand_i64(
            "workspace_member_only_solve_selected_member_count",
            vec![workspace, root, target],
        )
        .expect("workspace_member_only_solve_selected_member_count");
    println!("workspace_member_only_solve_selected_member_count = {count}");
}
