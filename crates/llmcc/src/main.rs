use llmcc_rust::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let input_file = &args[1];

    let gcx = GlobalCtxt::from_file::<LangRust>(input_file.clone()).unwrap();
    let ctx = gcx.create_context();
    let tree = gcx.tree();
    let globals = SymbolRegistry::default();
    build_llmcc_ir::<LangRust>(&tree, ctx)?;

    let root = HirId(0);
    resolve_symbols(root, ctx, &globals);
    print_llmcc_ir(root, ctx);

    build_llmcc_graph::<LangRust>(root, ctx)?;
    print_llmcc_graph(BlockId(0), ctx);

    Ok(())
}
