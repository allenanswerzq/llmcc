use clap::Parser;
use llmcc_core::ir_builder;
use llmcc_core::query::ProjectQuery;
use llmcc_core::graph_builder::ProjectGraph;
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

    /// Query mode: find, find_related, find_related_recursive, list_functions, list_structs
    #[arg(long, value_name = "MODE")]
    query: Option<String>,

    /// Name of the symbol/function to query
    #[arg(long, value_name = "NAME")]
    query_name: Option<String>,

    /// File/unit index for file_structure query
    #[arg(long, value_name = "INDEX")]
    query_unit: Option<usize>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();

    if args.no_print_ir {
        args.print_ir = false;
    }
    if args.no_print_graph {
        args.print_graph = false;
    }

    let (cc, files) = if let Some(dir) = args.dir {
        eprintln!(" loading .rs files from directory: {}", dir);
        let ctx = CompileCtxt::from_dir::<_, LangRust>(&dir)?;
        let file_paths = ctx.get_files();
        eprintln!(" found {} .rs files", file_paths.len());
        (ctx, file_paths)
    } else {
        let cc = CompileCtxt::from_files::<LangRust>(&args.files)?;
        (cc, args.files)
    };

    ir_builder::build_llmcc_ir::<LangRust>(&cc)?;

    let globals = cc.create_globals();

    if args.print_ir {
        for (index, _path) in files.iter().enumerate() {
            let unit = cc.compile_unit(index);
            print_llmcc_ir(unit);
        }
    }

    for (index, _path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        collect_symbols(unit, globals);
    }

    let mut pg = ProjectGraph::new(&cc);
    for (index, _path) in files.iter().enumerate() {
        let unit: CompileUnit<'_> = cc.compile_unit(index);
        bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph::<LangRust>(unit, index)?;

        if args.print_graph {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        pg.add_child(unit_graph);
    }

    pg.link_units();

    // Handle query mode
    if let Some(query_mode) = args.query {
        let query = ProjectQuery::new(&pg);
        let output = match query_mode.as_str() {
            "find" => {
                if let Some(name) = args.query_name {
                    query.find_by_name(&name).format_for_llm()
                } else {
                    eprintln!("Error: --query_name is required for 'find' mode");
                    std::process::exit(1);
                }
            }
            "find_related" => {
                if let Some(name) = args.query_name {
                    query.find_related(&name).format_for_llm()
                } else {
                    eprintln!("Error: --query_name is required for 'find_related' mode");
                    std::process::exit(1);
                }
            }
            "find_related_recursive" => {
                if let Some(name) = args.query_name {
                    query.find_related_recursive(&name).format_for_llm()
                } else {
                    eprintln!("Error: --query_name is required for 'find_related_recursive' mode");
                    std::process::exit(1);
                }
            }
            "list_functions" => {
                query.find_all_functions().format_for_llm()
            }
            "list_structs" => {
                query.find_all_structs().format_for_llm()
            }
            "file_structure" => {
                let unit_idx = args.query_unit.unwrap_or(0);
                query.file_structure(unit_idx).format_for_llm()
            }
            _ => {
                eprintln!(
                    "Unknown query mode: {}. Valid modes are: find, find_related, find_related_recursive, list_functions, list_structs, file_structure",
                    query_mode
                );
                std::process::exit(1);
            }
        };

        println!("{}", output);
    }

    Ok(())
}
