use llmcc::{run_main, LlmccOptions};
use llmcc_python::LangPython;
use llmcc_rust::LangRust;
use pyo3::{exceptions::PyValueError, prelude::*, wrap_pyfunction};

/// Main llmcc module interface - Direct Rust API exposure
#[pymodule]
fn llmcc_bindings(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_llmcc, m)?)?;

    // Version info
    m.add("VERSION", env!("CARGO_PKG_VERSION"))?;

    Ok(())
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
    depends=false,
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
    depends: bool,
    dependents: bool,
    summary: bool,
) -> PyResult<Option<String>> {
    if depends && dependents {
        return Err(PyErr::new::<PyValueError, _>(
            "'depends' and 'dependents' are mutually exclusive",
        ));
    }

    let opts = LlmccOptions {
        files: files.unwrap_or_default(),
        dirs: dirs.unwrap_or_default(),
        print_ir,
        print_block,
        design_graph: print_design_graph,
        pagerank,
        top_k,
        query,
        depends,
        dependents,
        recursive,
        summary,
    };

    let result = match lang {
        "rust" => run_main::<LangRust>(&opts),
        "python" => run_main::<LangPython>(&opts),
        other => {
            return Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown language: {}. Use 'rust' or 'python'",
                other
            )));
        }
    };

    result.map_err(|err| PyErr::new::<PyValueError, _>(err.to_string()))
}
