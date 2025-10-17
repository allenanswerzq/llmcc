use clap::Parser;
use llmcc_rust::*;
use std::time::Instant;

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
    let start_total = Instant::now();
    let mut args = Args::parse();

    // Handle negation flags
    if args.no_print_ir {
        args.print_ir = false;
    }
    if args.no_print_graph {
        args.print_graph = false;
    }

    // Step 1: Load context and files
    let step1_start = Instant::now();
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
    let step1_duration = step1_start.elapsed();
    eprintln!("[TIMING] Step 1 (Load context): {:.3}ms", step1_duration.as_secs_f64() * 1000.0);

    // Step 2: Create globals
    let step2_start = Instant::now();
    let globals = cc.create_globals();
    let step2_duration = step2_start.elapsed();
    eprintln!("[TIMING] Step 2 (Create globals): {:.3}ms", step2_duration.as_secs_f64() * 1000.0);

    // Step 3: Build IR and collect symbols
    let step3_start = Instant::now();
    let mut symbol_collection_durations = Vec::new();
    for (index, path) in files.iter().enumerate() {
        let file_start = Instant::now();
        let unit = cc.compile_unit(index);
        build_llmcc_ir::<LangRust>(unit)?;

        if args.print_ir {
            print_llmcc_ir(unit);
        }

        collect_symbols(unit, globals);
        let file_duration = file_start.elapsed();
        symbol_collection_durations.push((path.clone(), file_duration));
        eprintln!("  [{}] {}: {:.3}ms", index, path, file_duration.as_secs_f64() * 1000.0);
    }
    let step3_duration = step3_start.elapsed();
    eprintln!("[TIMING] Step 3 (Build IR & collect symbols): {:.3}ms", step3_duration.as_secs_f64() * 1000.0);

    // Step 4: Build graph and bind symbols
    let step4_start = Instant::now();
    let mut mut_graph = ProjectGraph::new(&cc);
    let mut binding_durations = Vec::new();
    let mut bind_symbols_total = std::time::Duration::ZERO;
    let mut build_graph_total = std::time::Duration::ZERO;
    
    for (index, path) in files.iter().enumerate() {
        let bind_start = Instant::now();
        let unit = cc.compile_unit(index);
        
        let bind_sym_start = Instant::now();
        bind_symbols(unit, globals);
        let bind_sym_duration = bind_sym_start.elapsed();
        bind_symbols_total += bind_sym_duration;

        let build_graph_start = Instant::now();
        let unit_graph = build_llmcc_graph::<LangRust>(unit, index)?;
        let build_graph_duration = build_graph_start.elapsed();
        build_graph_total += build_graph_duration;

        if args.print_graph {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        mut_graph.add_child(unit_graph);
        let bind_duration = bind_start.elapsed();
        binding_durations.push((path.clone(), bind_duration));
        eprintln!("  [{}] {}: {:.3}ms (bind: {:.3}ms, graph: {:.3}ms)", 
            index, path, bind_duration.as_secs_f64() * 1000.0,
            bind_sym_duration.as_secs_f64() * 1000.0,
            build_graph_duration.as_secs_f64() * 1000.0);
    }
    let step4_duration = step4_start.elapsed();
    eprintln!("[TIMING] Step 4 (Bind symbols & build graph): {:.3}ms", step4_duration.as_secs_f64() * 1000.0);
    eprintln!("  └─ Bind symbols subtotal: {:.3}ms", bind_symbols_total.as_secs_f64() * 1000.0);
    eprintln!("  └─ Build graph subtotal: {:.3}ms", build_graph_total.as_secs_f64() * 1000.0);

    // Step 5: Link units
    let step5_start = Instant::now();
    
    // Time the link_units call
    let link_start = Instant::now();
    mut_graph.link_units();
    let link_duration = link_start.elapsed();
    
    let step5_duration = step5_start.elapsed();
    eprintln!("[TIMING] Step 5 (Link units): {:.3}ms", step5_duration.as_secs_f64() * 1000.0);
    eprintln!("  └─ link_units() call: {:.3}ms", link_duration.as_secs_f64() * 1000.0);

    // Print summary
    let total_duration = start_total.elapsed();
    eprintln!("\n=== TIMING SUMMARY ===");
    eprintln!("Step 1 (Load context):          {:.3}ms", step1_duration.as_secs_f64() * 1000.0);
    eprintln!("Step 2 (Create globals):        {:.3}ms", step2_duration.as_secs_f64() * 1000.0);
    eprintln!("Step 3 (Build IR & symbols):    {:.3}ms", step3_duration.as_secs_f64() * 1000.0);
    eprintln!("Step 4 (Bind symbols & graph):  {:.3}ms (bind: {:.3}ms, graph: {:.3}ms)", 
        step4_duration.as_secs_f64() * 1000.0,
        bind_symbols_total.as_secs_f64() * 1000.0,
        build_graph_total.as_secs_f64() * 1000.0);
    eprintln!("Step 5 (Link units):            {:.3}ms", step5_duration.as_secs_f64() * 1000.0);
    eprintln!("---");
    eprintln!("TOTAL:                          {:.3}ms", total_duration.as_secs_f64() * 1000.0);
    eprintln!("\n=== BREAKDOWN ===");
    eprintln!("Parsing (Step 1):               {:.1}%", (step1_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0);
    eprintln!("Symbol collection (Step 3):     {:.1}%", (step3_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0);
    eprintln!("Symbol binding (Step 4):        {:.1}%", (bind_symbols_total.as_secs_f64() / total_duration.as_secs_f64()) * 100.0);
    eprintln!("Graph building (Step 4):        {:.1}%", (build_graph_total.as_secs_f64() / total_duration.as_secs_f64()) * 100.0);
    eprintln!("Cross-unit linking (Step 5):    {:.1}%", (step5_duration.as_secs_f64() / total_duration.as_secs_f64()) * 100.0);

    Ok(())
}
