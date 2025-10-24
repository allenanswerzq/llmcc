use clap::Parser;
use llmcc_core::lang_def::LanguageTrait;
use llmcc_python::*;
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

    /// Language to use: 'rust' or 'python'
    #[arg(long, value_name = "LANG", default_value = "rust")]
    lang: String,

    /// Print intermediate representation (IR)
    #[arg(long, default_value_t = false)]
    print_ir: bool,

    /// Print project graph
    #[arg(long, default_value_t = false)]
    print_graph: bool,

    /// Name of the symbol/function to query (enables find_depends mode)
    #[arg(long, value_name = "NAME")]
    query: Option<String>,

    /// Search recursively for transitive dependencies (default: direct dependencies only)
    #[arg(long, default_value_t = false)]
    recursive: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.lang.as_str() {
        "rust" => run_main::<LangRust>(&args),
        "python" => run_main::<LangPython>(&args),
        _ => Err(format!("Unknown language: {}. Use 'rust' or 'python'", args.lang).into()),
    }
}

fn run_main<L: LanguageTrait>(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let (cc, files) = if let Some(dir) = args.dir.as_ref() {
        eprintln!(" loading .rs files from directory: {}", dir);
        let ctx = CompileCtxt::from_dir::<_, L>(dir)?;
        let file_paths = ctx.get_files();
        eprintln!(" found {} .rs files", file_paths.len());
        (ctx, file_paths)
    } else {
        let cc = CompileCtxt::from_files::<L>(&args.files)?;
        (cc, args.files.clone())
    };

    build_llmcc_ir::<L>(&cc)?;

    let globals = cc.create_globals();

    if args.print_ir {
        for (index, _path) in files.iter().enumerate() {
            let unit = cc.compile_unit(index);
            print_llmcc_ir(unit);
        }
    }

    for (index, _path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::collect_symbols(unit, globals);
    }

    let mut pg = ProjectGraph::new(&cc);
    for (index, _path) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph::<L>(unit, index)?;

        if args.print_graph {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        pg.add_child(unit_graph);
    }

    pg.link_units();

    if let Some(name) = args.query.as_ref() {
        let query = ProjectQuery::new(&pg);
        let output = if args.recursive {
            query.find_depends_recursive(name).format_for_llm()
        } else {
            query.find_depends(name).format_for_llm()
        };
        println!("{}", output);
    }

    Ok(())
}
