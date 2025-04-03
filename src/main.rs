use clap::Parser as ClapParser;
use proc_macro2::{Ident, Span};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use syn::ItemUse;
use syn::UseTree;
use syn::{PathArguments, PathSegment};

use quote::ToTokens;
use std::env;
use std::string::ToString;
use syn::{Item, ItemFn};

#[derive(Debug, Clone, ClapParser, Serialize, Deserialize)]
struct ReplacementArg {
    from_arg: String,
    to_arg: String,
}

use std::str::FromStr;

impl FromStr for ReplacementArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split the input string at the first '=' character
        let parts: Vec<&str> = s.splitn(2, '=').collect();

        // Check if we have exactly two parts (before and after '=')
        if parts.len() != 2 {
            return Err(format!(
                "Invalid format: '{}'. Expected format: '<string1>=<string2>'",
                s
            ));
        }

        // Create a new ReplacementArg with the parts
        Ok(ReplacementArg {
            from_arg: parts[0].to_string(),
            to_arg: parts[1].to_string(),
        })
    }
}

/// This program does something useful, but its author needs to edit this.
/// Else it will be just hanging around forever
#[derive(Debug, Clone, ClapParser, Serialize, Deserialize)]
#[clap(version = "0.0.1", author = "Andrew Yourtchenko <ayourtch@gmail.com>")]
struct Opts {
    /// File to work on
    #[clap(short, long)]
    file_path: Option<String>,

    /// Path to a json file containing the bulk replacement config to initialize with
    #[clap(long)]
    bulk_replacement_config: Option<String>,

    /// Replacements - simple name to a new name
    #[clap(long)]
    callsite_replace: Vec<ReplacementArg>,

    /// qualified Replacements - full name to a new name
    #[clap(long)]
    callsite_qreplace: Vec<ReplacementArg>,

    /// Replacements - full name to a new name
    #[clap(long)]
    path_replace: Vec<ReplacementArg>,

    /// Replacements - full name to a new name
    #[clap(long)]
    path_qreplace: Vec<ReplacementArg>,

    /// File functions mapping
    #[clap(long)]
    file_function_mappings: Vec<ReplacementArg>,

    /// Override options from this yaml/json file
    #[clap(short, long)]
    options_override: Option<String>,

    /// Write the edited code back
    #[clap(short, long)]
    write: bool,

    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
}

use std::fs;
use syn::{parse_file, visit_mut::VisitMut, Expr, ExprCall, ExprPath, Path};

#[derive(Debug)]
struct CodeReplacer {
    // Map from original function name to new path
    replacements: HashMap<String, String>,
    // Optionally: Map from (module::func) to new path for more specific replacements
    qualified_replacements: HashMap<String, String>,
    import_replacements: HashMap<String, String>,

    // Specific path replacements (highest priority)
    specific_path_replacements: HashMap<String, String>,
    // Crate-level replacements (lower priority)
    crate_replacements: HashMap<String, String>,
    file_function_mappings: HashMap<String, String>,
}

impl CodeReplacer {
    // Helper method to process UseTree nodes recursively
    fn process_use_tree(&mut self, tree: &mut UseTree) {
        match tree {
            // Simple path import: `use some::path;`
            UseTree::Path(use_path) => {
                // Check if the path should be replaced
                let path = &use_path.ident.to_string();

                // Recursively process the subtree
                self.process_use_tree(&mut *use_path.tree);
            }

            // Named import: `use some::path::{self, Name, Other}`
            UseTree::Group(use_group) => {
                // Process each item in the group
                for item in &mut use_group.items {
                    self.process_use_tree(item);
                }
            }

            // Full path: `use some::path::Name;`
            UseTree::Name(use_name) => {
                // Nothing to do here as we process at the Path level
            }

            // Glob import: `use some::path::*;`
            UseTree::Glob(use_glob) => {
                // Nothing to do here as we process at the Path level
            }

            // Rename: `use some::path::Name as OtherName;`
            UseTree::Rename(use_rename) => {
                // Nothing to do here as we process at the Path level
            }
        }
    }

    fn from_config(config_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config_content = fs::read_to_string(config_path)?;
        let config: serde_json::Value = serde_json::from_str(&config_content)?;

        let mut replacements = HashMap::new();
        let mut qualified_replacements = HashMap::new();
        let mut import_replacements = HashMap::new();
        let mut crate_replacements = HashMap::new();
        let mut specific_path_replacements = HashMap::new();
        let mut file_function_mappings = HashMap::new();

        // Parse simple replacements
        if let Some(simple) = config.get("callsite_replace").and_then(|v| v.as_object()) {
            for (key, value) in simple {
                if let Some(value_str) = value.as_str() {
                    replacements.insert(key.clone(), value_str.to_string());
                }
            }
        }

        // Parse qualified replacements
        if let Some(qualified) = config.get("callsite_qreplace").and_then(|v| v.as_object()) {
            for (key, value) in qualified {
                if let Some(value_str) = value.as_str() {
                    qualified_replacements.insert(key.clone(), value_str.to_string());
                }
            }
        }

        // Parse simple replacements
        if let Some(simple) = config.get("path_replace").and_then(|v| v.as_object()) {
            for (key, value) in simple {
                if let Some(value_str) = value.as_str() {
                    crate_replacements.insert(key.clone(), value_str.to_string());
                }
            }
        }

        // Parse qualified replacements
        if let Some(qualified) = config.get("path_qreplace").and_then(|v| v.as_object()) {
            for (key, value) in qualified {
                if let Some(value_str) = value.as_str() {
                    specific_path_replacements.insert(key.clone(), value_str.to_string());
                }
            }
        }

        // Parse import replacements
        if let Some(qualified) = config.get("import_replace").and_then(|v| v.as_object()) {
            for (key, value) in qualified {
                if let Some(value_str) = value.as_str() {
                    import_replacements.insert(key.clone(), value_str.to_string());
                }
            }
        }

        if let Some(qualified) = config
            .get("file_function_mappings")
            .and_then(|v| v.as_object())
        {
            for (key, value) in qualified {
                if let Some(value_str) = value.as_str() {
                    file_function_mappings.insert(key.clone(), value_str.to_string());
                }
            }
        }

        Ok(CodeReplacer {
            replacements,
            qualified_replacements,
            import_replacements,
            crate_replacements,
            specific_path_replacements,
            file_function_mappings,
        })
    }

    fn new() -> Self {
        let mut replacements = HashMap::new();
        // replacements.insert("foobar".to_string(), "newcrate::blah".to_string());
        // replacements.insert("another_func".to_string(), "newcrate::replacement".to_string());

        let mut qualified_replacements = HashMap::new();
        // qualified_replacements.insert("module::foobar".to_string(), "newcrate::specific_blah".to_string());
        let mut import_replacements = HashMap::new();
        let mut specific_path_replacements = HashMap::new();
        // specific_path_replacements.insert("crate1::foo".to_string(), "moo".to_string());
        // specific_path_replacements.insert("crate2::bar::baz".to_string(), "newcrate2::qux".to_string());

        let mut crate_replacements = HashMap::new();
        // crate_replacements.insert("crate1".to_string(), "newcrate1".to_string());
        let mut file_function_mappings = HashMap::new();

        CodeReplacer {
            replacements,
            qualified_replacements,
            import_replacements,
            crate_replacements,
            specific_path_replacements,
            file_function_mappings,
        }
    }

    // Check if this path should be replaced and return the replacement if so
    fn get_replacement(&self, path: &Path) -> Option<String> {
        // Try fully qualified path first for more specific matches
        let full_path = path_to_string(path);
        if let Some(replacement) = self.qualified_replacements.get(&full_path) {
            return Some(replacement.clone());
        }

        // Then try just the function name for more general matches
        if let Some(last_segment) = path.segments.last() {
            let func_name = last_segment.ident.to_string();
            if let Some(replacement) = self.replacements.get(&func_name) {
                return Some(replacement.clone());
            }
        }

        None
    }

    // Get the most appropriate replacement for a path
    fn get_generic_replacement(
        &self,
        path_str: &str,
        specific_path_replacements: &HashMap<String, String>,
        maybe_root_replacements: Option<&HashMap<String, String>>,
    ) -> Option<String> {
        // println!("TRY REPLACING: {}", &path_str);
        let path_segments: Vec<String> = path_str.split("::").map(|s| s.to_owned()).collect();

        // First, check for specific path replacements
        if let Some(replacement) = specific_path_replacements.get(path_str) {
            return Some(replacement.clone());
        }

        // Then, check for partial path matches from the start
        for len in (1..path_segments.len()).rev() {
            let partial_path = path_segments[0..len].join("::");
            // println!("TRY: {}", &partial_path);
            if let Some(replacement) = specific_path_replacements.get(&partial_path) {
                // Found a partial match - replace prefix and keep the rest
                let remaining_path = path_segments[len..].join("::");
                if remaining_path.is_empty() {
                    return Some(replacement.clone());
                } else {
                    return Some(format!("{}::{}", replacement, remaining_path));
                }
            }
        }

        if let Some(root_replacements) = maybe_root_replacements {
            // If no specific match is found, try crate-level replacements
            if let Some(first_segment) = path_segments.first() {
                let crate_name = first_segment.to_string();
                if let Some(crate_replacement) = root_replacements.get(&crate_name) {
                    // Replace just the crate part of the path
                    let mut path_parts = path_segments.clone();

                    // Replace the first part (crate name) with its replacement
                    path_parts[0] = crate_replacement.clone();

                    return Some(path_parts.join("::"));
                }
            }
        }

        None
    }
    fn get_path_replacement(&self, path: &str) -> Option<String> {
        self.get_generic_replacement(
            path,
            &self.specific_path_replacements,
            Some(&self.crate_replacements),
        )
    }

    fn get_import_replacement(&self, path: &str) -> Option<String> {
        // println!("GET IMPORT REPLACEMENT: {}", path);
        // self.import_replacements.get(path).cloned()
        let ret = self.get_generic_replacement(path, &self.import_replacements, None);
        // println!("REPLACEMENT: {:?}", &ret);
        ret
    }

    /// Converts a String into a Syn Path
    ///
    /// This function takes a dot-separated path string (like "std::collections::HashMap")
    /// and converts it into a Syn Path structure.
    fn string_to_path(path_str: &str) -> Path {
        // Split the path string by "::" to get segments
        let segments: Vec<PathSegment> = path_str
            .split("::")
            .map(|segment| PathSegment {
                ident: Ident::new(segment, Span::call_site()),
                arguments: PathArguments::None,
            })
            .collect();

        Path {
            leading_colon: None,
            segments: segments.into_iter().collect(),
        }
    }

    // Helper to extract the full path from a use tree
    fn extract_use_path_str(&self, tree: &UseTree, prefix: &str) -> Option<String> {
        match tree {
            UseTree::Path(use_path) => {
                let new_prefix = if prefix.is_empty() {
                    use_path.ident.to_string()
                } else {
                    format!("{}::{}", prefix, use_path.ident)
                };
                self.extract_use_path_str(&use_path.tree, &new_prefix)
            }
            UseTree::Name(use_name) => {
                if prefix.is_empty() {
                    Some(use_name.ident.to_string())
                } else {
                    Some(format!("{}::{}", prefix, use_name.ident))
                }
            }
            UseTree::Rename(use_rename) => {
                if prefix.is_empty() {
                    Some(use_rename.ident.to_string())
                } else {
                    Some(format!("{}::{}", prefix, use_rename.ident))
                }
            }
            UseTree::Glob(_) => {
                if prefix.is_empty() {
                    Some(format!("*"))
                } else {
                    Some(format!("{}::*", prefix))
                }
            }
            &UseTree::Group(ref x) => {
                // println!("GROUP: {:?}", &x);
                let values = x
                    .clone()
                    .items
                    .into_iter()
                    .map(|item| self.extract_use_path_str(&item, prefix))
                    .filter_map(|x| x)
                    .collect::<Vec<String>>()
                    .join(", ");
                // println!("VALUES: {}", &values);
                Some(format!("{{ {} }}", &values))
            }
        }
    }
    fn extract_use_path(&self, tree: &UseTree, prefix: &str) -> Option<Path> {
        self.extract_use_path_str(tree, prefix)
            .map(|s| Self::string_to_path(&s))
    }
}

impl VisitMut for CodeReplacer {
    fn visit_expr_call_mut(&mut self, node: &mut ExprCall) {
        // First visit any inner call expressions
        syn::visit_mut::visit_expr_call_mut(self, node);

        // Check if this is a call to a function we want to replace
        if let Expr::Path(ExprPath { path, .. }) = &mut *node.func {
            if let Some(replacement) = self.get_replacement(path) {
                // Replace with the new path
                *path = syn::parse_str::<Path>(&replacement).unwrap();
            }
        }
    }

    // Now properly handle use tree replacements
    fn visit_use_tree_mut(&mut self, node: &mut UseTree) {
        let node_copy = node.clone();
        // println!("USE_TREE: {:?}", &node_copy);
        match node {
            UseTree::Path(ref mut use_path) => {
                // First, get the full path up to this point
                if let Some(path_str) = self.extract_use_path_str(&node_copy, "") {
                    // Check if this path should be completely replaced
                    if let Some(replacement) = self.get_import_replacement(&path_str) {
                        // Replace the entire use tree
                        *node = syn::parse_str::<UseTree>(&replacement).unwrap();
                        println!("Replace the node, new node: {:?}", &node);
                        return; // Skip further visitation
                    }
                }

                // Otherwise, continue with normal visitation
                // self.visit_use_tree_mut(&mut *use_path.tree);
            }
            UseTree::Group(use_group) => {
                // Process each item in the group
                let mut i = 0;
                while i < use_group.items.len() {
                    // Extract the full path for this item
                    if let Some(path_str) = self.extract_use_path_str(&use_group.items[i], "") {
                        if let Some(replacement) = self.get_import_replacement(&path_str) {
                            // Replace this item
                            use_group.items[i] = syn::parse_str::<UseTree>(&replacement).unwrap();
                        }
                    }

                    // Visit the item for nested replacements
                    self.visit_use_tree_mut(&mut use_group.items[i]);
                    i += 1;
                }
            }
            UseTree::Name(use_name) => {
                // Extract the full path including this name
                if let Some(path_str) = self.extract_use_path_str(node, "") {
                    if let Some(replacement) = self.get_import_replacement(&path_str) {
                        // Replace with the new use tree
                        *node = syn::parse_str::<UseTree>(&replacement).unwrap();
                    }
                }
            }
            UseTree::Rename(use_rename) => {
                // Similar to Name, but with rename info
                if let Some(parent_path) = self.extract_use_path(node, "") {
                    let path_str = path_to_string(&parent_path);
                    if let Some(replacement) = self.get_import_replacement(&path_str) {
                        // Replace with the new use tree
                        *node = syn::parse_str::<UseTree>(&replacement).unwrap();
                    }
                }
            }
            UseTree::Glob(_) => {
                // No specific replacements for glob imports
            }
        }
    }
    // Handle all paths, which will cover both use statements and function calls
    fn visit_path_mut(&mut self, path: &mut Path) {
        let path_str = path_to_string(&path.clone());
        if let Some(replacement) = self.get_path_replacement(&path_str) {
            // Parse the replacement string into a new Path
            *path = syn::parse_str::<Path>(&replacement).unwrap();
        } else {
            // Continue visiting child paths
            syn::visit_mut::visit_path_mut(self, path);
        }
    }
}
/*
// Helper function to convert a syn::Path to a string representation
fn path_to_string(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}
*/

fn path_to_string(path: &syn::Path) -> String {
    path.to_token_stream().to_string()
}

fn print_function_details(function: &ItemFn) {
    println!("Function signature: {}", function.sig.ident);

    // Print parameters
    println!("Parameters:");
    for input in &function.sig.inputs {
        println!("  {:?}", input);
    }

    // Print return type if specified
    if let syn::ReturnType::Type(_, ref ty) = function.sig.output {
        println!("Return type: {:?}", ty);
    } else {
        println!("Return type: ()");
    }

    // Print if the function is async
    println!("Is async: {}", function.sig.asyncness.is_some());

    // Print if the function is unsafe
    println!("Is unsafe: {}", function.sig.unsafety.is_some());
}

fn add_file_function_mappings(
    file_path: &str,
    function_prefix: &str,
    function_table: &mut HashMap<String, String>,
) {
    println!(
        "ADDING FUNCTION MAPPINGS: {} => {}",
        file_path, function_prefix
    );
    // Read the file content
    let file_content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(err) => {
            panic!("Error reading file {}: {}", file_path, err);
            return;
        }
    };

    // Parse the file content
    let syntax = match parse_file(&file_content) {
        Ok(syntax) => syntax,
        Err(err) => {
            panic!("Error parsing file {}: {}", file_path, err);
            return;
        }
    };

    for item in syntax.items {
        // println!("item: {:?}", &item);
        match item {
            Item::Macro(mac) => {
                let name = path_to_string(&mac.mac.path);
                if name != "macro_rules" {
                    let macro_name = path_to_string(&mac.mac.path);
                    let first_token = mac
                        .mac
                        .tokens
                        .clone()
                        .into_iter()
                        .nth(0)
                        .expect("need the first token")
                        .to_string();

                    if macro_name.starts_with("define_") || macro_name.starts_with("legacy_define_")
                    {
                        let function_match = first_token.clone();
                        let function_replacement =
                            format!("{}::{}", function_prefix, &function_match);
                        function_table.insert(function_match, function_replacement);
                    } else {
                        println!("Skip macro {}", &macro_name);
                    }

                    // println!("Macro: {} = {}", path_to_string(&mac.mac.path), &mac.mac.tokens.clone().into_iter().nth(0).expect("need the first token").to_string());
                    //. println!("All tokens: {} = {}", path_to_string(&mac.mac.path), &mac.mac.tokens.to_string());
                    // println!("Full definition: {:#?}", &mac);
                }
            }
            Item::Fn(function) => {
                // println!("Function Ident: {}", &function.sig.ident);
                // Get the span of the function name
                let span = function.sig.ident.span();

                // Get line and column information
                let start = span.start();
                let end = span.end();

                let function_match = function.sig.ident.to_string();
                let function_replacement = format!("{}::{}", function_prefix, &function_match);
                /*
                println!("Function '{}' found at position:", &function_match);
                println!("  Line: {}, Column: {}", start.line, start.column);
                println!("  Ends at Line: {}, Column: {}", end.line, end.column);
                */
                function_table.insert(function_match, function_replacement);

                if function.sig.ident == "foo" {
                    println!("Found function foo in {}", file_path);
                    print_function_details(&function);
                    return;
                }
            }
            _ => {}
        }
    }
}

fn perform_replacements(file_path: &str, opts: &Opts) {
    let file_content = std::fs::read_to_string(file_path).unwrap();

    // Parse the file
    let mut syntax = parse_file(&file_content).unwrap();

    // Apply modifications with our HashMap-based replacer
    let mut replacer = if let Some(ref cpath) = opts.bulk_replacement_config {
        CodeReplacer::from_config(&cpath).expect("Could not parse the replacer config")
    } else {
        CodeReplacer::new()
    };
    for ia in &opts.callsite_replace {
        replacer
            .replacements
            .insert(ia.from_arg.clone(), ia.to_arg.clone());
    }
    for ia in &opts.callsite_qreplace {
        replacer
            .qualified_replacements
            .insert(ia.from_arg.clone(), ia.to_arg.clone());
    }
    for ia in &opts.path_replace {
        replacer
            .crate_replacements
            .insert(ia.from_arg.clone(), ia.to_arg.clone());
    }
    for ia in &opts.path_qreplace {
        replacer
            .specific_path_replacements
            .insert(ia.from_arg.clone(), ia.to_arg.clone());
    }
    for ia in &opts.file_function_mappings {
        replacer
            .file_function_mappings
            .insert(ia.from_arg.clone(), ia.to_arg.clone());
    }

    for (file_path, prefix) in &replacer.file_function_mappings {
        add_file_function_mappings(file_path, prefix, &mut replacer.qualified_replacements);
    }
    println!("Loaded replacer: {:?}", &replacer);
    syn::visit_mut::visit_file_mut(&mut replacer, &mut syntax);

    // Convert back to string with proper formatting
    let modified_content = prettyplease::unparse(&syntax);

    if opts.write {
        std::fs::write(file_path, modified_content).unwrap();
    } else {
        println!("{}", modified_content);
    }
}

fn main() {
    let opts: Opts = Opts::parse();

    // allow to load the options, so far there is no good built-in way
    let opts = if let Some(fname) = &opts.options_override {
        if let Ok(data) = std::fs::read_to_string(&fname) {
            let res = serde_json::from_str(&data);
            if res.is_ok() {
                res.unwrap()
            } else {
                serde_yaml::from_str(&data).unwrap()
            }
        } else {
            opts
        }
    } else {
        opts
    };

    if opts.verbose > 4 {
        let data = serde_json::to_string_pretty(&opts).unwrap();
        println!("{}", data);
        println!("===========");
        let data = serde_yaml::to_string(&opts).unwrap();
        println!("{}", data);
    }
    if let Some(ref file_path) = opts.file_path {
        perform_replacements(&file_path, &opts);
    }
}
