use llmcc_core::context::CompileCtxt;
use llmcc_core::graph_builder::{
    build_llmcc_graph_with_config,
    BlockRelation,
    GraphBuildConfig,
    ProjectGraph,
};
use llmcc_core::ir_builder::{build_llmcc_ir_with_config, IrBuildConfig};
use llmcc_core::LanguageTrait;
use llmcc_rust::LangRust;

#[test]
fn compact_project_graph_includes_enum_dependencies() {
    let source = r#"
        enum AskForApproval {
            OnRequest,
        }

        enum Op {
            UserTurn { approval_policy: AskForApproval },
        }
    "#;

    let cc = CompileCtxt::from_sources::<LangRust>(&[source.as_bytes().to_vec()]);
    build_llmcc_ir_with_config::<LangRust>(&cc, IrBuildConfig::default()).unwrap();
    let globals = cc.create_globals();

    for index in 0..cc.files.len() {
        let unit = cc.compile_unit(index);
        LangRust::collect_symbols(unit, globals);
    }

    let mut project = ProjectGraph::new(&cc);
    for index in 0..cc.files.len() {
        let unit = cc.compile_unit(index);
        LangRust::bind_symbols(unit, globals);
        let graph = build_llmcc_graph_with_config::<LangRust>(
            unit,
            index,
            GraphBuildConfig::compact(),
        )
        .unwrap();
        project.add_child(graph);
    }

    project.link_units();

    let block_indexes = cc.block_indexes.borrow();
    let op_info = block_indexes.find_by_name("Op");
    assert_eq!(op_info.len(), 1, "expected a single Op block, got {op_info:?}");
    let (op_unit, _, op_block) = op_info[0];

    let approval_info = block_indexes.find_by_name("AskForApproval");
    assert_eq!(
        approval_info.len(),
        1,
        "expected a single AskForApproval block, got {approval_info:?}"
    );
    let (approval_unit, _, approval_block) = approval_info[0];

    assert_eq!(
        op_unit, approval_unit,
        "Op and AskForApproval should be in same unit"
    );

    let unit_graph = project.unit_graph(op_unit).expect("missing unit graph");
    let op_symbol = project
        .cc
        .find_symbol_by_block_id(op_block)
        .expect("Op symbol");
    let approval_symbol = project
        .cc
        .find_symbol_by_block_id(approval_block)
        .expect("AskForApproval symbol");
    assert!(
        op_symbol
            .depends
            .borrow()
            .contains(&approval_symbol.id),
        "Symbol dependencies missing AskForApproval"
    );
    let dependencies = unit_graph
        .edges()
        .get_related(op_block, BlockRelation::DependsOn);

    assert!(
        dependencies.contains(&approval_block),
        "Op dependencies missing AskForApproval: {dependencies:?}"
    );
}
