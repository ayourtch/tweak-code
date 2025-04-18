
This is a semi-useful prototype of a code rewriter for refactoring

# Rust Code Transformer

> **DISCLAIMER**: This README was automatically generated based on code analysis. Most of the code in this project was also automatically generated.

A tool for performing automated refactoring and transformation of Rust code, particularly focused on function call replacements and import path modifications.

## Overview

This utility allows you to replace function calls, import paths, and crate references in Rust source files. It's useful for refactoring code when:

- Moving functions between modules
- Renaming functions or modules
- Migrating from one library to another
- Updating deprecated API calls

The tool uses Rust's `syn` parser to correctly understand and modify Rust code while preserving the original formatting.


### Command Line Options

| Option | Description |
|--------|-------------|
| `-f, --file-path <FILE_PATH>` | Path to the Rust file to transform |
| `--bulk-replacement-config <PATH>` | Path to a JSON file containing the bulk replacement configuration |
| `--callsite-replace <FROM=TO>` | Replace a function name with another at call sites (can be specified multiple times) |
| `--callsite-qreplace <FROM=TO>` | Replace a fully qualified function path with another at call sites (can be specified multiple times) |
| `--path-replace <FROM=TO>` | Replace a crate name with another (can be specified multiple times) |
| `--path-qreplace <FROM=TO>` | Replace a fully qualified path with another (can be specified multiple times) |
| `--file-function-mappings <FILE=PREFIX>` | Map functions defined in a file to a new module prefix (can be specified multiple times) |
| `-o, --options-override <FILE>` | Override options from a YAML/JSON file |
| `-w, --write` | Write the modified code back to the original file (otherwise prints to stdout) |
| `-v, --verbose` | Increase verbosity level (can be used multiple times) |
| `-h, --help` | Print help information |
| `-V, --version` | Print version information |

### Examples

#### Basic Function Replacement

Replace all calls to `old_function` with `new_function`:

```bash
tweak-code --file-path src/main.rs --callsite-replace old_function=new_function
```

#### Qualified Path Replacement

Replace a fully qualified path:

```bash
tweak-code --file-path src/main.rs --callsite-qreplace module::old_function=new_module::new_function
```

#### Crate Replacement

Replace all imports from one crate with another:

```bash
tweak-code --file-path src/main.rs --path-replace old_crate=new_crate
```

#### Bulk Replacement Using Config File

Create a JSON configuration file with multiple replacements:

```json
{
  "callsite_replace": {
    "old_function": "new_function",
    "another_old": "another_new"
  },
  "callsite_qreplace": {
    "module::function": "new_module::new_function"
  },
  "path_replace": {
    "old_crate": "new_crate"
  },
  "path_qreplace": {
    "old_crate::module": "new_crate::new_module"
  },
  "import_replace": {
    "old_import::path": "new_import::path"
  },
  "file_function_mappings": {
    "src/file.rs": "new_module"
  }
}
```

Then run:

```bash
rust-code-transformer --file-path src/main.rs --bulk-replacement-config replacements.json
```

#### Write Changes Back to File

To modify the file in-place:

```bash
rust-code-transformer --file-path src/main.rs --callsite-replace old_function=new_function --write
```

## Replacement Types Explained

- **callsite-replace**: Replaces function names at call sites based on the function name only
- **callsite-qreplace**: Replaces function calls based on fully qualified paths
- **path-replace**: Replaces crate names in import paths and function calls
- **path-qreplace**: Replaces specific fully qualified paths
- **file-function-mappings**: Maps all functions from a source file to a new module prefix

## How It Works

The tool:
1. Parses the source file using Rust's `syn` parser
2. Visits all nodes in the syntax tree (function calls, paths, imports)
3. Applies the specified replacements
4. Preserves the original formatting using `prettyplease`
5. Outputs the modified code to stdout or writes it back to the file

## Bulk Replacement Configuration

The JSON configuration file for bulk replacements supports the following sections:

- `callsite_replace`: Simple function name replacements
- `callsite_qreplace`: Qualified function path replacements
- `path_replace`: Crate name replacements
- `path_qreplace`: Specific fully qualified path replacements
- `import_replace`: Import path replacements
- `file_function_mappings`: Map functions from files to new module prefixes

## Dependencies

This tool relies on the following Rust crates:
- `syn`: For parsing and modifying Rust code
- `quote`: For token stream handling
- `proc-macro2`: For identifier manipulation
- `clap`: For command-line argument parsing
- `serde`: For JSON/YAML configuration parsing
- `prettyplease`: For formatting the modified code

## License

MIT
