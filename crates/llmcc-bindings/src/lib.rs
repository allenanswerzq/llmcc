use llmcc::{run_main, LlmccOptions, QueryDirection};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_python::LangPython;
use llmcc_rust::LangRust;
use pyo3::{exceptions::PyValueError, prelude::*, wrap_pyfunction};
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
    files: Option<Vec<String>>,
    dir_paths: Option<Vec<String>>,
    print_ir: bool,
    print_block: bool,
    print_design_graph: bool,
    pagerank: bool,
    top_k: Option<usize>,
    query: Option<String>,
    recursive: bool,
    dependents: bool,
    summary: bool,
) -> Result<Option<String>, Box<dyn Error>>
where
    L: LanguageTrait,
{
    let opts = LlmccOptions {
        files: files.unwrap_or_default(),
        dirs: dir_paths.unwrap_or_default(),
        print_ir,
        print_block,
        design_graph: print_design_graph,
        pagerank,
        top_k,
        query,
        query_direction: if dependents {
            QueryDirection::Dependents
        } else {
            QueryDirection::Depends
        },
        recursive,
        summary,
    };

    run_main::<L>(&opts)
}

#[pyfunction]
#[pyo3(signature = (
    lang,
    files=None,
    dirs=None,
    print_ir=false,
    print_block=false,
    print_design_graph=false,
    pagerank=false,
    top_k=None,
    query=None,
    recursive=false,
    dependents=false,
    summary=false
))]
fn run_llmcc(
    lang: &str,
    files: Option<Vec<String>>,
    dirs: Option<Vec<String>>,
    print_ir: bool,
    print_block: bool,
    print_design_graph: bool,
    pagerank: bool,
    top_k: Option<usize>,
    query: Option<String>,
    recursive: bool,
    dependents: bool,
    summary: bool,
) -> PyResult<Option<String>> {
    let result = match lang {
        "rust" => run_workflow::<LangRust>(
            files.clone(),
            dirs.clone(),
            print_ir,
            print_block,
            print_design_graph,
            pagerank,
            top_k,
            query.clone(),
            recursive,
            dependents,
            summary,
        ),
        "python" => run_workflow::<LangPython>(
            files.clone(),
            dirs.clone(),
            print_ir,
            print_block,
            print_design_graph,
            pagerank,
            top_k,
            query.clone(),
            recursive,
            dependents,
            summary,
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
