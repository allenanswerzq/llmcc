use std::collections::HashSet;
use std::error::Error;
use std::io::{self, ErrorKind};

use ignore::WalkBuilder;

use llmcc_core::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryDirection {
    Depends,
    Dependents,
}

pub struct LlmccOptions {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub print_ir: bool,
    pub print_block: bool,
    pub design_graph: bool,
    pub pagerank: bool,
    pub top_k: Option<usize>,
    pub query: Option<String>,
    pub query_direction: QueryDirection,
    pub recursive: bool,
}

pub fn run_main<L: LanguageTrait>(opts: &LlmccOptions) -> Result<Option<String>, Box<dyn Error>> {
    if !opts.files.is_empty() && !opts.dirs.is_empty() {
        return Err("Specify either --file or --dir, not both".into());
    }

    let mut seen = HashSet::new();
    let mut requested_files = Vec::new();

    let mut add_path = |path: String| {
        if seen.insert(path.clone()) {
            requested_files.push(path);
        }
    };

    for file in &opts.files {
        add_path(file.clone());
    }

    if !opts.dirs.is_empty() {
        let supported_exts = L::supported_extensions();
        for dir in &opts.dirs {
            let walker = WalkBuilder::new(dir).standard_filters(true).build();
            for entry in walker {
                let entry = entry.map_err(|e| {
                    io::Error::new(
                        ErrorKind::Other,
                        format!("Failed to walk directory {dir}: {e}"),
                    )
                })?;

                if !entry
                    .file_type()
                    .map(|file_type| file_type.is_file())
                    .unwrap_or(false)
                {
                    continue;
                }

                let path = entry.path();
                let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                    continue;
                };

                if supported_exts.contains(&ext) {
                    add_path(path.to_string_lossy().into_owned());
                }
            }
        }
    }

    if requested_files.is_empty() {
        return Err("No input files provided. Use --file or --dir.".into());
    }

    let cc = CompileCtxt::from_files::<L>(&requested_files)?;
    let files = cc.get_files();

    let use_compact_builder = opts.design_graph && opts.query.is_none();

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
    let graph_config = if use_compact_builder {
        GraphBuildConfig::compact()
    } else {
        GraphBuildConfig::default()
    };

    for (index, _) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph_with_config::<L>(unit, index, graph_config)?;

        if opts.print_block {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        pg.add_child(unit_graph);
    }

    pg.link_units();

    let mut outputs = Vec::new();

    if opts.design_graph {
        if opts.pagerank {
            let limit = Some(opts.top_k.unwrap_or(25));
            pg.set_compact_rank_limit(limit);
        }
        outputs.push(pg.render_compact_graph());
    } else if let Some(name) = opts.query.as_ref() {
        let query = ProjectQuery::new(&pg);
        let query_output = match opts.query_direction {
            QueryDirection::Dependents => {
                if opts.recursive {
                    query.find_depended_recursive(name).format_for_llm()
                } else {
                    query.find_depended(name).format_for_llm()
                }
            }
            QueryDirection::Depends => {
                if opts.recursive {
                    query.find_depends_recursive(name).format_for_llm()
                } else {
                    query.find_depends(name).format_for_llm()
                }
            }
        };
        outputs.push(query_output);
    }

    if outputs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(outputs.join("\n")))
    }
}
