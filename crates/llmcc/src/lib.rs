use std::error::Error;
use std::path::Path;

use llmcc_core::*;

/// Input options for building an LLMCC project.
pub struct LlmccOptions {
    pub files: Vec<String>,
    pub dir: Option<String>,
    pub print_ir: bool,
    pub print_graph: bool,
    pub compact_graph: bool,
    pub query: Option<String>,
    pub recursive: bool,
    pub dependents: bool,
}

pub fn run_main<L: LanguageTrait>(opts: &LlmccOptions) -> Result<Option<String>, Box<dyn Error>> {
    let (cc, files) = if let Some(dir) = opts.dir.as_ref() {
        let ctx = CompileCtxt::from_dir::<_, L>(Path::new(dir))?;
        let file_paths = ctx.get_files();
        (ctx, file_paths)
    } else {
        let ctx = CompileCtxt::from_files::<L>(&opts.files)?;
        (ctx, opts.files.clone())
    };

    build_llmcc_ir::<L>(&cc)?;
    let globals = cc.create_globals();

    if opts.print_ir {
        for (index, _) in files.iter().enumerate() {
            let unit = cc.compile_unit(index);
            print_llmcc_ir(unit);
        }
    }

    for (index, _) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::collect_symbols(unit, globals);
    }

    let mut pg = ProjectGraph::new(&cc);
    let graph_config = if opts.compact_graph {
        GraphBuildConfig::compact()
    } else {
        GraphBuildConfig::default()
    };
    for (index, _) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph_with_config::<L>(unit, index, graph_config)?;

        if opts.print_graph {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        pg.add_child(unit_graph);
    }

    pg.link_units();

    if opts.recursive && opts.dependents {
        return Err("`--recursive` is not supported together with `--dependents`".into());
    }

    let mut outputs = Vec::new();

    if opts.compact_graph {
        outputs.push(pg.render_compact_graph());
    }

    if let Some(name) = opts.query.as_ref() {
        let query = ProjectQuery::new(&pg);
        let query_output = if opts.dependents {
            query.find_depended(name).format_for_llm()
        } else if opts.recursive {
            query.find_depends_recursive(name).format_for_llm()
        } else {
            query.find_depends(name).format_for_llm()
        };
        outputs.push(query_output);
    }

    if outputs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(outputs.join("\n")))
    }
}
