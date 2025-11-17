#![allow(clippy::useless_conversion)]
#![allow(unsafe_op_in_unsafe_fn)]
use llmcc::{LlmccOptions, run_main};
// use llmcc_python::LangPython;  // TODO: will be added back in the future
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
#[allow(clippy::too_many_arguments)]
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
) -> Result<Option<String>, PyErr> {
    if depends && dependents {
        return Err(PyValueError::new_err(
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
        // "python" => run_main::<LangPython>(&opts),  // TODO: will be added back in the future
        other => {
            return Err(PyValueError::new_err(format!(
                "Unknown language: {}. Use 'rust'",
                other
            )));
        }
    };

    result.map_err(|err| PyValueError::new_err(err.to_string()))
}
