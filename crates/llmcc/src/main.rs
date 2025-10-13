use llmcc_rust::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: llmcc <file> [<file>...]");
        return Ok(());
    }

    let gcx = GlobalCtxt::from_files::<LangRust>(&files)?;

    for (index, _) in files.iter().enumerate() {
        let ctx = gcx.file_context(index);
        let tree = ctx.tree();
        let global_scope = ctx.alloc_scope(HirId(0));
        build_llmcc_ir::<LangRust>(&tree, ctx)?;
        let _ = collect_symbols(HirId(0), ctx, global_scope);
    }

    for (index, path) in files.iter().enumerate() {
        let ctx = gcx.file_context(index);
        let global_scope = ctx.alloc_scope(HirId(0));
        bind_symbols(HirId(0), ctx, global_scope);

        println!("== {} ==", path);
        print_llmcc_ir(HirId(0), ctx);

        build_llmcc_graph::<LangRust>(HirId(0), ctx)?;
        print_llmcc_graph(BlockId(0), ctx);
    }

    Ok(())
}
