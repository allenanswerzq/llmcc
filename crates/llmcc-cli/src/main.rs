use std::time::Instant;

use clap::ArgGroup;
use clap::{Parser, ValueEnum};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use llmcc::LlmccOptions;
use llmcc::OutputFormat;
use llmcc::run_main;
use llmcc_core::Result;
use llmcc_cpp::LangCpp;
use llmcc_dot::ComponentDepth;
use llmcc_go::LangGo;
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

    /// Language to use: 'rust', 'typescript' (or 'ts'), 'cpp', 'go'
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

    /// Output format for agent-native reports
    #[arg(long = "format", value_enum)]
    format: Option<CliOutputFormat>,

    /// Print a Markdown summary for coding agents
    #[arg(long = "agent-summary", default_value_t = false)]
    agent_summary: bool,

    /// Print package-level dependency table
    #[arg(long = "package-deps", default_value_t = false)]
    package_deps: bool,

    /// Component grouping preset: project, package, module, file
    #[arg(long = "graph-level", value_enum)]
    graph_level: Option<GraphLevel>,

    /// Collapse test files during discovery
    #[arg(long = "collapse-tests", default_value_t = false)]
    collapse_tests: bool,

    /// Include only exported/public nodes in outputs
    #[arg(long = "only-exported", default_value_t = false)]
    only_exported: bool,

    /// Exclude matching paths during discovery. Supports '*' wildcards.
    #[arg(long = "exclude", action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Symbol name for symbol-centered reports
    #[arg(long = "symbol")]
    symbol: Option<String>,

    /// Print a blast-radius report for --symbol
    #[arg(long = "blast-radius", default_value_t = false)]
    blast_radius: bool,

    /// Infer tests for a source file
    #[arg(long = "tests-for")]
    tests_for: Option<String>,

    /// Use git diff --name-only as the primary changed-file set
    #[arg(long = "git-diff", default_value_t = false)]
    git_diff: bool,

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

#[derive(Clone, Debug, ValueEnum)]
pub enum CliOutputFormat {
    Text,
    Json,
    Markdown,
    Dot,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum GraphLevel {
    Project,
    Package,
    Module,
    File,
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

    let component_depth = args
        .graph_level
        .as_ref()
        .map(|level| match level {
            GraphLevel::Project => ComponentDepth::Project,
            GraphLevel::Package => ComponentDepth::Crate,
            GraphLevel::Module => ComponentDepth::Module,
            GraphLevel::File => ComponentDepth::File,
        })
        .unwrap_or_else(|| ComponentDepth::from_number(args.component_depth));

    let output_format = args.format.as_ref().map(|format| match format {
        CliOutputFormat::Text => OutputFormat::Text,
        CliOutputFormat::Json => OutputFormat::Json,
        CliOutputFormat::Markdown => OutputFormat::Markdown,
        CliOutputFormat::Dot => OutputFormat::Dot,
    });

    let opts = LlmccOptions {
        files: args.files,
        dirs: args.dirs,
        output: args.output.clone(),
        print_ir: args.print_ir,
        print_block: args.print_block,
        graph: args.graph,
        component_depth,
        pagerank_top_k: args.pagerank_top_k,
        output_format,
        agent_summary: args.agent_summary,
        package_deps: args.package_deps,
        collapse_tests: args.collapse_tests,
        only_exported: args.only_exported,
        exclude: args.exclude,
        symbol: args.symbol,
        blast_radius: args.blast_radius,
        tests_for: args.tests_for,
        git_diff: args.git_diff,
        cluster_by_crate: args.cluster_by_crate,
        short_labels: args.short_labels,
    };

    let result = match args.lang.as_str() {
        "rust" => run_main::<LangRust>(&opts),
        "typescript" | "ts" => run_main::<LangTypeScript>(&opts),
        "cpp" | "c++" | "c" => run_main::<LangCpp>(&opts),
        "go" | "golang" => run_main::<LangGo>(&opts),
        _ => {
            return Err(format!(
                "Unknown language: {}. Use 'rust', 'typescript', 'cpp', or 'go'",
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
            return Err(e);
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
