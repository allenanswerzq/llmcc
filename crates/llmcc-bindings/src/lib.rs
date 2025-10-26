use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use llmcc_core::{
    build_llmcc_graph_with_config, build_llmcc_ir, lang_def::LanguageTrait, print_llmcc_graph,
    print_llmcc_ir, CompileCtxt, GraphBuildConfig, ProjectGraph, ProjectQuery,
};
use llmcc_python::LangPython;
use llmcc_rust::LangRust;
use std::error::Error;

/// Main llmcc module interface - Direct Rust API exposure
#[pymodule]
fn llmcc_bindings(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_llmcc, m)?)?;

    // Version info
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}

fn run_workflow<L>(
    lang_label: &str,
    files: Option<Vec<String>>,
    dir: Option<String>,
    print_ir: bool,
    print_block: bool,
    print_project_graph: bool,
    query: Option<String>,
    recursive: bool,
    dependents: bool,
) -> Result<Option<String>, Box<dyn Error>>
where
    L: LanguageTrait,
{
    if recursive && dependents {
        return Err("Recursive queries are not yet supported for dependents".into());
    }

    let (cc, files) = if let Some(dir_path) = dir {
        eprintln!(" loading {lang_label} files from directory: {}", dir_path);
        let ctx = CompileCtxt::from_dir::<_, L>(dir_path.as_str())?;
        let loaded_files = ctx.get_files();
        if loaded_files.is_empty() {
            return Err(format!("No source files found in {dir_path}").into());
        }
        eprintln!(" found {} files", loaded_files.len());
        (ctx, loaded_files)
    } else {
        let Some(raw_files) = files else {
            return Err("No input files provided".into());
        };
        if raw_files.is_empty() {
            return Err("Input file list is empty".into());
        }
        let ctx = CompileCtxt::from_files::<L>(&raw_files)?;
        (ctx, raw_files)
    };

    build_llmcc_ir::<L>(&cc)?;

    let globals = cc.create_globals();

    if print_ir {
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
    let graph_config = if print_project_graph {
        GraphBuildConfig::compact()
    } else {
        GraphBuildConfig::default()
    };
    for (index, _) in files.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals);
        let unit_graph = build_llmcc_graph_with_config::<L>(unit, index, graph_config)?;

        if print_block {
            print_llmcc_graph(unit_graph.root(), unit);
        }

        pg.add_child(unit_graph);
    }

    pg.link_units();

    let mut outputs = Vec::new();

    if print_project_graph {
        outputs.push(pg.render_compact_graph());
    }

    if let Some(symbol_name) = query {
        let query = ProjectQuery::new(&pg);
        let result = if dependents {
            query.find_depended(&symbol_name)
        } else if recursive {
            query.find_depends_recursive(&symbol_name)
        } else {
            query.find_depends(&symbol_name)
        };
        outputs.push(result.format_for_llm());
    }

    if outputs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(outputs.join("\n")))
    }
}

#[pyfunction]
#[pyo3(signature = (lang, files=None, dir=None, print_ir=false, print_block=false, print_project_graph=false, query=None, recursive=false, dependents=false))]
fn run_llmcc(
    lang: &str,
    files: Option<Vec<String>>,
    dir: Option<String>,
    print_ir: bool,
    print_block: bool,
    print_project_graph: bool,
    query: Option<String>,
    recursive: bool,
    dependents: bool,
) -> PyResult<Option<String>> {
    let result = match lang {
        "rust" => run_workflow::<LangRust>(
            "rust",
            files.clone(),
            dir.clone(),
            print_ir,
            print_block,
            print_project_graph,
            query.clone(),
            recursive,
            dependents,
        ),
        "python" => run_workflow::<LangPython>(
            "python",
            files.clone(),
            dir.clone(),
            print_ir,
            print_block,
            print_project_graph,
            query.clone(),
            recursive,
            dependents,
        ),
        other => {
            return Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown language: {}. Use 'rust' or 'python'",
                other
            )));
        }
    };

    result.map_err(|err| PyErr::new::<PyValueError, _>(err.to_string()))
}
