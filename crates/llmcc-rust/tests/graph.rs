use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_rust::{
    bind_symbols, build_llmcc_graph, build_llmcc_ir, collect_symbols, BlockId, BlockRelation,
    CompileCtxt, CompileUnit, LangRust, ProjectGraph, UnitGraph,
};

struct GraphFixture<'tcx> {
    cc: &'tcx CompileCtxt<'tcx>,
    globals: &'tcx Scope<'tcx>,
    unit_count: usize,
}

impl GraphFixture<'static> {
    fn new(sources: &[&str]) -> Self {
        let bytes: Vec<Vec<u8>> = sources.iter().map(|src| src.as_bytes().to_vec()).collect();
        let cc: &'static CompileCtxt<'static> =
            Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&bytes)));
        let globals = cc.create_globals();
        Self {
            cc,
            globals,
            unit_count: bytes.len(),
        }
    }
}

impl<'tcx> GraphFixture<'tcx> {
    fn unit(&self, index: usize) -> CompileUnit<'tcx> {
        self.cc.compile_unit(index)
    }

    fn build_project_graph(&self) -> ProjectGraph {
        let order: Vec<usize> = (0..self.unit_count).collect();
        self.build_project_graph_with_order(&order)
    }

    fn build_project_graph_with_order(&self, order: &[usize]) -> ProjectGraph {
        assert_eq!(
            order.len(),
            self.unit_count,
            "expected build order covering all units",
        );

        for index in 0..self.unit_count {
            let unit = self.unit(index);
            build_llmcc_ir::<LangRust>(unit).expect("failed to build IR");
            collect_symbols(unit, self.globals);
        }

        let mut graph = self.cc.create_graph();
        for &index in order {
            let unit = self.unit(index);
            bind_symbols(unit, self.globals);
            let unit_graph =
                build_llmcc_graph::<LangRust>(unit, index).expect("graph build failed");
            graph.add_child(unit_graph);
        }

        graph.link_units();
        graph
    }

    fn block_id_for(&self, unit_index: usize, name: &str, kind: SymbolKind) -> BlockId {
        if let Some(symbol) = self.lookup_symbol(unit_index, name, Some(kind)) {
            if let Some(block) = symbol.block_id() {
                return block;
            }
            panic!(
                "symbol `{name}` (kind {:?}, unit {:?}) missing block id",
                symbol.kind(),
                symbol.unit_index()
            );
        }

        if let Some(symbol) = self.lookup_symbol(unit_index, name, None) {
            if let Some(block) = symbol.block_id() {
                return block;
            }
            panic!(
                "symbol `{name}` (kind {:?}, unit {:?}) missing block id",
                symbol.kind(),
                symbol.unit_index()
            );
        }

        panic!("missing block id for `{name}` in unit {unit_index}");
    }

    fn function_block_id(&self, unit_index: usize, name: &str) -> BlockId {
        self.block_id_for(unit_index, name, SymbolKind::Function)
    }

    fn struct_block_id(&self, unit_index: usize, name: &str) -> BlockId {
        self.block_id_for(unit_index, name, SymbolKind::Struct)
    }

    fn lookup_symbol(
        &self,
        unit_index: usize,
        name: &str,
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        let mut stack = ScopeStack::new(&self.cc.arena, &self.cc.interner);
        stack.push(self.globals);
        let key = self.cc.interner.intern(name);

        stack
            .find_global_suffix_with_filters(&[key], kind, Some(unit_index))
            .or_else(|| stack.find_global_suffix_with_filters(&[key], kind, None))
            .or_else(|| stack.find_global_suffix_with_filters(&[key], None, Some(unit_index)))
            .or_else(|| stack.find_global_suffix_with_filters(&[key], None, None))
    }

    fn unit_graph<'a>(&self, graph: &'a ProjectGraph, index: usize) -> &'a UnitGraph {
        graph
            .units()
            .iter()
            .find(|unit| unit.unit_index() == index)
            .unwrap_or_else(|| panic!("missing unit graph for index {index}"))
    }

    fn assert_no_unresolved(&self) {
        assert!(
            self.cc.unresolve_symbols.borrow().is_empty(),
            "expected unresolved symbol queue to be empty"
        );
    }
}

#[test]
fn single_unit_function_call_creates_edge() {
    let fixture = GraphFixture::new(&["fn helper() {}\nfn caller() { helper(); }\n"]);
    let graph = fixture.build_project_graph();

    let unit_graph = fixture.unit_graph(&graph, 0);
    let caller_block = fixture.function_block_id(0, "caller");
    let helper_block = fixture.function_block_id(0, "helper");

    assert_eq!(
        unit_graph.edges().get_depends(caller_block),
        vec![helper_block]
    );
    assert_eq!(
        unit_graph.edges().get_depended(helper_block),
        vec![caller_block]
    );
    fixture.assert_no_unresolved();
}

#[test]
fn cross_unit_call_creates_bidirectional_edges() {
    let fixture = GraphFixture::new(&["fn helper() {}\n", "fn caller() { helper(); }\n"]);
    let graph = fixture.build_project_graph_with_order(&[1, 0]);

    let helper_block = fixture.function_block_id(0, "helper");
    let caller_block = fixture.function_block_id(1, "caller");

    let caller_unit = fixture.unit_graph(&graph, 1);
    let helper_unit = fixture.unit_graph(&graph, 0);

    assert!(caller_unit
        .edges()
        .has_relation(caller_block, BlockRelation::DependsOn, helper_block));
    assert!(helper_unit.edges().has_relation(
        helper_block,
        BlockRelation::DependedBy,
        caller_block
    ));

    fixture.assert_no_unresolved();
}

#[test]
fn type_reference_creates_dependency_edge() {
    let fixture = GraphFixture::new(&["struct Foo;\n", "fn use_type(_: Foo) {}\n"]);
    let graph = fixture.build_project_graph();

    let foo_block = fixture.struct_block_id(0, "Foo");
    let use_type_block = fixture.function_block_id(1, "use_type");

    let use_type_unit = fixture.unit_graph(&graph, 1);
    let foo_unit = fixture.unit_graph(&graph, 0);

    assert!(use_type_unit.edges().has_relation(
        use_type_block,
        BlockRelation::DependsOn,
        foo_block
    ));
    assert!(foo_unit
        .edges()
        .has_relation(foo_block, BlockRelation::DependedBy, use_type_block));

    fixture.assert_no_unresolved();
}

#[test]
fn duplicate_dependencies_are_deduped() {
    let fixture = GraphFixture::new(&["fn helper() {}\nfn caller() { helper(); helper(); }\n"]);
    let graph = fixture.build_project_graph();

    let unit_graph = fixture.unit_graph(&graph, 0);
    let caller_block = fixture.function_block_id(0, "caller");
    let helper_block = fixture.function_block_id(0, "helper");

    assert_eq!(
        unit_graph.edges().get_depends(caller_block),
        vec![helper_block]
    );
    fixture.assert_no_unresolved();
}
