use anyhow::Result;
use anyhow::anyhow;
use clap::ArgGroup;
use clap::Parser;

use llmcc::LlmccOptions;
use llmcc::run_main;
use llmcc_python::LangPython;
use llmcc_rust::LangRust;

#[derive(Parser, Debug)]
#[command(
    name = "llmcc",
    about = "llmcc: llm context compiler",
    version,
    group = ArgGroup::new("inputs").required(true).args(["files", "dirs"])
)]
pub struct Args {
    /// Individual files to compile (repeatable)
    #[arg(
        short = 'f',
        long = "file",
        value_name = "FILE",
        num_args = 1..,
        action = clap::ArgAction::Append,
        conflicts_with = "dirs"
    )]
    files: Vec<String>,

    /// Directories to scan recursively (repeatable)
    #[arg(
        short = 'd',
        long = "dir",
        value_name = "DIR",
        num_args = 1..,
        action = clap::ArgAction::Append,
        conflicts_with = "files"
    )]
    dirs: Vec<String>,

    /// Language to use: 'rust' or 'python'
    #[arg(long, value_name = "LANG", default_value = "rust")]
    lang: String,

    /// Print intermediate representation (IR), internal debugging output
    #[arg(long, default_value_t = false)]
    print_ir: bool,

    /// Print basic block graph
    #[arg(long, default_value_t = false)]
    print_block: bool,

    /// Render a scoped design graph for the provided files or directories
    #[arg(
        long = "design-graph",
        default_value_t = false,
        conflicts_with_all = ["depends", "dependents", "query"]
    )]
    design_graph: bool,

    /// Summarize query output with file path and line range instead of full code blocks
    #[arg(long, default_value_t = false)]
    summary: bool,

    /// Use page rank algorithm to filter the most important nodes in the high graph
    #[arg(long, default_value_t = false)]
    pagerank: bool,

    /// Top k nodes to select using PageRank algorithm
    #[arg(long, value_name = "K", requires = "pagerank")]
    top_k: Option<usize>,

    /// Name of the symbol/function to query
    #[arg(long, value_name = "NAME")]
    query: Option<String>,

    /// Search recursively for transitive dependencies (default: direct dependencies only)
    #[arg(long, default_value_t = false)]
    recursive: bool,

    /// Return blocks that the queried symbol depends on
    #[arg(long, default_value_t = false, conflicts_with = "dependents")]
    depends: bool,

    /// Return blocks that depend on the queried symbol
    #[arg(long, default_value_t = false, conflicts_with = "depends")]
    dependents: bool,
}

pub fn run(args: Args) -> Result<()> {
    if args.query.is_none() && (args.depends || args.dependents) {
        eprintln!("Warning: --depends/--dependents flags are ignored without --query");
    }

    if args.pagerank && !args.design_graph {
        return Err(anyhow!("--pagerank requires --design-graph"));
    }

    let opts = LlmccOptions {
        files: args.files,
        dirs: args.dirs,
        print_ir: args.print_ir,
        print_block: args.print_block,
        design_graph: args.design_graph,
        pagerank: args.pagerank,
        top_k: args.top_k,
        query: args.query,
        depends: args.depends,
        dependents: args.dependents,
        recursive: args.recursive,
        summary: args.summary,
    };

    let result = match args.lang.as_str() {
        "rust" => run_main::<LangRust>(&opts),
        "python" => run_main::<LangPython>(&opts),
        _ => Err(format!("Unknown language: {}", args.lang).into()),
    };

    if let Ok(Some(output)) = result {
        println!("{output}");
    }
    Ok(())
}

pub fn main() -> Result<()> {
    let args = Args::parse();
    run(args)
}