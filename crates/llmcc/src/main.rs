use llmcc_rust::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: llmcc <file> [<file>...]");
        return Ok(());
    }

    let gcx = GlobalCtxt::from_files::<LangRust>(&files)?;
    let registry = SymbolRegistry::default();

    for (index, _) in files.iter().enumerate() {
        let ctx = gcx.create_context(index);
        build_llmcc_ir::<LangRust>(ctx.tree(), ctx)?;
        collect_symbols(HirId(0), ctx, &registry);
    }

    for (index, path) in files.iter().enumerate() {
        let ctx = gcx.create_context(index);
        bind_symbols(HirId(0), ctx, &registry);

        println!("== {} ==", path);
        print_llmcc_ir(HirId(0), ctx);

        build_llmcc_graph::<LangRust>(HirId(0), ctx)?;
        print_llmcc_graph(BlockId(0), ctx);
    }

    Ok(())
}
