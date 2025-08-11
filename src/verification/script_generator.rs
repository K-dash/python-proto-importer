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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_import_test_script_empty_modules() {
        let script = create_import_test_script("test_package", &[]);

        assert!(script.contains("import sys"));
        assert!(script.contains("import importlib"));
        assert!(script.contains("import traceback"));
        assert!(script.contains("failed = []"));
        assert!(script.contains("succeeded = []"));
        assert!(script.contains("IMPORT_TEST_SUMMARY"));
        assert!(script.contains("IMPORT_TEST_SUCCESS"));
    }

    #[test]
    fn test_create_import_test_script_single_module() {
        let modules = vec!["test_module".to_string()];
        let script = create_import_test_script("test_package", &modules);

        assert!(script.contains("test_module -> test_package.test_module"));
        assert!(script.contains("importlib.import_module('test_package.test_module')"));
        assert!(script.contains("succeeded.append('test_module')"));
        assert!(script.contains("ImportError"));
        assert!(script.contains("ModuleNotFoundError"));
        assert!(script.contains("SyntaxError"));
    }

    #[test]
    fn test_create_import_test_script_multiple_modules() {
        let modules = vec![
            "module1".to_string(),
            "module2".to_string(),
            "subpkg.module3".to_string(),
        ];
        let script = create_import_test_script("mypackage", &modules);

        assert!(script.contains("module1 -> mypackage.module1"));
        assert!(script.contains("module2 -> mypackage.module2"));
        assert!(script.contains("subpkg.module3 -> mypackage.subpkg.module3"));
        assert!(script.contains("importlib.import_module('mypackage.module1')"));
        assert!(script.contains("importlib.import_module('mypackage.module2')"));
        assert!(script.contains("importlib.import_module('mypackage.subpkg.module3')"));
    }

    #[test]
    fn test_create_import_test_script_empty_package_name() {
        let modules = vec!["standalone_module".to_string()];
        let script = create_import_test_script("", &modules);

        assert!(script.contains("standalone_module -> standalone_module"));
        assert!(script.contains("importlib.import_module('standalone_module')"));
        assert!(script.contains("succeeded.append('standalone_module')"));
    }

    #[test]
    fn test_create_import_test_script_error_handling() {
        let modules = vec!["test_module".to_string()];
        let script = create_import_test_script("pkg", &modules);

        assert!(script.contains("relative import"));
        assert!(script.contains("relative import context issue"));
        assert!(script.contains("failed.append"));
        assert!(script.contains("ImportError:"));
        assert!(script.contains("ModuleNotFoundError:"));
        assert!(script.contains("SyntaxError:"));
        assert!(script.contains("Exception:"));
    }

    #[test]
    fn test_create_import_test_script_output_format() {
        let modules = vec!["mod1".to_string(), "mod2".to_string()];
        let script = create_import_test_script("pkg", &modules);

        assert!(script.contains("IMPORT_TEST_SUMMARY:succeeded="));
        assert!(script.contains(",failed="));
        assert!(script.contains(",total="));
        assert!(script.contains("IMPORT_ERROR:"));
        assert!(script.contains("sys.exit(1)"));
        assert!(script.contains("IMPORT_TEST_SUCCESS:all_modules_imported_successfully"));
    }
}
