use std::collections::HashSet;

use llmcc_rust::{
    bind_symbols, build_llmcc_graph, build_llmcc_ir, collect_symbols, BlockRelation, CompileCtxt,
    LangRust,
};

#[test]
fn cross_unit_dependencies_create_edges() {
    let sources = vec![
        b"fn helper() {}\n".to_vec(),
        b"fn caller() { helper(); }\n".to_vec(),
    ];

    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let globals = cc.create_globals();

    for index in 0..sources.len() {
        let unit = cc.compile_unit(index);
        build_llmcc_ir::<LangRust>(unit).expect("failed to build IR");
        collect_symbols(unit, globals);
    }

    let mut graph = cc.create_graph(globals);

    for index in 0..sources.len() {
        let unit = cc.compile_unit(index);
        bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph::<LangRust>(unit).expect("graph build failed");
        graph.add_child(unit_graph);
    }

    graph.link_units();

    let edges: HashSet<_> = graph
        .cross_unit_edges()
        .iter()
        .map(|edge| (edge.from.unit_index, edge.to.unit_index, edge.relation))
        .collect();

    assert_eq!(edges.len(), 1);
    assert!(edges.contains(&(1, 0, BlockRelation::Calls)));
}

#[test]
fn local_dependencies_do_not_create_cross_unit_edges() {
    let sources = vec![b"fn helper() {}\nfn caller() { helper(); }\n".to_vec()];

    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let globals = cc.create_globals();

    for index in 0..sources.len() {
        let unit = cc.compile_unit(index);
        build_llmcc_ir::<LangRust>(unit).expect("failed to build IR");
        collect_symbols(unit, globals);
    }

    let mut graph = cc.create_graph(globals);

    for index in 0..sources.len() {
        let unit = cc.compile_unit(index);
        bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph::<LangRust>(unit).expect("graph build failed");
        graph.add_child(unit_graph);
    }

    graph.link_units();

    assert!(graph.cross_unit_edges().is_empty());
}
