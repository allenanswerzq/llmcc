#![allow(clippy::useless_conversion)]
#![allow(unsafe_op_in_unsafe_fn)]
use llmcc_cli::{LlmccOptions, run_main};
use llmcc_dot::ComponentDepth;
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
    graph=false,
    component_depth="crate"
))]
fn run_llmcc(
    lang: &str,
    files: Option<Vec<String>>,
    dirs: Option<Vec<String>>,
    print_ir: bool,
    print_block: bool,
    graph: bool,
    component_depth: &str,
) -> Result<Option<String>, PyErr> {
    let depth = match component_depth {
        "crate" => ComponentDepth::Crate,
        "module" => ComponentDepth::Module,
        "file" => ComponentDepth::File,
        other => {
            return Err(PyValueError::new_err(format!(
                "Unknown component_depth: {other}. Use 'crate', 'module', or 'file'"
            )));
        }
    };

    let opts = LlmccOptions {
        files: files.unwrap_or_default(),
        dirs: dirs.unwrap_or_default(),
        output: None,
        print_ir,
        print_block,
        graph,
        component_depth: depth,
        pagerank_top_k: None,
        cluster_by_crate: false,
        short_labels: false,
    };

    let result = match lang {
        "rust" => run_main::<LangRust>(&opts),
        // "python" => run_main::<LangPython>(&opts),  // TODO: will be added back in the future
        other => {
            return Err(PyValueError::new_err(format!(
                "Unknown language: {other}. Use 'rust'"
            )));
        }
    };

    result.map_err(|err| PyValueError::new_err(err.to_string()))
}
