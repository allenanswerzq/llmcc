use llmcc_rust::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: llmcc <file> [<file>...]");
        return Ok(());
    }

    let gcx = GlobalCtxt::from_files::<LangRust>(&files)?;
    let globals = gcx.create_globals();

    for (index, _) in files.iter().enumerate() {
        let ctx = gcx.file_context(index);
        build_llmcc_ir::<LangRust>(&ctx.tree(), ctx)?;

        let root = ctx.file_start_hir_id().unwrap();
        let _ = collect_symbols(root, ctx, globals);
    }

    for (index, path) in files.iter().enumerate() {
        let ctx = gcx.file_context(index);
        let root = ctx.file_start_hir_id().unwrap();
        bind_symbols(root, ctx, globals);

        println!("== {} ==", path);
        print_llmcc_ir(root, ctx);

        build_llmcc_graph::<LangRust>(root, ctx)?;
        print_llmcc_graph(BlockId(0), ctx);
    }

    Ok(())
}
