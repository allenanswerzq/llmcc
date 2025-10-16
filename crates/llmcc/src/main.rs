use llmcc_rust::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage: llmcc <file> [<file>...]");
        return Ok(());
    }

    let cc = CompileCtxt::from_files::<LangRust>(&files)?;
    let globals = cc.create_globals();

    for (index, path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        build_llmcc_ir::<LangRust>(unit)?;

        println!("== {} ==", path);
        print_llmcc_ir(unit);

        collect_symbols(unit, globals);
    }

    let mut graph = cc.create_graph();
    for (index, _path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        bind_symbols(unit, globals);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, index)?;
        print_llmcc_graph(unit_graph.root(), unit);
        graph.add_child(unit_graph);
    }
    graph.link_units(&cc);

    Ok(())
}
