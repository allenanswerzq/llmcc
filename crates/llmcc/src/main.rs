use clap::Parser;
use llmcc_rust::*;

#[derive(Parser, Debug)]
#[command(name = "llmcc")]
#[command(about = "llmcc: llm context compiler")]
#[command(version)]
struct Args {
    /// Files to compile
    #[arg(value_name = "FILE", required_unless_present = "dir")]
    files: Vec<String>,

    /// Load all .rs files from a directory (recursive)
    #[arg(short, long, value_name = "DIR")]
    dir: Option<String>,

    /// Print intermediate representation (IR)
    #[arg(long, default_value_t = false)]
    print_ir: bool,

    /// Print project graph
    #[arg(long, default_value_t = true)]
    print_graph: bool,

    /// Don't print IR (use with other flags to disable default)
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_print_ir: bool,

    /// Don't print graph
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_print_graph: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();

    // Handle negation flags
    if args.no_print_ir {
        args.print_ir = false;
    }
    if args.no_print_graph {
        args.print_graph = false;
    }

    let (cc, files) = if let Some(dir) = args.dir {
        eprintln!("Loading .rs files from directory: {}", dir);
        let ctx = CompileCtxt::from_dir::<_, LangRust>(&dir)?;
        let file_paths = ctx.get_files();
        eprintln!("Found {} .rs files", file_paths.len());
        (ctx, file_paths)
    } else {
        let cc = CompileCtxt::from_files::<LangRust>(&args.files)?;
        (cc, args.files)
    };

    let globals = cc.create_globals();

    // Build IR and optionally print
    for (index, path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        build_llmcc_ir::<LangRust>(unit)?;

        if args.print_ir {
            print_llmcc_ir(unit);
        }

        collect_symbols(unit, globals);
    }

    // Build graph and optionally print
    let mut graph = ProjectGraph::new(&cc);
    for (index, path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        bind_symbols(unit, globals);

        let unit_graph = build_llmcc_graph::<LangRust>(unit, index)?;

        if args.print_graph {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        graph.add_child(unit_graph);
    }
    graph.link_units();

    Ok(())
}
