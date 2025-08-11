/// Create a comprehensive Python import test script
///
/// This function generates a Python script that attempts to import all provided modules
/// and reports success/failure statistics to stderr. The script handles various types
/// of import errors and provides detailed error reporting.
pub fn create_import_test_script(package_name: &str, modules: &[String]) -> String {
    let mut script = String::new();
    script.push_str("import sys\n");
    script.push_str("import importlib\n");
    script.push_str("import traceback\n");
    script.push('\n');
    script.push_str("failed = []\n");
    script.push_str("succeeded = []\n");
    script.push('\n');

    for module in modules {
        let full_module = if package_name.is_empty() {
            module.clone()
        } else {
            format!("{}.{}", package_name, module)
        };

        script.push_str(&format!(
            r#"
# Test module: {} -> {}
try:
    mod = importlib.import_module('{}')
    succeeded.append('{}')
except ImportError as e:
    import_error = str(e)
    if "relative import" in import_error.lower():
        import_error += " (relative import context issue)"
    failed.append(('{}', 'ImportError: ' + import_error))
except ModuleNotFoundError as e:
    failed.append(('{}', 'ModuleNotFoundError: ' + str(e)))
except SyntaxError as e:
    failed.append(('{}', 'SyntaxError: ' + str(e) + ' at line ' + str(e.lineno or 'unknown')))
except Exception as e:
    tb = traceback.format_exc()
    failed.append(('{}', 'Exception: ' + type(e).__name__ + ': ' + str(e)))
"#,
            module, full_module, full_module, module, module, module, module, module
        ));
    }

    script.push('\n');
    script.push_str("print(f'IMPORT_TEST_SUMMARY:succeeded={len(succeeded)},failed={len(failed)},total={len(succeeded)+len(failed)}', file=sys.stderr)\n");
    script.push('\n');
    script.push_str("if failed:\n");
    script.push_str("    for module, error in failed:\n");
    script.push_str("        print(f'IMPORT_ERROR:{module}:{error}', file=sys.stderr)\n");
    script.push_str("    sys.exit(1)\n");
    script.push_str("else:\n");
    script.push_str(
        "    print('IMPORT_TEST_SUCCESS:all_modules_imported_successfully', file=sys.stderr)\n",
    );

    script
}
