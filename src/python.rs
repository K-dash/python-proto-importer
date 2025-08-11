#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "python")]
#[allow(clippy::useless_conversion)]
#[pyfunction]
fn main(py: Python<'_>) -> PyResult<i32> {
    // Build argv with a stable program name followed by Python's argv[1:]
    let sys = py.import("sys")?;
    let argv: Vec<String> = sys.getattr("argv")?.extract()?;
    let mut args: Vec<String> = Vec::new();
    if argv.len() > 1 {
        args.extend(argv.iter().skip(1).cloned());
    }
    let iter = std::iter::once("proto-importer".to_string()).chain(args.into_iter());
    match crate::cli::run_cli_with(iter) {
        Ok(()) => Ok(0),
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e.to_string())),
    }
}

#[cfg(feature = "python")]
#[pymodule]
pub fn python_proto_importer(_py: Python<'_>, m: &pyo3::prelude::Bound<PyModule>) -> PyResult<()> {
    m.add_function(pyo3::wrap_pyfunction!(main, m)?)?;
    Ok(())
}
