use llmcc_rust::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_file = &args[1];

    let gcx = GlobalCtxt::from_file::<LanguageRust>(input_file.clone()).unwrap();
    let ctx = gcx.create_context();
    let tree = gcx.tree();
    build_llmcc_ir::<LanguageRust>(&tree, &ctx);

    let root = HirId(0);
    resolve_symbols(root, &ctx);
    print_llmcc_ir(root, &ctx);

    build_llmcc_graph::<LanguageRust>(root, &ctx);
    print_llmcc_graph(BlockId(0), &ctx);
}
