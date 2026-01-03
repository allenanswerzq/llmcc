use std::time::Instant;

use anyhow::Result;
use clap::ArgGroup;
use clap::Parser;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use llmcc_cli::LlmccOptions;
use llmcc_cli::run_main;
use llmcc_dot::ComponentDepth;
use llmcc_rust::LangRust;

#[derive(Parser, Debug)]
#[command(
    name = "llmcc",
    about = "llmcc: zoom in, zoom out, understand everything",
    version,
    group = ArgGroup::new("inputs").required(true).args(["files", "dirs"])
)]
pub struct Cli {
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

    /// Print intermediate representation (IR)
    #[arg(long, default_value_t = false)]
    print_ir: bool,

    /// Print basic block graph
    #[arg(long, default_value_t = false)]
    print_block: bool,

    /// Render a DOT graph for visualization
    #[arg(long, default_value_t = false)]
    graph: bool,

    /// Component grouping depth for graph visualization (0=flat, 1=crate, 2=module, 3=file)
    #[arg(long = "depth", default_value = "3")]
    component_depth: usize,

    /// Show only top K nodes by PageRank score
    #[arg(long = "pagerank-top-k")]
    pagerank_top_k: Option<usize>,

    /// Cluster modules by their parent crate (for module-level graphs)
    #[arg(long = "cluster-by-crate")]
    cluster_by_crate: bool,

    /// Use shortened labels (module name only, without crate prefix)
    #[arg(long = "short-labels")]
    short_labels: bool,

    /// Output file path (writes to file instead of stdout)
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output: Option<String>,
}

pub fn run(args: Cli) -> Result<()> {
    let total_start = Instant::now();

    // Initialize tracing subscriber for logging
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();
    }

    let opts = LlmccOptions {
        files: args.files,
        dirs: args.dirs,
        output: args.output.clone(),
        print_ir: args.print_ir,
        print_block: args.print_block,
        graph: args.graph,
        component_depth: ComponentDepth::from_number(args.component_depth),
        pagerank_top_k: args.pagerank_top_k,
        cluster_by_crate: args.cluster_by_crate,
        short_labels: args.short_labels,
    };

    let result = match args.lang.as_str() {
        "rust" => run_main::<LangRust>(&opts),
        _ => Err(format!("Unknown language: {}", args.lang).into()),
    };

    if let Ok(Some(output)) = result {
        if let Some(ref path) = args.output {
            std::fs::write(path, &output)?;
            tracing::info!("output written to: {}", path);
        } else {
            println!("{output}");
        }
    }

    tracing::info!("Total time: {:.2}s", total_start.elapsed().as_secs_f64());
    Ok(())
}

pub fn main() -> Result<()> {
    let args = Cli::parse();
    run(args)
}
