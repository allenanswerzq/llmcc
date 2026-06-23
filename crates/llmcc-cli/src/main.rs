use clap::{ArgGroup, Args, CommandFactory, Parser};

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(target_env = "msvc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use llmcc::Runner;
use llmcc::RunnerOptions;
use llmcc_core::{Result, SupportedLang, ViewDepth};

#[derive(Args, Debug)]
#[command(group = ArgGroup::new("inputs").required(true).args(["files", "dirs"]))]
struct InputArgs {
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
}

#[derive(Args, Debug)]
struct RenderArgs {
    /// Print intermediate representation (IR)
    #[arg(long)]
    print_ir: bool,

    /// Print basic block graph
    #[arg(long)]
    print_block: bool,

    /// Render a DOT graph for visualization (default behavior)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    graph: bool,

    /// Component grouping depth for graph visualization (0=project, 1=package, 2=namespace, 3=file)
    #[arg(long = "depth", default_value_t = 3)]
    view_depth: usize,

    /// Show only top K nodes by PageRank score
    #[arg(long = "top-k")]
    top_k: Option<usize>,

    /// Output optimized for AI agents (no visual styling)
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    ai: bool,

    /// Emit a flat DOT graph without subgraph clusters
    #[arg(long = "flat", default_value_t = true, action = clap::ArgAction::Set)]
    flat: bool,
}

#[derive(Parser, Debug)]
#[command(
    name = "llmcc",
    about = "llmcc: multi-depth architecture views for code understanding and generation in extremely fast speed",
    version
)]
pub struct Cli {
    #[command(flatten)]
    input: InputArgs,

    /// Language to use: rust, typescript (ts), cpp (c++, c)
    #[arg(long, value_name = "LANG", default_value = "rust")]
    lang: SupportedLang,

    #[command(flatten)]
    render: RenderArgs,

    /// Output file path (writes to file instead of stdout)
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output: Option<String>,
}

impl Cli {
    fn into_runner(self) -> Runner {
        let Cli {
            input,
            lang,
            render,
            output,
        } = self;

        let options = RunnerOptions {
            files: input.files,
            dirs: input.dirs,
            output,
            print_ir: render.print_ir,
            print_block: render.print_block,
            graph: render.graph,
            view_depth: ViewDepth::from_repr(render.view_depth as u8).unwrap_or_default(),
            top_k: render.top_k,
            ai: render.ai,
            flat: render.flat,
        };

        Runner::new(lang, options)
    }
}

pub fn main() -> Result<()> {
    println!("llmcc version: {}", env!("CARGO_PKG_VERSION"));

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();

    // Print help if no arguments provided.
    let cli = Cli::try_parse().unwrap_or_else(|e| {
        if std::env::args().len() <= 1 {
            Cli::command().print_help().ok();
            std::process::exit(0);
        }
        e.exit();
    });

    cli.into_runner().execute()
}
