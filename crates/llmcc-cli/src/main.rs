use std::time::Instant;

use clap::ArgGroup;
use clap::Parser;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use llmcc::LlmccOptions;
use llmcc::run_main;
use llmcc_core::Result;
use llmcc_cpp::LangCpp;
use llmcc_dot::ComponentDepth;
use llmcc_py::LangPython;
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;

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

    /// Language to use: 'rust', 'typescript' (or 'ts'), 'cpp', 'python' (or 'py')
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
        "typescript" | "ts" => run_main::<LangTypeScript>(&opts),
        "cpp" | "c++" | "c" => run_main::<LangCpp>(&opts),
        "python" | "py" => run_main::<LangPython>(&opts),
        _ => {
            return Err(format!(
                "Unknown language: {}. Use 'rust', 'typescript', 'cpp', or 'python'",
                args.lang
            )
            .into());
        }
    };

    match result {
        Ok(Some(output)) => {
            if let Some(ref path) = args.output {
                std::fs::write(path, &output)?;
                tracing::info!(path, "output written");
            } else {
                println!("{output}");
            }
        }
        Ok(None) => {
            // No output requested (e.g., print-ir or print-block mode)
        }
        Err(e) => {
            eprintln!("Error: {e}");
            tracing::error!(error = %e, "execution failed");
        }
    }

    let total_secs = total_start.elapsed().as_secs_f64();
    tracing::info!(total_secs, "complete");
    eprintln!("Total time: {total_secs:.2}s");
    Ok(())
}

pub fn main() -> Result<()> {
    let args = Cli::parse();
    run(args)
}
